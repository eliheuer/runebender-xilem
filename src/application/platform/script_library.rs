// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A bounded directory of ordinary Python recipe files.
//!
//! The library detects observed external edits with content revisions.
//! It does not claim operating-system locking or an atomic compare-and-swap against arbitrary
//! writers that race the final same-directory rename.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

const MAX_SCRIPT_BYTES: usize = 256 * 1024;
const MAX_NAME_BYTES: usize = 128;
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

/// Content and disk identity for one saved recipe.
#[derive(Clone, Debug)]
pub(crate) struct ScriptDocument {
    /// Metadata for the exact content returned in [`Self::content`].
    pub(crate) metadata: ScriptMetadata,
    /// UTF-8 Python source.
    pub(crate) content: String,
}

/// Bounded metadata for one direct `.py` file.
#[derive(Clone, Debug)]
pub(crate) struct ScriptMetadata {
    /// Validated direct filename, including `.py`.
    pub(crate) name: String,
    /// Optional first `# Description:` line from the script header.
    pub(crate) description: Option<String>,
    /// SHA-256 content revision used for observed-conflict checks.
    pub(crate) revision: String,
    /// Exact UTF-8 byte length.
    pub(crate) size: usize,
    /// Filesystem modification time when available.
    pub(crate) modified: Option<SystemTime>,
}

/// Failure while opening or changing a script library.
#[derive(Debug)]
pub(crate) enum ScriptLibraryError {
    /// The selected library root is missing or not a directory.
    InvalidRoot,
    /// A script name is not one direct `.py` filename.
    InvalidName,
    /// The direct entry is missing, not a regular file, or is a symbolic link.
    NotFound,
    /// A script is larger than the library limit.
    TooLarge,
    /// Script content is not valid UTF-8.
    InvalidUtf8,
    /// The destination exists when a new script was requested.
    AlreadyExists,
    /// The on-disk revision differs from the caller's observed revision.
    Conflict {
        /// Revision currently observed on disk.
        actual_revision: String,
    },
    /// A filesystem operation failed.
    Io(io::Error),
}

impl fmt::Display for ScriptLibraryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoot => {
                formatter.write_str("script library must be an existing directory")
            }
            Self::InvalidName => formatter.write_str("script name must be one direct .py filename"),
            Self::NotFound => formatter.write_str("script is missing or is not a regular file"),
            Self::TooLarge => formatter.write_str("script exceeds 262144 UTF-8 bytes"),
            Self::InvalidUtf8 => formatter.write_str("script is not valid UTF-8"),
            Self::AlreadyExists => formatter.write_str("script already exists"),
            Self::Conflict { .. } => {
                formatter.write_str("script changed on disk; reload or save under another name")
            }
            Self::Io(error) => write!(formatter, "script library I/O failed: {error}"),
        }
    }
}

impl std::error::Error for ScriptLibraryError {}

impl From<io::Error> for ScriptLibraryError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// One selected directory containing ordinary Python recipe files.
#[derive(Clone, Debug)]
pub(crate) struct ScriptLibrary {
    root: PathBuf,
}

