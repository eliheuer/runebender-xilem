// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Script artifacts and the editor-owned draft buffer.
//!
//! The runtime owns script-library persistence and recipe execution.
//! This module intentionally owns neither: it makes a complete Python fence
//! from chat available to the user and preserves an explicitly opened draft.

use crate::application::view::panels::tabs::Rail;
use crate::application::workspace::Workspace;

/// A complete Python artifact offered by chat.
///
/// A fenced block is not opened until its closing fence arrives, so ordinary
/// streaming prose cannot become executable source by accident.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScriptArtifact {
    pub(crate) name: String,
    pub(crate) content: String,
}

/// The buffer visible in the Scripts panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScriptDraft {
    pub(crate) name: String,
    pub(crate) content: String,
    pub(crate) dirty: bool,
}

/// Presentation state which remains local until the runtime library is wired.
///
/// Keeping this buffer separate from the library lets an assistant offer code
/// without writing or executing it. The library integration will attach a
/// revision to this same draft before it enables Save.
#[derive(Debug, Default)]
pub(crate) struct ScriptsState {
    pub(crate) draft: Option<ScriptDraft>,
    pub(crate) notice: Option<String>,
}

impl Workspace {
    /// Open a user-selected chat artifact in the Scripts panel without saving
    /// or executing it.
    pub(crate) fn open_script_artifact(&mut self, artifact: ScriptArtifact) {
        if self
            .scripts
            .draft
            .as_ref()
            .is_some_and(|draft| draft.dirty && draft.content != artifact.content)
        {
            self.scripts.notice =
                Some("Save or discard the current script before replacing its edited draft".into());
            return;
        }
        self.scripts.draft = Some(ScriptDraft {
            name: artifact.name,
            content: artifact.content,
            dirty: false,
        });
        self.scripts.notice = Some("Opened chat artifact without saving or running it".into());
        self.rail = Rail::Scripts;
    }

    /// Change the open draft's name without touching the script library.
    pub(crate) fn script_name_changed(&mut self, name: String) {
        if let Some(draft) = self.scripts.draft.as_mut()
            && draft.name != name
        {
            draft.name = name;
            draft.dirty = true;
            self.scripts.notice = None;
        }
    }

    /// Change the open draft without saving or executing it.
    pub(crate) fn script_content_changed(&mut self, content: String) {
        if let Some(draft) = self.scripts.draft.as_mut()
            && draft.content != content
        {
            draft.content = content;
            draft.dirty = true;
            self.scripts.notice = None;
        }
    }
}

/// Return each fully closed Python fence offered in a chat response.
pub(crate) fn python_artifacts(text: &str) -> Vec<ScriptArtifact> {
    let mut artifacts = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some(name) = python_fence_name(line) else {
            continue;
        };
        let mut source = String::new();
        let mut closed = false;
        for line in lines.by_ref() {
            if line.trim() == "```" {
                closed = true;
                break;
            }
            source.push_str(line);
            source.push('\n');
        }
        if closed && !source.trim().is_empty() {
            artifacts.push(ScriptArtifact {
                name,
                content: source,
            });
        }
    }
    artifacts
}

fn python_fence_name(line: &str) -> Option<String> {
    let mut words = line.trim().strip_prefix("```")?.split_whitespace();
    let language = words.next()?;
    if !matches!(language, "python" | "py") {
        return None;
    }
    let name = words
        .find(|word| word.ends_with(".py"))
        .unwrap_or("chat-script.py")
        .trim_matches(|character| matches!(character, '(' | ')' | '[' | ']' | '{' | '}'));
    (name.ends_with(".py") && !name.contains(['/', '\\'])).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_closed_python_fences_become_artifacts() {
        assert_eq!(
            python_artifacts("Use this later:\n```python anchors.py\nprint('anchors')\n```"),
            vec![ScriptArtifact {
                name: "anchors.py".into(),
                content: "print('anchors')\n".into(),
            }]
        );
        assert!(python_artifacts("```python\nprint('still streaming')").is_empty());
        assert!(python_artifacts("`python` is a prose fragment").is_empty());
    }

    #[test]
    fn artifact_names_cannot_choose_directories() {
        assert!(python_artifacts("```python ../unsafe.py\npass\n```").is_empty());
        assert!(python_artifacts("```javascript tool.js\npass\n```").is_empty());
    }
}
