// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Script artifacts and the editor-owned draft buffer.
//!
//! The runtime owns script-library persistence and recipe execution.
//! This module intentionally owns neither: it makes a complete Python fence
//! from chat available to the user and preserves an explicitly opened draft.

use std::collections::BTreeMap;

#[cfg(unix)]
use runebender::document::agent::ToolCall;
#[cfg(unix)]
use runebender::document::agent_edit::AgentEditRequest;
use runebender::document::agent_edit::AgentLayerGuard;
use runebender::document::edit_batch::canonical_glyph_revision;
use runebender::document::script_recipe::{
    ScriptRecipeAnchor, ScriptRecipeInput, ScriptRecipeLayer, ScriptRecipeResult,
};
use serde_json::Value;
#[cfg(unix)]
use serde_json::json;

use crate::application::platform::script_jobs::{
    ScriptJobCancelOutcome, ScriptJobConfig, ScriptJobHandle, ScriptJobOutcome, ScriptJobQueue,
    ScriptJobRequest, ScriptJobStatus,
};
use crate::application::view::panels::tabs::Rail;
use crate::application::workspace::{Mode, Workspace};

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
    pub(crate) revision: Option<String>,
    pub(crate) saved_name: Option<String>,
}

/// One background run bound to the exact captured UI and document state.
#[derive(Debug)]
pub(crate) struct ScriptRun {
    pub(crate) handle: ScriptJobHandle,
    input: ScriptRecipeInput,
    script: String,
    parameters: String,
    document_revision: u64,
    glyphs: Vec<String>,
}

/// A validated result retained for review before any font mutation.
#[derive(Clone, Debug)]
pub(crate) struct ScriptProposal {
    pub(crate) input: ScriptRecipeInput,
    pub(crate) result: ScriptRecipeResult,
    script: String,
    parameters: String,
    document_revision: u64,
    glyphs: Vec<String>,
    pub(crate) stderr: String,
    pub(crate) applied: bool,
}

/// A wake-up from the asynchronous script status pump.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScriptProgress;

/// Presentation state which remains local until the runtime library is wired.
///
/// Keeping this buffer separate from the library lets an assistant offer code
/// without writing or executing it. The library integration will attach a
/// revision to this same draft before it enables Save.
#[derive(Debug)]
pub(crate) struct ScriptsState {
    pub(crate) draft: Option<ScriptDraft>,
    pub(crate) notice: Option<String>,
    /// Explicit JSON-object parameters captured with a Run request.
    pub(crate) parameters: String,
    pub(crate) library_filter: String,
    pub(crate) running: Option<ScriptRun>,
    pub(crate) proposal: Option<ScriptProposal>,
    pub(crate) next_job: u64,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) library: Option<crate::application::platform::script_library::ScriptLibrary>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) library_items: Vec<crate::application::platform::script_library::ScriptMetadata>,
}