impl ScriptLibrary {
    /// Open an existing selected directory without creating or scanning subdirectories.
    pub(crate) fn open(root: impl AsRef<Path>) -> Result<Self, ScriptLibraryError> {
        let root = root.as_ref().canonicalize().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ScriptLibraryError::InvalidRoot
            } else {
                ScriptLibraryError::Io(error)
            }
        })?;
        if !root.is_dir() {
            return Err(ScriptLibraryError::InvalidRoot);
        }
        Ok(Self { root })
    }

    /// Return sorted metadata for readable direct non-symlink `.py` files.
    pub(crate) fn list(&self) -> Result<Vec<ScriptMetadata>, ScriptLibraryError> {
        let mut scripts = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if validate_name(&name).is_err() {
                continue;
            }
            match self.load(&name) {
                Ok(document) => scripts.push(document.metadata),
                Err(ScriptLibraryError::NotFound | ScriptLibraryError::InvalidUtf8) => {}
                Err(error) => return Err(error),
            }
        }
        scripts.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(scripts)
    }

    /// Load one direct script and compute the revision of its exact bytes.
    pub(crate) fn load(&self, name: &str) -> Result<ScriptDocument, ScriptLibraryError> {
        let path = self.script_path(name)?;
        let metadata = regular_file_metadata(&path)?;
        if metadata.len() > MAX_SCRIPT_BYTES as u64 {
            return Err(ScriptLibraryError::TooLarge);
        }
        let bytes = fs::read(&path)?;
        if bytes.len() > MAX_SCRIPT_BYTES {
            return Err(ScriptLibraryError::TooLarge);
        }
        let content = String::from_utf8(bytes).map_err(|_| ScriptLibraryError::InvalidUtf8)?;
        Ok(document(name, content, metadata.modified().ok()))
    }

    /// Save a new script or replace the exact revision previously loaded by the caller.
    ///
    /// `None` requires the destination to remain absent.
    /// `Some` detects an observed external change before staging and immediately before rename.
    pub(crate) fn save(
        &self,
        name: &str,
        content: &str,
        expected_revision: Option<&str>,
    ) -> Result<ScriptDocument, ScriptLibraryError> {
        if content.len() > MAX_SCRIPT_BYTES {
            return Err(ScriptLibraryError::TooLarge);
        }
        let path = self.script_path(name)?;
        self.check_expected(&path, expected_revision)?;

        let (temporary, mut file) = self.create_temporary()?;
        let staged = (|| -> Result<(), ScriptLibraryError> {
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            drop(file);
            self.check_expected(&path, expected_revision)?;
            fs::rename(&temporary, &path)?;
            sync_directory(&self.root)?;
            Ok(())
        })();
        if staged.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        staged?;
        self.load(name)
    }

    /// Rename one exact observed revision without overwriting another script.
    pub(crate) fn rename(
        &self,
        old_name: &str,
        new_name: &str,
        expected_revision: &str,
    ) -> Result<ScriptDocument, ScriptLibraryError> {
        let old_path = self.script_path(old_name)?;
        let new_path = self.script_path(new_name)?;
        if old_path == new_path {
            return self.load(old_name);
        }
        self.check_expected(&old_path, Some(expected_revision))?;
        match fs::symlink_metadata(&new_path) {
            Ok(_) => return Err(ScriptLibraryError::AlreadyExists),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        self.check_expected(&old_path, Some(expected_revision))?;
        fs::rename(old_path, new_path)?;
        sync_directory(&self.root)?;
        self.load(new_name)
    }

    fn script_path(&self, name: &str) -> Result<PathBuf, ScriptLibraryError> {
        validate_name(name)?;
        Ok(self.root.join(name))
    }

    fn check_expected(
        &self,
        path: &Path,
        expected_revision: Option<&str>,
    ) -> Result<(), ScriptLibraryError> {
        match (expected_revision, fs::symlink_metadata(path)) {
            (None, Err(error)) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            (None, Ok(_)) => Err(ScriptLibraryError::AlreadyExists),
            (None, Err(error)) => Err(error.into()),
            (Some(_), Err(error)) if error.kind() == io::ErrorKind::NotFound => {
                Err(ScriptLibraryError::NotFound)
            }
            (Some(expected), Ok(_)) => {
                let actual = self.load_path_revision(path)?;
                if actual == expected {
                    Ok(())
                } else {
                    Err(ScriptLibraryError::Conflict {
                        actual_revision: actual,
                    })
                }
            }
            (Some(_), Err(error)) => Err(error.into()),
        }
    }

    fn load_path_revision(&self, path: &Path) -> Result<String, ScriptLibraryError> {
        let metadata = regular_file_metadata(path)?;
        if metadata.len() > MAX_SCRIPT_BYTES as u64 {
            return Err(ScriptLibraryError::TooLarge);
        }
        let bytes = fs::read(path)?;
        if bytes.len() > MAX_SCRIPT_BYTES {
            return Err(ScriptLibraryError::TooLarge);
        }
        Ok(hex_digest(Sha256::digest(bytes)))
    }

    fn create_temporary(&self) -> Result<(PathBuf, File), ScriptLibraryError> {
        for _ in 0..32 {
            let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let name = format!(".runebender-script-{}-{sequence}.tmp", std::process::id());
            let path = self.root.join(name);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(ScriptLibraryError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique temporary script file",
        )))
    }
}

