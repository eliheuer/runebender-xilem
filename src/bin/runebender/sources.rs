// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Turning the paths on the command line into UFOs to work on.
//!
//! A command takes any mix of UFO directories and designspace files.
//! A designspace stands for its sources, so one path can name a whole
//! family and a shell loop does not have to know the masters.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Expands one path into the UFOs it names.
///
/// A `.designspace` becomes its sources, resolved against the
/// document's own directory and in the order the document lists them.
/// Any other path stands for itself. Sources that a designspace names
/// more than once, which is what layer sources do, appear once.
pub(crate) fn expand(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.extension().is_none_or(|e| e != "designspace") {
        return Ok(vec![path.to_path_buf()]);
    }
    let doc = norad::designspace::DesignSpaceDocument::load(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for source in &doc.sources {
        let ufo = dir.join(&source.filename);
        if seen.insert(ufo.clone()) {
            out.push(ufo);
        }
    }
    if out.is_empty() {
        return Err(format!("{}: no sources", path.display()));
    }
    Ok(out)
}

/// Expands every path given, keeping the order and dropping repeats.
pub(crate) fn expand_all(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for path in paths {
        for ufo in expand(path)? {
            if seen.insert(ufo.clone()) {
                out.push(ufo);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ufo_path_stands_for_itself() {
        let got = expand(Path::new("Font.ufo")).expect("expands");
        assert_eq!(got, vec![PathBuf::from("Font.ufo")]);
    }

    /// The whole point of taking many paths is that a family is one
    /// argument, so a repeated source must not be edited twice.
    #[test]
    fn repeats_are_dropped() {
        let paths = vec![PathBuf::from("A.ufo"), PathBuf::from("A.ufo")];
        assert_eq!(expand_all(&paths).expect("expands").len(), 1);
    }
}