impl Default for ScriptsState {
    fn default() -> Self {
        Self {
            draft: None,
            notice: None,
            parameters: "{}".into(),
            library_filter: String::new(),
            running: None,
            proposal: None,
            next_job: 1,
            #[cfg(not(target_arch = "wasm32"))]
            library: None,
            #[cfg(not(target_arch = "wasm32"))]
            library_items: Vec::new(),
        }
    }
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
            revision: None,
            saved_name: None,
        });
        self.scripts.notice = Some("Opened chat artifact without saving or running it".into());
        self.rail = Rail::Scripts;
    }

    /// Select the user-owned directory containing ordinary Python scripts.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn choose_script_library(&mut self) {
        let start = self
            .font
            .document_source()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let Some(path) = crate::application::platform::dialogs::folder(start) else {
            return;
        };
        match crate::application::platform::script_library::ScriptLibrary::open(path) {
            Ok(library) => {
                self.scripts.library = Some(library);
                self.refresh_script_library();
            }
            Err(error) => self.scripts.notice = Some(error.to_string()),
        }
    }

    /// Refresh the saved-script list without changing the open draft.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn refresh_script_library(&mut self) {
        let Some(library) = self.scripts.library.as_ref() else {
            return;
        };
        match library.list() {
            Ok(items) => {
                self.scripts.library_items = items;
                self.scripts.notice = None;
            }
            Err(error) => self.scripts.notice = Some(error.to_string()),
        }
    }

    /// Save only the exact observed revision; an external edit leaves the draft intact.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn save_script_draft(&mut self) {
        let (Some(library), Some(draft)) =
            (self.scripts.library.as_ref(), self.scripts.draft.as_mut())
        else {
            self.scripts.notice = Some("Choose a Scripts folder before saving".into());
            return;
        };
        let saved = if let (Some(saved_name), Some(revision)) =
            (draft.saved_name.as_deref(), draft.revision.as_deref())
            && saved_name != draft.name
        {
            library
                .rename(saved_name, &draft.name, revision)
                .and_then(|renamed| {
                    library.save(
                        &draft.name,
                        &draft.content,
                        Some(&renamed.metadata.revision),
                    )
                })
        } else {
            library.save(&draft.name, &draft.content, draft.revision.as_deref())
        };
        match saved {
            Ok(saved) => {
                draft.revision = Some(saved.metadata.revision.clone());
                draft.saved_name = Some(saved.metadata.name);
                draft.dirty = false;
                self.scripts.notice = Some("Saved script".into());
                self.refresh_script_library();
            }
            Err(error) => self.scripts.notice = Some(error.to_string()),
        }
    }

    /// Load one saved script unless doing so would discard manual edits.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn open_saved_script(&mut self, name: &str) {
        if self.scripts.draft.as_ref().is_some_and(|draft| draft.dirty) {
            self.scripts.notice =
                Some("Save or discard the current script before opening another".into());
            return;
        }
        let Some(library) = self.scripts.library.as_ref() else {
            return;
        };
        match library.load(name) {
            Ok(document) => {
                self.scripts.draft = Some(ScriptDraft {
                    name: document.metadata.name.clone(),
                    content: document.content,
                    dirty: false,
                    revision: Some(document.metadata.revision),
                    saved_name: Some(document.metadata.name),
                });
                self.scripts.notice = None;
                self.scripts.proposal = None;
            }
            Err(error) => self.scripts.notice = Some(error.to_string()),
        }
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

    /// Change the exact JSON parameter text supplied to a subsequent run.
    pub(crate) fn script_parameters_changed(&mut self, parameters: String) {
        if self.scripts.parameters != parameters {
            self.scripts.parameters = parameters;
        }
    }

    /// Human-readable current capture scope.
    pub(crate) fn script_scope_label(&self) -> String {
        let glyphs = self.script_scope_glyphs();
        let source_name = self
            .font
            .master_names()
            .get(self.font.active())
            .cloned()
            .unwrap_or_else(|| "Unnamed".into());
        let source = self
            .font
            .project
            .source_id(self.font.active())
            .map_or_else(|| "unavailable".into(), |source| source.0.to_string());
        format!(
            "Source {source} · {source_name} · {}",
            if glyphs.is_empty() {
                "no glyphs selected".into()
            } else {
                glyphs.join(", ")
            }
        )
    }

    fn script_scope_glyphs(&self) -> Vec<String> {
        let mut glyphs = match self.mode {
            Mode::Editor(_) => vec![self.session.glyph_name.clone()],
            Mode::Overview => {
                let mut indices = self.multi_selected.iter().copied().collect::<Vec<_>>();
                if indices.is_empty()
                    && let Some(selected) = self.selected
                {
                    indices.push(selected);
                }
                indices.sort_unstable();
                indices
                    .into_iter()
                    .filter_map(|index| self.font.glyphs.get(index))
                    .map(|glyph| glyph.name.clone())
                    .collect()
            }
            Mode::Nodes => Vec::new(),
        };
        glyphs.sort();
        glyphs.dedup();
        glyphs
    }

    fn capture_script_input(&mut self) -> Result<(ScriptRecipeInput, Vec<String>), String> {
        let glyphs = self.script_scope_glyphs();
        if glyphs.is_empty() {
            return Err("Select at least one glyph before running a script".into());
        }
        let parameters: Value = serde_json::from_str(&self.scripts.parameters)
            .map_err(|error| format!("Parameters must be a JSON object: {error}"))?;
        let Value::Object(parameters) = parameters else {
            return Err("Parameters must be a JSON object".into());
        };
        let parameters = parameters.into_iter().collect::<BTreeMap<_, _>>();
        let source = self
            .font
            .project
            .source_id(self.font.active())
            .ok_or("The active source is unavailable")?;
        let layer_id = self
            .font
            .project
            .document_source(source)
            .ok_or("The active source is unavailable")?
            .default_layer();
        let mut layers = Vec::with_capacity(glyphs.len());
        for glyph_name in &glyphs {
            let glyph = self
                .font
                .project
                .document_glyph(glyph_name)
                .ok_or_else(|| format!("Glyph {glyph_name} is unavailable"))?;
            let layer = self
                .font
                .project
                .document_layer(glyph_name, &layer_id)
                .ok_or_else(|| format!("Glyph {glyph_name} has no active-source layer"))?;
            layers.push(ScriptRecipeLayer {
                guard: AgentLayerGuard {
                    glyph: glyph_name.clone(),
                    glyph_id: glyph.id().to_wire(),
                    layer: layer_id.name.clone(),
                    expected_revision: canonical_glyph_revision(layer)?,
                },
                width: layer.width(),
                anchors: layer
                    .anchors()
                    .map(|anchor| {
                        let position = anchor.position();
                        ScriptRecipeAnchor {
                            id: anchor.id().to_wire(),
                            name: (!anchor.name().is_empty()).then(|| anchor.name().to_string()),
                            x: position.x,
                            y: position.y,
                        }
                    })
                    .collect(),
            });
        }
        let job_id = format!("scripts-{}-{}", self.document_id, self.scripts.next_job);
        self.scripts.next_job = self.scripts.next_job.saturating_add(1);
        let input = ScriptRecipeInput::new(job_id, source.0, parameters, layers)
            .map_err(|error| error.to_string())?;
        Ok((input, glyphs))
    }

    /// Submit the exact unsaved draft and immutable scope to the shared queue.
    pub(crate) fn run_script_draft(&mut self) {
        if self.scripts.running.is_some() {
            self.scripts.notice = Some("A script is already running".into());
            return;
        }
        let Some(draft) = self.scripts.draft.as_ref() else {
            self.scripts.notice = Some("Open or create a Python script before running".into());
            return;
        };
        let script = draft.content.clone();
        let parameters = self.scripts.parameters.clone();
        let document_revision = self.font.project.document_revision();
        let (input, glyphs) = match self.capture_script_input() {
            Ok(capture) => capture,
            Err(error) => {
                self.scripts.notice = Some(error);
                return;
            }
        };
        if self.script_jobs.is_none() {
            let executable = std::env::var_os("RUNEBENDER_PYTHON")
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "python3".into());
            match ScriptJobQueue::new(ScriptJobConfig::new(executable)) {
                Ok(queue) => self.script_jobs = Some(queue),
                Err(error) => {
                    self.scripts.notice = Some(format!("Python runner unavailable: {error:?}"));
                    return;
                }
            }
        }
        let handle = match self
            .script_job_queue()
            .expect("queue initialized above")
            .submit(ScriptJobRequest {
                input: input.clone(),
                script: script.clone(),
            }) {
            Ok(handle) => handle,
            Err(error) => {
                self.scripts.notice = Some(format!("Could not start script: {error:?}"));
                return;
            }
        };
        self.scripts.running = Some(ScriptRun {
            handle,
            input,
            script,
            parameters,
            document_revision,
            glyphs,
        });
        self.scripts.proposal = None;
        self.scripts.notice = Some("Running Python recipe…".into());
    }

    /// Poll only the Scripts panel's retained handle.
    pub(crate) fn script_pump(&mut self) {
        let Some(run) = self.scripts.running.as_ref() else {
            return;
        };
        let handle = run.handle;
        let inspection = self
            .script_job_queue()
            .and_then(|queue| queue.inspect(handle));
        let Some(inspection) = inspection else {
            self.scripts.running = None;
            self.scripts.notice = Some("The script job is no longer retained".into());
            return;
        };
        if matches!(
            inspection.status,
            ScriptJobStatus::Queued | ScriptJobStatus::Running
        ) {
            self.scripts.notice = Some(
                match inspection.status {
                    ScriptJobStatus::Queued => "Script queued…",
                    _ => "Running Python recipe…",
                }
                .into(),
            );
            return;
        }
        let run = self.scripts.running.take().expect("run retained above");
        if let Some(queue) = self.script_job_queue() {
            let _ = queue.discard(handle);
        }
        match inspection.outcome {
            Some(ScriptJobOutcome::Completed { result, stderr }) => {
                self.scripts.notice = Some(if result.edits.is_empty() {
                    "Report completed without font changes".into()
                } else {
                    format!("Preview ready · {} guarded layer edits", result.edits.len())
                });
                self.scripts.proposal = Some(ScriptProposal {
                    input: run.input,
                    result,
                    script: run.script,
                    parameters: run.parameters,
                    document_revision: run.document_revision,
                    glyphs: run.glyphs,
                    stderr,
                    applied: false,
                });
            }
            Some(ScriptJobOutcome::Failed { failure, stderr }) => {
                self.scripts.notice = Some(format!(
                    "Script failed: {failure}{}",
                    if stderr.trim().is_empty() {
                        String::new()
                    } else {
                        format!("\n{}", stderr.trim())
                    }
                ));
            }
            Some(ScriptJobOutcome::Cancelled { stderr, .. }) => {
                self.scripts.notice = Some(if stderr.trim().is_empty() {
                    "Script cancelled".into()
                } else {
                    format!("Script cancelled\n{}", stderr.trim())
                });
            }
            None => self.scripts.notice = Some("Script ended without a result".into()),
        }
    }

    /// Request cancellation without blocking the application thread.
    pub(crate) fn cancel_script_run(&mut self) {
        let Some(run) = self.scripts.running.as_ref() else {
            return;
        };
        let outcome = self
            .script_job_queue()
            .map(|queue| queue.cancel(run.handle))
            .unwrap_or(ScriptJobCancelOutcome::UnknownHandle);
        self.scripts.notice = Some(match outcome {
            ScriptJobCancelOutcome::CancelledBeforeStart => "Script cancelled".into(),
            ScriptJobCancelOutcome::CancellationRequested => "Cancelling script…".into(),
            ScriptJobCancelOutcome::TooLate => "Script has already finished".into(),
            ScriptJobCancelOutcome::UnknownHandle => "Script job is no longer retained".into(),
        });
    }

    /// Explain whether a retained proposal is still bound to current state.
    pub(crate) fn script_proposal_stale_reason(&self) -> Option<String> {
        let proposal = self.scripts.proposal.as_ref()?;
        if proposal.applied {
            return Some("This proposal was already applied".into());
        }
        if self.font.project.document_revision() != proposal.document_revision {
            return Some("The font changed after this run".into());
        }
        if self
            .scripts
            .draft
            .as_ref()
            .map(|draft| draft.content.as_str())
            != Some(proposal.script.as_str())
        {
            return Some("The script changed after this run".into());
        }
        if self.scripts.parameters != proposal.parameters {
            return Some("The parameters changed after this run".into());
        }
        if self.script_scope_glyphs() != proposal.glyphs {
            return Some("The selected glyph scope changed after this run".into());
        }
        let active = self
            .font
            .project
            .source_id(self.font.active())
            .map(|source| source.0);
        if active != Some(proposal.input.source) {
            return Some("The active source changed after this run".into());
        }
        None
    }

    /// Apply one fresh validated proposal through the existing receipt path.
    #[cfg(unix)]
    pub(crate) fn apply_script_proposal(&mut self) {
        if let Some(reason) = self.script_proposal_stale_reason() {
            self.scripts.notice = Some(format!("Run the script again: {reason}"));
            return;
        }
        let Some(proposal) = self.scripts.proposal.as_ref() else {
            return;
        };
        if proposal.result.edits.is_empty() {
            self.scripts.notice = Some("This report has no font changes to apply".into());
            return;
        }
        let Some(epoch) = self
            .live
            .as_ref()
            .map(|server| server.document_epoch().to_string())
        else {
            self.scripts.notice =
                Some("Guarded Apply requires the native live document session".into());
            return;
        };
        let request = AgentEditRequest {
            expected_document_epoch: epoch,
            actor: "scripts-panel".into(),
            operation_key: proposal.input.job_id.clone(),
            authorization: "user-approved".into(),
            source: proposal.input.source,
            history_name: format!(
                "Apply {}",
                self.scripts
                    .draft
                    .as_ref()
                    .map_or("Python recipe", |draft| draft.name.as_str())
            ),
            reads: proposal.result.reads.clone(),
            edits: proposal.result.edits.clone(),
        };
        let call = ToolCall {
            name: "agent_apply".into(),
            arguments: serde_json::to_value(request).expect("typed script request serializes"),
        };
        let response = self
            .call_agent_edit(&call)
            .unwrap_or_else(|| json!({"ok":false,"error":"agent_apply unavailable"}));
        if response.get("ok").and_then(Value::as_bool) == Some(true) {
            if let Some(proposal) = self.scripts.proposal.as_mut() {
                proposal.applied = true;
            }
            self.scripts.notice =
                Some("Applied script proposal · use ordinary Undo to revert".into());
        } else {
            self.scripts.notice = Some(format!(
                "Apply failed: {}",
                response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
            ));
        }
    }

    /// Undo the receipt-backed script edit through ordinary editor history.
    pub(crate) fn undo_script_apply(&mut self) {
        self.undo_active_edit(false);
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
    use crate::application::font_model::FontModel;
    use runebender::document::agent_edit::{AgentEditOperation, AgentLayerEdits};
    use runebender::document::project::Project;

    fn workspace() -> Workspace {
        let path = std::env::temp_dir().join(format!(
            "runebender-script-ui-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the test clock is after the epoch")
                .as_nanos()
        ));
        let mut app = Workspace::from_model(FontModel::from_project(Project::new_font(path)))
            .expect("new-font workspace");
        let index = app.font.index_of("A").expect("new font contains A");
        app.open_glyph(index);
        app.open_script_artifact(ScriptArtifact {
            name: "test.py".into(),
            content: "print('not executed by these state tests')\n".into(),
        });
        app
    }

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

    #[test]
    fn capture_requires_an_object_and_binds_the_active_source_and_glyph() {
        let mut app = workspace();
        app.scripts.parameters = r#"{"recipe":"list","glyphs":["A"]}"#.into();
        let (input, glyphs) = app.capture_script_input().expect("capture succeeds");
        assert_eq!(glyphs, ["A"]);
        assert_eq!(
            input.source,
            app.font.project.source_id(app.font.active()).unwrap().0
        );
        assert_eq!(input.layers.len(), 1);
        assert_eq!(input.layers[0].guard.glyph, "A");
        assert_eq!(input.parameters["recipe"], "list");

        app.scripts.parameters = "[]".into();
        assert_eq!(
            app.capture_script_input().unwrap_err(),
            "Parameters must be a JSON object"
        );
    }

    #[test]
    fn draft_parameter_and_document_changes_make_a_preview_stale() {
        let mut app = workspace();
        let (input, glyphs) = app.capture_script_input().expect("capture succeeds");
        app.scripts.proposal = Some(ScriptProposal {
            result: ScriptRecipeResult {
                schema_version: input.schema_version,
                job_id: input.job_id.clone(),
                input_hash: input.input_hash.clone(),
                report: "No changes".into(),
                reads: Vec::new(),
                edits: Vec::new(),
            },
            input,
            script: app.scripts.draft.as_ref().unwrap().content.clone(),
            parameters: app.scripts.parameters.clone(),
            document_revision: app.font.project.document_revision(),
            glyphs,
            stderr: String::new(),
            applied: false,
        });
        assert_eq!(app.script_proposal_stale_reason(), None);
        app.script_parameters_changed(r#"{"dx":10}"#.into());
        assert_eq!(
            app.script_proposal_stale_reason().as_deref(),
            Some("The parameters changed after this run")
        );
    }

    #[cfg(unix)]
    #[test]
    fn guarded_apply_uses_ordinary_undo() {
        let mut app = workspace();
        let (input, glyphs) = app.capture_script_input().expect("capture succeeds");
        let before = input.layers[0].width;
        let target = input.layers[0].guard.clone();
        let result = ScriptRecipeResult {
            schema_version: input.schema_version,
            job_id: input.job_id.clone(),
            input_hash: input.input_hash.clone(),
            report: "Move width for test".into(),
            reads: Vec::new(),
            edits: vec![AgentLayerEdits {
                target,
                operations: vec![AgentEditOperation::SetWidth {
                    width: before + 20.0,
                }],
            }],
        };
        result.validate_against(&input).expect("proposal is valid");
        app.scripts.proposal = Some(ScriptProposal {
            input,
            result,
            script: app.scripts.draft.as_ref().unwrap().content.clone(),
            parameters: app.scripts.parameters.clone(),
            document_revision: app.font.project.document_revision(),
            glyphs,
            stderr: String::new(),
            applied: false,
        });

        app.apply_script_proposal();
        let address = app.font.active_layer_address("A").unwrap();
        assert_eq!(
            app.font
                .project
                .document_layer("A", &address.layer)
                .unwrap()
                .width(),
            before + 20.0
        );
        assert!(app.scripts.proposal.as_ref().unwrap().applied);

        app.undo_script_apply();
        assert_eq!(
            app.font
                .project
                .document_layer("A", &address.layer)
                .unwrap()
                .width(),
            before
        );
    }
}