fn document(name: &str, content: String, modified: Option<SystemTime>) -> ScriptDocument {
    let description = content.lines().take(8).find_map(|line| {
        line.strip_prefix("# Description:")
            .map(str::trim)
            .filter(|description| !description.is_empty())
            .map(|description| description.chars().take(256).collect())
    });
    let metadata = ScriptMetadata {
        name: name.into(),
        description,
        revision: hex_digest(Sha256::digest(content.as_bytes())),
        size: content.len(),
        modified,
    };
    ScriptDocument { metadata, content }
}

fn regular_file_metadata(path: &Path) -> Result<fs::Metadata, ScriptLibraryError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            ScriptLibraryError::NotFound
        } else {
            ScriptLibraryError::Io(error)
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ScriptLibraryError::NotFound);
    }
    Ok(metadata)
}

fn validate_name(name: &str) -> Result<(), ScriptLibraryError> {
    if name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || name == "."
        || name == ".."
        || name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
        || !name.ends_with(".py")
        || name == ".py"
    {
        return Err(ScriptLibraryError::InvalidName);
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), ScriptLibraryError> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    Ok(())
}

fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "runebender-script-library-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn saves_loads_lists_and_renames_scripts() {
        let root = TestDirectory::new("roundtrip");
        let library = ScriptLibrary::open(&root.0).expect("open library");
        let saved = library
            .save(
                "anchors.py",
                "# Description: List anchors\nprint('ok')\n",
                None,
            )
            .expect("save new script");
        assert_eq!(saved.metadata.description.as_deref(), Some("List anchors"));
        assert_eq!(library.list().expect("list scripts").len(), 1);
        let renamed = library
            .rename("anchors.py", "list-anchors.py", &saved.metadata.revision)
            .expect("rename script");
        assert_eq!(renamed.metadata.name, "list-anchors.py");
        assert!(matches!(
            library.load("anchors.py"),
            Err(ScriptLibraryError::NotFound)
        ));
    }

    #[test]
    fn rejects_traversal_and_symlinks() {
        let root = TestDirectory::new("paths");
        let library = ScriptLibrary::open(&root.0).expect("open library");
        assert!(matches!(
            library.save("../escape.py", "", None),
            Err(ScriptLibraryError::InvalidName)
        ));

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("missing.py", root.0.join("link.py"))
                .expect("create symlink");
            assert!(matches!(
                library.load("link.py"),
                Err(ScriptLibraryError::NotFound)
            ));
        }
    }

    #[test]
    fn refuses_to_overwrite_an_external_edit() {
        let root = TestDirectory::new("conflict");
        let library = ScriptLibrary::open(&root.0).expect("open library");
        let saved = library
            .save("recipe.py", "print('first')\n", None)
            .expect("save new script");
        fs::write(root.0.join("recipe.py"), "print('external')\n").expect("external edit");
        let error = library
            .save(
                "recipe.py",
                "print('draft')\n",
                Some(&saved.metadata.revision),
            )
            .expect_err("conflict");
        assert!(matches!(error, ScriptLibraryError::Conflict { .. }));
        assert_eq!(
            fs::read_to_string(root.0.join("recipe.py")).expect("read external edit"),
            "print('external')\n"
        );
    }
}
