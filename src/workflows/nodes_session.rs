// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Guarded authoring and bounded run state for one live node graph.
//!
//! [`GraphSession`] owns the canonical editable [`NodeGraph`], graph-only history and retained
//! execution identities.
//! It does not execute Python, compile proofs or apply font edits.
//! Application adapters capture those inputs through their existing owners and report terminal
//! results back with the exact identity issued here.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::nodes::{Link, Node, NodeGraph, NodeType, Problem, Registry};

/// Schema version for the live graph session API.
pub const GRAPH_SESSION_SCHEMA_VERSION: u32 = 1;

const MAX_GRAPH_NODES: usize = 64;
const MAX_GRAPH_LINKS: usize = 128;
const MAX_GRAPH_BYTES: usize = 1024 * 1024;
const MAX_PATCH_EDITS: usize = 128;
const MAX_MUTATION_BYTES: usize = 1024 * 1024;
const MAX_GRAPH_HISTORY: usize = 32;
const MAX_GRAPH_RECEIPTS: usize = 64;
const MAX_ACTIVE_RUNS: usize = 16;
const MAX_RUN_RECEIPTS: usize = 64;
const MAX_RUN_OUTPUTS: usize = 16;
const MAX_RUN_ERRORS: usize = 16;
const MAX_ID_BYTES: usize = 128;
const MAX_OPERATION_KEY_BYTES: usize = 128;
const MAX_OUTPUT_TEXT_BYTES: usize = 64 * 1024;
const MAX_ERROR_BYTES: usize = 4 * 1024;
const MAX_CODE_BYTES: usize = 256 * 1024;
const MAX_PARAMETERS_BYTES: usize = 64 * 1024;

/// Discoverable bounds shared by native UI and agent adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct GraphSessionLimits {
    /// Maximum nodes in one graph.
    pub nodes: usize,
    /// Maximum links in one graph.
    pub links: usize,
    /// Maximum serialized bytes in one canonical graph.
    pub graph_bytes: usize,
    /// Maximum edits in one atomic patch.
    pub patch_edits: usize,
    /// Maximum serialized bytes in one mutation request.
    pub mutation_bytes: usize,
    /// Maximum UTF-8 bytes in one Python node's code field.
    pub code_bytes: usize,
    /// Maximum serialized bytes in one Python node's parameter object.
    pub parameters_bytes: usize,
    /// Maximum UTF-8 bytes in one identity or field name.
    pub identity_bytes: usize,
    /// Maximum UTF-8 bytes in one operation key.
    pub operation_key_bytes: usize,
    /// Maximum UTF-8 bytes in one retained report.
    pub output_text_bytes: usize,
    /// Maximum UTF-8 bytes in one retained run error.
    pub error_bytes: usize,
    /// Maximum graph-only undo snapshots.
    pub history: usize,
    /// Maximum retained graph mutation receipts.
    pub graph_receipts: usize,
    /// Maximum queued or running executions.
    pub active_runs: usize,
    /// Maximum retained run-start receipts and tombstones.
    pub run_receipts: usize,
    /// Maximum node outputs retained for one run.
    pub outputs_per_run: usize,
    /// Maximum structured errors retained for one run.
    pub errors_per_run: usize,
}

impl GraphSessionLimits {
    const fn current() -> Self {
        Self {
            nodes: MAX_GRAPH_NODES,
            links: MAX_GRAPH_LINKS,
            graph_bytes: MAX_GRAPH_BYTES,
            patch_edits: MAX_PATCH_EDITS,
            mutation_bytes: MAX_MUTATION_BYTES,
            code_bytes: MAX_CODE_BYTES,
            parameters_bytes: MAX_PARAMETERS_BYTES,
            identity_bytes: MAX_ID_BYTES,
            operation_key_bytes: MAX_OPERATION_KEY_BYTES,
            output_text_bytes: MAX_OUTPUT_TEXT_BYTES,
            error_bytes: MAX_ERROR_BYTES,
            history: MAX_GRAPH_HISTORY,
            graph_receipts: MAX_GRAPH_RECEIPTS,
            active_runs: MAX_ACTIVE_RUNS,
            run_receipts: MAX_RUN_RECEIPTS,
            outputs_per_run: MAX_RUN_OUTPUTS,
            errors_per_run: MAX_RUN_ERRORS,
        }
    }
}

/// Node definitions and bounds accepted by one graph session.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GraphDiscovery {
    /// [`GRAPH_SESSION_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Supported node definitions.
    pub node_types: Vec<NodeType>,
    /// Hard authoring and retention limits.
    pub limits: GraphSessionLimits,
    /// JSON Schemas for authoring, run and cancellation requests.
    pub request_schemas: GraphRequestSchemas,
}

/// Discoverable JSON Schemas for graph commands.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GraphRequestSchemas {
    /// [`GraphInteractiveMutationRequest`] schema.
    pub interactive: Value,
    /// [`GraphMutationRequest`] schema.
    pub mutation: Value,
    /// [`GraphRunRequest`] schema.
    pub run: Value,
    /// [`GraphCancelRequest`] schema.
    pub cancel: Value,
}

/// Exact identity of one graph within one open document lifetime.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphIdentity {
    /// Host-generated graph identity.
    pub session_id: String,
    /// Host-generated document lifetime identity.
    pub document_epoch: String,
}

/// Guard for an authoring mutation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphGuard {
    /// Exact graph and document lifetime.
    pub identity: GraphIdentity,
    /// Full graph revision, including layout changes.
    pub revision: u64,
}

/// Guard for execution, intentionally independent of layout changes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphSemanticGuard {
    /// Exact graph and document lifetime.
    pub identity: GraphIdentity,
    /// Monotonic semantic revision.
    pub semantic_revision: u64,
    /// Canonical content digest excluding node positions and file order.
    pub semantic_hash: String,
}

/// One complete read of the editable graph.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GraphSnapshot {
    /// Exact graph and document lifetime.
    pub identity: GraphIdentity,
    /// Full graph revision, including layout changes.
    pub revision: u64,
    /// Monotonic semantic revision.
    pub semantic_revision: u64,
    /// Canonical content digest excluding layout.
    pub semantic_hash: String,
    /// Canonical graph used by the editor, save path and executor.
    pub graph: NodeGraph,
    /// Current structured validation errors.
    pub diagnostics: Vec<GraphDiagnostic>,
    /// Whether graph-only Undo has an entry.
    pub can_undo: bool,
    /// Whether graph-only Redo has an entry.
    pub can_redo: bool,
}

/// Stable diagnostic category for a graph problem.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphDiagnosticCode {
    /// Unsupported future file version.
    Version,
    /// Repeated node identity.
    DuplicateId,
    /// Unknown node type.
    UnknownType,
    /// Known but unavailable node implementation.
    NotBuilt,
    /// Link endpoint is absent.
    DanglingLink,
    /// Link port is absent.
    UnknownPort,
    /// Link connects different kinds.
    KindMismatch,
    /// More than one link enters one input.
    DoubleInput,
    /// Required input is absent.
    MissingInput,
    /// Typed value has the wrong kind.
    BadValue,
    /// Value does not name an input.
    StrayValue,
    /// Directed cycle.
    Cycle,
}

/// A graph error addressable by node, port or field.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphDiagnostic {
    /// Stable category.
    pub code: GraphDiagnosticCode,
    /// Node most directly responsible, when one exists.
    pub node: Option<u32>,
    /// Linked input or output port, when one exists.
    pub port: Option<String>,
    /// Typed value field, when one exists.
    pub field: Option<String>,
    /// Full structured link, when relevant.
    pub link: Option<Link>,
    /// Human-readable detail.
    pub message: String,
}

impl From<&Problem> for GraphDiagnostic {
    fn from(problem: &Problem) -> Self {
        let (code, node, port, field, link) = match problem {
            Problem::Version { .. } => (GraphDiagnosticCode::Version, None, None, None, None),
            Problem::DuplicateId { id } => (
                GraphDiagnosticCode::DuplicateId,
                Some(*id),
                None,
                None,
                None,
            ),
            Problem::UnknownType { node, .. } => (
                GraphDiagnosticCode::UnknownType,
                Some(*node),
                None,
                None,
                None,
            ),
            Problem::NotBuilt { node, .. } => {
                (GraphDiagnosticCode::NotBuilt, Some(*node), None, None, None)
            }
            Problem::DanglingLink { link } => (
                GraphDiagnosticCode::DanglingLink,
                Some(link.to()),
                Some(link.input().into()),
                None,
                Some(link.clone()),
            ),
            Problem::UnknownPort { link, end } => {
                let output = end == "output";
                (
                    GraphDiagnosticCode::UnknownPort,
                    Some(if output { link.from() } else { link.to() }),
                    Some(if output { link.output() } else { link.input() }.into()),
                    None,
                    Some(link.clone()),
                )
            }
            Problem::KindMismatch { link, .. } => (
                GraphDiagnosticCode::KindMismatch,
                Some(link.to()),
                Some(link.input().into()),
                None,
                Some(link.clone()),
            ),
            Problem::DoubleInput { node, input } => (
                GraphDiagnosticCode::DoubleInput,
                Some(*node),
                Some(input.clone()),
                None,
                None,
            ),
            Problem::MissingInput { node, input } => (
                GraphDiagnosticCode::MissingInput,
                Some(*node),
                Some(input.clone()),
                None,
                None,
            ),
            Problem::BadValue { node, input, .. } => (
                GraphDiagnosticCode::BadValue,
                Some(*node),
                None,
                Some(input.clone()),
                None,
            ),
            Problem::StrayValue { node, input } => (
                GraphDiagnosticCode::StrayValue,
                Some(*node),
                None,
                Some(input.clone()),
                None,
            ),
            Problem::Cycle { nodes } => (
                GraphDiagnosticCode::Cycle,
                nodes.first().copied(),
                None,
                None,
                None,
            ),
        };
        Self {
            code,
            node,
            port,
            field,
            link,
            message: problem.to_string(),
        }
    }
}

/// One atomic graph edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "edit", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphEdit {
    /// Add one caller-identified node.
    AddNode {
        /// Complete node including stable ID and initial values.
        node: Node,
    },
    /// Remove a node and all touching links.
    RemoveNode {
        /// Existing node ID.
        node: u32,
    },
    /// Change only canvas layout.
    MoveNode {
        /// Existing node ID.
        node: u32,
        /// Finite canvas position.
        pos: [f32; 2],
    },
    /// Set one typed node field.
    SetValue {
        /// Existing node ID.
        node: u32,
        /// Input field name.
        field: String,
        /// JSON value validated through the node registry.
        value: Value,
    },
    /// Remove one typed node field.
    RemoveValue {
        /// Existing node ID.
        node: u32,
        /// Input field name.
        field: String,
    },
    /// Connect an output to an input, replacing the input's previous link.
    Connect {
        /// Complete typed link.
        link: Link,
    },
    /// Remove the link entering one input.
    Disconnect {
        /// Destination node.
        node: u32,
        /// Destination input port.
        input: String,
    },
}

/// A graph-only mutation, independent of code-editor and font history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "mutation", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphMutation {
    /// Apply every edit or none of them.
    Patch {
        /// Ordered edits resolved against one temporary graph.
        edits: Vec<GraphEdit>,
    },
    /// Restore the previous graph snapshot.
    Undo,
    /// Restore the next graph snapshot.
    Redo,
}

/// One actor-scoped, revision-guarded mutation request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphMutationRequest {
    /// Exact session and full graph revision.
    pub guard: GraphGuard,
    /// Bounded actor identity owning the retry key.
    pub actor: String,
    /// Bounded idempotency key within the actor and document epoch.
    pub operation_key: String,
    /// Atomic graph-only operation.
    pub mutation: GraphMutation,
}

/// Direct synchronous UI mutation without a network retry receipt.
///
/// Text widgets retain their own typing undo.
/// Committed code changes and drag completion call this boundary so ordinary interaction does not
/// exhaust the agent idempotency ledger.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphInteractiveMutationRequest {
    /// Exact session and full graph revision.
    pub guard: GraphGuard,
    /// Atomic graph-only operation.
    pub mutation: GraphMutation,
}

/// Result of one direct synchronous UI mutation.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GraphInteractiveMutationResult {
    /// Whether the graph changed.
    pub changed: bool,
    /// Whether execution semantics changed.
    pub semantic_changed: bool,
    /// Exact graph state after the request.
    pub snapshot: GraphSnapshot,
}

/// Immutable result retained for exact mutation retries.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GraphMutationReceipt {
    /// Actor supplied by the request.
    pub actor: String,
    /// Operation key supplied by the request.
    pub operation_key: String,
    /// SHA-256 of the normalized guarded payload.
    pub payload_sha256: String,
    /// Whether the original request changed the graph.
    pub changed: bool,
    /// Whether the original request changed execution semantics.
    pub semantic_changed: bool,
    /// Exact result produced by the original request.
    pub snapshot: GraphSnapshot,
}

/// Whether this response executed or replayed an existing receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphReceiptDisposition {
    /// The request executed now.
    Applied,
    /// The exact retained receipt was returned without executing again.
    Replayed,
}

/// Response to one mutation attempt.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GraphMutationResponse {
    /// New execution or exact retry.
    pub disposition: GraphReceiptDisposition,
    /// Immutable original result.
    pub receipt: GraphMutationReceipt,
}

/// Stable graph service error category.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphSessionErrorCode {
    /// A required identity is missing, too large or malformed.
    InvalidIdentity,
    /// Request names another graph or document lifetime.
    WrongSession,
    /// Full graph revision is stale.
    StaleRevision,
    /// Semantic revision or hash is stale.
    StaleSemantics,
    /// An operation key was reused with another payload.
    PayloadMismatch,
    /// A bounded non-evicting receipt ledger is full.
    ReceiptLimit,
    /// A patch edit could not be resolved.
    InvalidEdit,
    /// Graph bounds would be exceeded.
    GraphLimit,
    /// Graph-only undo or redo has no entry.
    HistoryEmpty,
    /// Graph validation blocks execution.
    InvalidGraph,
    /// Graph is valid but not a supported live execution topology.
    UnsupportedTopology,
    /// Host capture does not match the graph.
    CaptureMismatch,
    /// Too many active or retained runs exist.
    RunLimit,
    /// Run handle is unknown.
    UnknownRun,
    /// Run is not in the required state.
    WrongRunState,
    /// Returned output is malformed or does not match the captured run.
    InvalidOutput,
}

/// Structured error from graph authoring or run state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphSessionError {
    /// Stable category.
    pub code: GraphSessionErrorCode,
    /// Human-readable detail.
    pub message: String,
    /// Index within an atomic patch, when relevant.
    pub edit_index: Option<usize>,
    /// Node most directly responsible, when known.
    pub node: Option<u32>,
    /// Port most directly responsible, when known.
    pub port: Option<String>,
    /// Field most directly responsible, when known.
    pub field: Option<String>,
    /// Graph diagnostics blocking execution.
    pub diagnostics: Box<[GraphDiagnostic]>,
}

impl GraphSessionError {
    fn new(code: GraphSessionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            edit_index: None,
            node: None,
            port: None,
            field: None,
            diagnostics: Box::default(),
        }
    }

    fn edit(index: usize, node: Option<u32>, message: impl Into<String>) -> Self {
        Self {
            code: GraphSessionErrorCode::InvalidEdit,
            message: message.into(),
            edit_index: Some(index),
            node,
            port: None,
            field: None,
            diagnostics: Box::default(),
        }
    }
}

impl fmt::Display for GraphSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GraphSessionError {}

#[derive(Clone, Debug)]
struct StoredGraphReceipt {
    payload_sha256: String,
    receipt: GraphMutationReceipt,
}

/// Host-captured identity of the one immutable base font.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct GraphFontCapture {
    /// Stable source identity.
    pub source: usize,
    /// Canonical Project revision captured with the input.
    pub document_revision: u64,
    /// SHA-256 of the immutable canonical capture.
    pub capture_sha256: String,
}

/// Host-captured identity of one Python recipe job.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct GraphScriptCapture {
    /// `live.python` node.
    pub node: u32,
    /// SHA-256 of exact submitted code.
    pub script_sha256: String,
    /// Hash carried by the validated script recipe input.
    pub input_sha256: String,
    /// SHA-256 of the node's exact structured parameters.
    pub parameters_sha256: String,
}

/// Host-captured identity of one proof request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct GraphProofCapture {
    /// `live.proof` node.
    pub node: u32,
    /// SHA-256 of the exact structured proof recipe.
    pub recipe_sha256: String,
}

/// Host-owned inputs binding one run to exact font, code, parameters and proofs.
///
/// Agent transports must ask the application to create this from captured state rather than
/// accepting these hashes as client assertions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct GraphRunCapture {
    /// One shared immutable base.
    pub font: GraphFontCapture,
    /// Exact Python recipe identities.
    pub scripts: Vec<GraphScriptCapture>,
    /// Exact specimen recipe identities.
    pub proofs: Vec<GraphProofCapture>,
}

/// Start one explicit live graph run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct GraphRunRequest {
    /// Layout-independent graph guard.
    pub guard: GraphSemanticGuard,
    /// Actor owning the retry key.
    pub actor: String,
    /// Idempotency key for this exact captured run.
    pub operation_key: String,
    /// Host-derived immutable capture identities.
    pub capture: GraphRunCapture,
}

/// Opaque session-local run handle.
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    schemars::JsonSchema,
)]
pub struct GraphRunHandle(u64);

impl GraphRunHandle {
    /// Monotonic diagnostic value.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// The only automatically executable topology in schema version 1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct GraphExecutionPlan {
    /// One `live.font` source node.
    pub source_node: u32,
    /// Proof directly connected to the source.
    pub unchanged_proof: u32,
    /// One `live.python` recipe node.
    pub python_node: u32,
    /// Proof connected to the Python result.
    pub changed_proof: u32,
}

/// Complete immutable identity a worker must echo on completion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct GraphRunIdentity {
    /// Exact graph and document lifetime.
    pub graph: GraphIdentity,
    /// Graph semantic revision at submission.
    pub semantic_revision: u64,
    /// Layout-independent graph hash at submission.
    pub semantic_hash: String,
    /// Exact host-captured font, script, parameter and proof identities.
    pub capture: GraphRunCapture,
}

/// Immutable work claimed by an application adapter.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GraphRunWork {
    /// Session-local handle.
    pub handle: GraphRunHandle,
    /// Identity to echo exactly on completion.
    pub identity: GraphRunIdentity,
    /// Canonical graph captured at submission.
    pub graph: NodeGraph,
    /// Validated first-phase execution plan.
    pub plan: GraphExecutionPlan,
}

/// Current retained state of a graph run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphRunStatus {
    /// Waiting for the application adapter.
    Queued,
    /// Existing Python or proof queues own active work.
    Running,
    /// Cancellation was requested from the owning queues.
    CancellationRequested,
    /// All required outputs completed and remain retained.
    Completed,
    /// A worker failed.
    Failed,
    /// Cancellation won and no outputs are published.
    Cancelled,
    /// Completion no longer matched graph, document or font identity.
    Stale,
    /// Heavy artifacts were explicitly released; retry identity remains retained.
    Released,
}

/// Honest proof lineage while compiled-family overlay integration remains separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphProofScope {
    /// Proof contains only the explicitly captured source experiment.
    SourceOnly,
    /// Proof came through the canonical full-family overlay compiler.
    CompiledFamily,
}

/// One retained node output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphNodeOutput {
    /// Producing node.
    pub node: u32,
    /// Typed output payload.
    pub value: GraphNodeOutputValue,
}

/// Data references published by existing queue owners.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphNodeOutputValue {
    /// Isolated version produced from the captured input.
    FontVersion {
        /// Same stable root source as the input.
        source: usize,
        /// Session-local version identity owned by the document experiment store.
        version_id: String,
        /// Revision of that isolated version.
        version_revision: u64,
        /// SHA-256 of the exact version content used downstream.
        content_sha256: String,
    },
    /// Exact proof artifact retained by the existing proof queue.
    Proof {
        /// Opaque proof handle or artifact identity.
        artifact_id: String,
        /// SHA-256 of exact image bytes.
        content_sha256: String,
        /// SHA-256 of the canonical whole-family compiler input.
        canonical_input_sha256: String,
        /// SHA-256 of the exact compiled font bytes rendered into the image.
        font_sha256: String,
        /// Exact proof recipe hash from the run identity.
        recipe_sha256: String,
        /// Source-only or compiled-family lineage.
        scope: GraphProofScope,
    },
    /// Bounded human-readable Python report.
    Report {
        /// UTF-8 report text.
        text: String,
    },
}

/// Structured terminal execution error.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphRunError {
    /// Stable application or worker error code.
    pub code: String,
    /// Human-readable detail.
    pub message: String,
    /// Responsible node, when known.
    pub node: Option<u32>,
    /// Responsible port, when known.
    pub port: Option<String>,
    /// Responsible field, when known.
    pub field: Option<String>,
}

/// Terminal result reported by queue adapters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum GraphRunOutcome {
    /// Complete typed outputs.
    Completed(Vec<GraphNodeOutput>),
    /// Failure without publishable output.
    Failed(Vec<GraphRunError>),
    /// Cancellation without publishable output.
    Cancelled,
}

/// Worker completion carrying the exact issued identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphRunCompletion {
    /// Run handle.
    pub handle: GraphRunHandle,
    /// Exact identity from [`GraphRunWork`].
    pub identity: GraphRunIdentity,
    /// Terminal worker result.
    pub outcome: GraphRunOutcome,
}

/// Current application context checked before output publication.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphDocumentState {
    /// Current document lifetime.
    pub document_epoch: String,
    /// Current canonical Project revision.
    pub document_revision: u64,
}

/// Read-only retained run state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphRunInspection {
    /// Run handle.
    pub handle: GraphRunHandle,
    /// Immutable run identity.
    pub identity: GraphRunIdentity,
    /// Current state.
    pub status: GraphRunStatus,
    /// Published outputs only when current and completed.
    pub outputs: Vec<GraphNodeOutput>,
    /// Terminal errors or stale reason.
    pub errors: Vec<GraphRunError>,
}

/// Immutable retained result of a run submission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphRunReceipt {
    /// Actor supplied by the request.
    pub actor: String,
    /// Operation key supplied by the request.
    pub operation_key: String,
    /// SHA-256 of the normalized capture request.
    pub payload_sha256: String,
    /// Original handle.
    pub handle: GraphRunHandle,
    /// Original immutable run identity.
    pub identity: GraphRunIdentity,
}

/// Response to a run submission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphRunResponse {
    /// New submission or exact retry.
    pub disposition: GraphReceiptDisposition,
    /// Immutable original submission result.
    pub receipt: GraphRunReceipt,
}

/// Actor-scoped cancellation request for one exact run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphCancelRequest {
    /// Exact graph and document lifetime.
    pub identity: GraphIdentity,
    /// Selected run only.
    pub handle: GraphRunHandle,
    /// Actor owning the retry key.
    pub actor: String,
    /// Idempotency key for this cancellation request.
    pub operation_key: String,
}

/// Original cancellation effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphCancelOutcome {
    /// Queued work was cancelled before an adapter claimed it.
    CancelledBeforeStart,
    /// Owning queues must be asked to stop this run.
    CancellationRequested,
    /// Run was already terminal.
    TooLate,
}

/// Immutable cancellation receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphCancelReceipt {
    /// Original effect.
    pub outcome: GraphCancelOutcome,
    /// Run state after the original request.
    pub status: GraphRunStatus,
}

/// Response to a cancellation request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GraphCancelResponse {
    /// New cancellation or exact retry.
    pub disposition: GraphReceiptDisposition,
    /// Immutable original result.
    pub receipt: GraphCancelReceipt,
}

#[derive(Clone, Debug)]
struct StoredRunReceipt {
    payload_sha256: String,
    receipt: GraphRunReceipt,
}

#[derive(Clone, Debug)]
struct StoredCancelReceipt {
    payload_sha256: String,
    receipt: GraphCancelReceipt,
}

#[derive(Clone, Debug)]
struct GraphRunRecord {
    identity: GraphRunIdentity,
    graph: NodeGraph,
    plan: GraphExecutionPlan,
    status: GraphRunStatus,
    outputs: Vec<GraphNodeOutput>,
    errors: Vec<GraphRunError>,
}

/// One reusable live graph service.
#[derive(Debug)]
pub struct GraphSession {
    identity: GraphIdentity,
    graph: NodeGraph,
    registry: Registry,
    revision: u64,
    semantic_revision: u64,
    semantic_hash: String,
    undo: Vec<NodeGraph>,
    redo: Vec<NodeGraph>,
    graph_receipts: BTreeMap<(String, String), StoredGraphReceipt>,
    next_run: u64,
    runs: BTreeMap<GraphRunHandle, GraphRunRecord>,
    run_receipts: BTreeMap<(String, String), StoredRunReceipt>,
    cancel_receipts: BTreeMap<(String, String), StoredCancelReceipt>,
}

impl GraphSession {
    /// Create a session without executing or modifying the supplied graph.
    pub fn new(
        session_id: impl Into<String>,
        document_epoch: impl Into<String>,
        graph: NodeGraph,
        registry: Registry,
    ) -> Result<Self, GraphSessionError> {
        let identity = GraphIdentity {
            session_id: session_id.into(),
            document_epoch: document_epoch.into(),
        };
        validate_identity(&identity)?;
        validate_graph_limits(&graph)?;
        let semantic_hash = semantic_hash(&graph)?;
        Ok(Self {
            identity,
            graph,
            registry,
            revision: 0,
            semantic_revision: 0,
            semantic_hash,
            undo: Vec::new(),
            redo: Vec::new(),
            graph_receipts: BTreeMap::new(),
            next_run: 1,
            runs: BTreeMap::new(),
            run_receipts: BTreeMap::new(),
            cancel_receipts: BTreeMap::new(),
        })
    }

    /// Discover actual supported node definitions and hard bounds.
    pub fn discovery(&self) -> GraphDiscovery {
        GraphDiscovery {
            schema_version: GRAPH_SESSION_SCHEMA_VERSION,
            node_types: self.registry.types.clone(),
            limits: GraphSessionLimits::current(),
            request_schemas: GraphRequestSchemas {
                interactive: serde_json::to_value(schemars::schema_for!(
                    GraphInteractiveMutationRequest
                ))
                .unwrap_or_default(),
                mutation: serde_json::to_value(schemars::schema_for!(GraphMutationRequest))
                    .unwrap_or_default(),
                run: serde_json::to_value(schemars::schema_for!(GraphRunRequest))
                    .unwrap_or_default(),
                cancel: serde_json::to_value(schemars::schema_for!(GraphCancelRequest))
                    .unwrap_or_default(),
            },
        }
    }

    /// Read the one canonical graph plus independent semantic identity.
    pub fn snapshot(&self) -> GraphSnapshot {
        GraphSnapshot {
            identity: self.identity.clone(),
            revision: self.revision,
            semantic_revision: self.semantic_revision,
            semantic_hash: self.semantic_hash.clone(),
            graph: self.graph.clone(),
            diagnostics: diagnostics(&self.graph, &self.registry),
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
        }
    }

    /// Derive script, parameter and proof identities from the canonical graph.
    ///
    /// The application supplies only the font capture and the input hash returned by its typed
    /// recipe capture.
    /// Agent transports should call this after capturing those values from live state rather than
    /// deserialize a caller-authored [`GraphRunCapture`].
    pub fn capture_run(
        &self,
        font: GraphFontCapture,
        script_input_sha256: impl Into<String>,
    ) -> Result<GraphRunCapture, GraphSessionError> {
        let graph_diagnostics = diagnostics(&self.graph, &self.registry);
        if !graph_diagnostics.is_empty() {
            let mut error = GraphSessionError::new(
                GraphSessionErrorCode::InvalidGraph,
                "graph validation failed; run capture is unavailable",
            );
            error.diagnostics = graph_diagnostics.into_boxed_slice();
            return Err(error);
        }
        let plan = execution_plan(&self.graph)?;
        validate_digest("font capture", &font.capture_sha256)?;
        let input_sha256 = script_input_sha256.into();
        validate_digest("recipe input", &input_sha256)?;
        let python = self.graph.node(plan.python_node).unwrap();
        let code = python
            .values
            .get("code")
            .and_then(Value::as_str)
            .filter(|code| !code.is_empty())
            .ok_or_else(|| {
                GraphSessionError::new(
                    GraphSessionErrorCode::CaptureMismatch,
                    "live.python code must be non-empty before Run",
                )
            })?;
        let parameters = python
            .values
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if !parameters.is_object() {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::CaptureMismatch,
                "live.python parameters must be a JSON object",
            ));
        }
        let proof_capture = |node: u32| -> Result<GraphProofCapture, GraphSessionError> {
            let recipe = self
                .graph
                .node(node)
                .and_then(|proof| proof.values.get("recipe"))
                .cloned()
                .unwrap_or_else(super::nodes_live::default_proof_recipe);
            if !recipe.is_object() {
                return Err(GraphSessionError::new(
                    GraphSessionErrorCode::CaptureMismatch,
                    "live.proof recipe must be a JSON object",
                ));
            }
            Ok(GraphProofCapture {
                node,
                recipe_sha256: digest_json(&recipe)?,
            })
        };
        let capture = GraphRunCapture {
            font,
            scripts: vec![GraphScriptCapture {
                node: plan.python_node,
                script_sha256: sha256(code.as_bytes()),
                input_sha256,
                parameters_sha256: digest_json(&parameters)?,
            }],
            proofs: vec![
                proof_capture(plan.unchanged_proof)?,
                proof_capture(plan.changed_proof)?,
            ],
        };
        validate_capture(&self.graph, &plan, &capture)?;
        Ok(capture)
    }

    /// Apply or replay one guarded graph-only mutation.
    pub fn mutate(
        &mut self,
        request: GraphMutationRequest,
    ) -> Result<GraphMutationResponse, GraphSessionError> {
        self.check_identity(&request.guard.identity)?;
        validate_actor_key(&request.actor, &request.operation_key)?;
        validate_mutation_bounds(&request)?;
        let payload_sha256 = digest_json(&(request.guard.clone(), &request.mutation))?;
        let ledger_key = (request.actor.clone(), request.operation_key.clone());
        if let Some(stored) = self.graph_receipts.get(&ledger_key) {
            if stored.payload_sha256 != payload_sha256 {
                return Err(GraphSessionError::new(
                    GraphSessionErrorCode::PayloadMismatch,
                    "graph operation_key was already used with another payload",
                ));
            }
            return Ok(GraphMutationResponse {
                disposition: GraphReceiptDisposition::Replayed,
                receipt: stored.receipt.clone(),
            });
        }
        if self.graph_receipts.len() >= MAX_GRAPH_RECEIPTS {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::ReceiptLimit,
                "graph mutation receipt limit reached for this document session",
            ));
        }
        let result = self.apply_mutation(&request.guard, request.mutation)?;

        let receipt = GraphMutationReceipt {
            actor: request.actor,
            operation_key: request.operation_key,
            payload_sha256: payload_sha256.clone(),
            changed: result.changed,
            semantic_changed: result.semantic_changed,
            snapshot: result.snapshot,
        };
        self.graph_receipts.insert(
            ledger_key,
            StoredGraphReceipt {
                payload_sha256,
                receipt: receipt.clone(),
            },
        );
        Ok(GraphMutationResponse {
            disposition: GraphReceiptDisposition::Applied,
            receipt,
        })
    }

    /// Apply one direct synchronous UI mutation without retaining a retry receipt.
    pub fn mutate_interactive(
        &mut self,
        request: GraphInteractiveMutationRequest,
    ) -> Result<GraphInteractiveMutationResult, GraphSessionError> {
        validate_interactive_mutation_bounds(&request)?;
        self.apply_mutation(&request.guard, request.mutation)
    }

    /// Validate and retain one explicit run without starting a worker.
    pub fn start_run(
        &mut self,
        request: GraphRunRequest,
    ) -> Result<GraphRunResponse, GraphSessionError> {
        self.check_identity(&request.guard.identity)?;
        validate_actor_key(&request.actor, &request.operation_key)?;
        let payload_sha256 = digest_json(&request)?;
        let ledger_key = (request.actor.clone(), request.operation_key.clone());
        if let Some(stored) = self.run_receipts.get(&ledger_key) {
            if stored.payload_sha256 != payload_sha256 {
                return Err(GraphSessionError::new(
                    GraphSessionErrorCode::PayloadMismatch,
                    "run operation_key was already used with another capture",
                ));
            }
            return Ok(GraphRunResponse {
                disposition: GraphReceiptDisposition::Replayed,
                receipt: stored.receipt.clone(),
            });
        }
        if self.run_receipts.len() >= MAX_RUN_RECEIPTS {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::ReceiptLimit,
                "run receipt limit reached for this document session",
            ));
        }
        self.check_semantics(&request.guard)?;
        let graph_diagnostics = diagnostics(&self.graph, &self.registry);
        if !graph_diagnostics.is_empty() {
            let mut error = GraphSessionError::new(
                GraphSessionErrorCode::InvalidGraph,
                "graph validation failed; no execution was queued",
            );
            error.diagnostics = graph_diagnostics.into_boxed_slice();
            return Err(error);
        }
        if self.active_run_count() >= MAX_ACTIVE_RUNS {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::RunLimit,
                "active graph run limit reached",
            ));
        }
        let plan = execution_plan(&self.graph)?;
        validate_capture(&self.graph, &plan, &request.capture)?;
        let identity = GraphRunIdentity {
            graph: self.identity.clone(),
            semantic_revision: self.semantic_revision,
            semantic_hash: self.semantic_hash.clone(),
            capture: request.capture,
        };
        let handle = GraphRunHandle(self.next_run);
        self.next_run = self.next_run.checked_add(1).unwrap_or(1);
        self.runs.insert(
            handle,
            GraphRunRecord {
                identity: identity.clone(),
                graph: self.graph.clone(),
                plan,
                status: GraphRunStatus::Queued,
                outputs: Vec::new(),
                errors: Vec::new(),
            },
        );
        let receipt = GraphRunReceipt {
            actor: request.actor,
            operation_key: request.operation_key,
            payload_sha256: payload_sha256.clone(),
            handle,
            identity,
        };
        self.run_receipts.insert(
            ledger_key,
            StoredRunReceipt {
                payload_sha256,
                receipt: receipt.clone(),
            },
        );
        Ok(GraphRunResponse {
            disposition: GraphReceiptDisposition::Applied,
            receipt,
        })
    }

    /// Claim one queued run for the existing Python and proof queue adapters.
    pub fn claim_run(&mut self, handle: GraphRunHandle) -> Result<GraphRunWork, GraphSessionError> {
        let record = self.run_mut(handle)?;
        if record.status != GraphRunStatus::Queued {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::WrongRunState,
                "only a queued graph run can be claimed",
            ));
        }
        record.status = GraphRunStatus::Running;
        Ok(GraphRunWork {
            handle,
            identity: record.identity.clone(),
            graph: record.graph.clone(),
            plan: record.plan.clone(),
        })
    }

    /// Apply or replay cancellation for one selected run only.
    pub fn cancel_run(
        &mut self,
        request: GraphCancelRequest,
    ) -> Result<GraphCancelResponse, GraphSessionError> {
        self.check_identity(&request.identity)?;
        validate_actor_key(&request.actor, &request.operation_key)?;
        let payload_sha256 = digest_json(&request)?;
        let ledger_key = (request.actor, request.operation_key);
        if let Some(stored) = self.cancel_receipts.get(&ledger_key) {
            if stored.payload_sha256 != payload_sha256 {
                return Err(GraphSessionError::new(
                    GraphSessionErrorCode::PayloadMismatch,
                    "cancel operation_key was already used with another run",
                ));
            }
            return Ok(GraphCancelResponse {
                disposition: GraphReceiptDisposition::Replayed,
                receipt: stored.receipt.clone(),
            });
        }
        if self.cancel_receipts.len() >= MAX_RUN_RECEIPTS {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::ReceiptLimit,
                "run cancellation receipt limit reached for this document session",
            ));
        }
        let record = self.run_mut(request.handle)?;
        let outcome = match record.status {
            GraphRunStatus::Queued => {
                record.status = GraphRunStatus::Cancelled;
                GraphCancelOutcome::CancelledBeforeStart
            }
            GraphRunStatus::Running => {
                record.status = GraphRunStatus::CancellationRequested;
                GraphCancelOutcome::CancellationRequested
            }
            GraphRunStatus::CancellationRequested
            | GraphRunStatus::Completed
            | GraphRunStatus::Failed
            | GraphRunStatus::Cancelled
            | GraphRunStatus::Stale
            | GraphRunStatus::Released => GraphCancelOutcome::TooLate,
        };
        let receipt = GraphCancelReceipt {
            outcome,
            status: record.status,
        };
        self.cancel_receipts.insert(
            ledger_key,
            StoredCancelReceipt {
                payload_sha256,
                receipt: receipt.clone(),
            },
        );
        Ok(GraphCancelResponse {
            disposition: GraphReceiptDisposition::Applied,
            receipt,
        })
    }

    /// Publish one exact worker completion if graph, document and font identity remain current.
    pub fn complete_run(
        &mut self,
        completion: GraphRunCompletion,
        current: &GraphDocumentState,
    ) -> Result<GraphRunInspection, GraphSessionError> {
        let semantic_revision = self.semantic_revision;
        let semantic_hash = self.semantic_hash.clone();
        let record = self.run_mut(completion.handle)?;
        if record.identity != completion.identity {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::CaptureMismatch,
                "graph run completion did not echo the issued identity",
            ));
        }
        if !matches!(
            record.status,
            GraphRunStatus::Running | GraphRunStatus::CancellationRequested
        ) {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::WrongRunState,
                "only a running graph job can complete",
            ));
        }
        if record.status == GraphRunStatus::CancellationRequested {
            record.status = GraphRunStatus::Cancelled;
            record.outputs.clear();
            record.errors.clear();
            return Ok(inspection(completion.handle, record));
        }
        if current.document_epoch != record.identity.graph.document_epoch
            || current.document_revision != record.identity.capture.font.document_revision
            || semantic_revision != record.identity.semantic_revision
            || semantic_hash != record.identity.semantic_hash
        {
            record.status = GraphRunStatus::Stale;
            record.outputs.clear();
            record.errors = vec![GraphRunError {
                code: "stale_run".into(),
                message: "graph, document or font changed before completion".into(),
                node: None,
                port: None,
                field: None,
            }];
            return Ok(inspection(completion.handle, record));
        }
        match completion.outcome {
            GraphRunOutcome::Completed(outputs) => {
                if let Err(error) = validate_outputs(record, &outputs) {
                    terminalize_invalid_completion(record, error);
                    return Ok(inspection(completion.handle, record));
                }
                record.status = GraphRunStatus::Completed;
                record.outputs = outputs;
                record.errors.clear();
            }
            GraphRunOutcome::Failed(errors) => {
                if let Err(error) = validate_run_errors(&errors) {
                    terminalize_invalid_completion(record, error);
                    return Ok(inspection(completion.handle, record));
                }
                record.status = GraphRunStatus::Failed;
                record.outputs.clear();
                record.errors = errors;
            }
            GraphRunOutcome::Cancelled => {
                record.status = GraphRunStatus::Cancelled;
                record.outputs.clear();
                record.errors.clear();
            }
        }
        Ok(inspection(completion.handle, record))
    }

    /// Inspect retained status and current output without consuming it.
    pub fn inspect_run(&self, handle: GraphRunHandle) -> Option<GraphRunInspection> {
        self.runs
            .get(&handle)
            .map(|record| inspection(handle, record))
    }

    /// Release heavy outputs while retaining the operation receipt and a tombstone.
    pub fn release_run(&mut self, handle: GraphRunHandle) -> Result<bool, GraphSessionError> {
        let record = self.run_mut(handle)?;
        if matches!(
            record.status,
            GraphRunStatus::Queued
                | GraphRunStatus::Running
                | GraphRunStatus::CancellationRequested
        ) {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::WrongRunState,
                "an active graph run cannot be released",
            ));
        }
        if record.status == GraphRunStatus::Released {
            return Ok(false);
        }
        record.outputs.clear();
        record.errors.clear();
        record.graph.nodes.clear();
        record.graph.links.clear();
        record.status = GraphRunStatus::Released;
        Ok(true)
    }

    fn check_identity(&self, identity: &GraphIdentity) -> Result<(), GraphSessionError> {
        validate_identity(identity)?;
        if identity != &self.identity {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::WrongSession,
                "request does not name this graph and document lifetime",
            ));
        }
        Ok(())
    }

    fn apply_mutation(
        &mut self,
        guard: &GraphGuard,
        mutation: GraphMutation,
    ) -> Result<GraphInteractiveMutationResult, GraphSessionError> {
        self.check_identity(&guard.identity)?;
        if guard.revision != self.revision {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::StaleRevision,
                format!(
                    "stale graph revision {}; current revision is {}",
                    guard.revision, self.revision
                ),
            ));
        }
        let before = self.graph.clone();
        let old_hash = self.semantic_hash.clone();
        let candidate = match &mutation {
            GraphMutation::Patch { edits } => apply_patch(&self.graph, edits)?,
            GraphMutation::Undo => self.undo.last().cloned().ok_or_else(|| {
                GraphSessionError::new(GraphSessionErrorCode::HistoryEmpty, "graph undo is empty")
            })?,
            GraphMutation::Redo => self.redo.last().cloned().ok_or_else(|| {
                GraphSessionError::new(GraphSessionErrorCode::HistoryEmpty, "graph redo is empty")
            })?,
        };
        validate_graph_limits(&candidate)?;
        let changed = candidate != self.graph;
        let new_hash = semantic_hash(&candidate)?;
        let semantic_changed = changed && new_hash != old_hash;
        if changed {
            match mutation {
                GraphMutation::Patch { .. } => {
                    push_bounded(&mut self.undo, before);
                    self.redo.clear();
                }
                GraphMutation::Undo => {
                    self.undo.pop();
                    push_bounded(&mut self.redo, before);
                }
                GraphMutation::Redo => {
                    self.redo.pop();
                    push_bounded(&mut self.undo, before);
                }
            }
            self.graph = candidate;
            self.revision = self.revision.wrapping_add(1);
            if semantic_changed {
                self.semantic_revision = self.semantic_revision.wrapping_add(1);
                self.semantic_hash = new_hash;
            }
        }
        Ok(GraphInteractiveMutationResult {
            changed,
            semantic_changed,
            snapshot: self.snapshot(),
        })
    }

    fn check_semantics(&self, guard: &GraphSemanticGuard) -> Result<(), GraphSessionError> {
        if guard.semantic_revision != self.semantic_revision
            || guard.semantic_hash != self.semantic_hash
        {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::StaleSemantics,
                "graph execution guard is stale",
            ));
        }
        Ok(())
    }

    fn active_run_count(&self) -> usize {
        self.runs
            .values()
            .filter(|record| {
                matches!(
                    record.status,
                    GraphRunStatus::Queued
                        | GraphRunStatus::Running
                        | GraphRunStatus::CancellationRequested
                )
            })
            .count()
    }

    fn run_mut(
        &mut self,
        handle: GraphRunHandle,
    ) -> Result<&mut GraphRunRecord, GraphSessionError> {
        self.runs.get_mut(&handle).ok_or_else(|| {
            GraphSessionError::new(
                GraphSessionErrorCode::UnknownRun,
                "unknown graph run handle",
            )
        })
    }
}

fn apply_patch(graph: &NodeGraph, edits: &[GraphEdit]) -> Result<NodeGraph, GraphSessionError> {
    let mut candidate = graph.clone();
    for (index, edit) in edits.iter().enumerate() {
        match edit {
            GraphEdit::AddNode { node } => {
                if candidate.node(node.id).is_some() {
                    return Err(GraphSessionError::edit(
                        index,
                        Some(node.id),
                        "node id already exists",
                    ));
                }
                if !finite_pos(node.pos) {
                    return Err(GraphSessionError::edit(
                        index,
                        Some(node.id),
                        "node position must be finite",
                    ));
                }
                candidate.nodes.push(node.clone());
            }
            GraphEdit::RemoveNode { node } => {
                if candidate.node(*node).is_none() {
                    return Err(GraphSessionError::edit(
                        index,
                        Some(*node),
                        "cannot remove a missing node",
                    ));
                }
                candidate.remove(*node);
            }
            GraphEdit::MoveNode { node, pos } => {
                if !finite_pos(*pos) {
                    return Err(GraphSessionError::edit(
                        index,
                        Some(*node),
                        "node position must be finite",
                    ));
                }
                let target = candidate.node_mut(*node).ok_or_else(|| {
                    GraphSessionError::edit(index, Some(*node), "cannot move a missing node")
                })?;
                target.pos = *pos;
            }
            GraphEdit::SetValue { node, field, value } => {
                if field.is_empty() || field.len() > MAX_ID_BYTES {
                    let mut error = GraphSessionError::edit(
                        index,
                        Some(*node),
                        "node field must contain 1..=128 UTF-8 bytes",
                    );
                    error.field = Some(field.clone());
                    return Err(error);
                }
                let target = candidate.node_mut(*node).ok_or_else(|| {
                    GraphSessionError::edit(index, Some(*node), "cannot edit a missing node")
                })?;
                target.values.insert(field.clone(), value.clone());
            }
            GraphEdit::RemoveValue { node, field } => {
                let target = candidate.node_mut(*node).ok_or_else(|| {
                    GraphSessionError::edit(index, Some(*node), "cannot edit a missing node")
                })?;
                target.values.remove(field);
            }
            GraphEdit::Connect { link } => {
                if candidate.node(link.from()).is_none() || candidate.node(link.to()).is_none() {
                    return Err(GraphSessionError::edit(
                        index,
                        Some(link.to()),
                        "cannot connect a missing node",
                    ));
                }
                candidate.connect(link.from(), link.output(), link.to(), link.input());
            }
            GraphEdit::Disconnect { node, input } => {
                if candidate.node(*node).is_none() {
                    return Err(GraphSessionError::edit(
                        index,
                        Some(*node),
                        "cannot disconnect a missing node",
                    ));
                }
                candidate
                    .links
                    .retain(|link| link.to() != *node || link.input() != input);
            }
        }
        validate_graph_limits(&candidate)?;
    }
    Ok(candidate)
}

fn validate_graph_limits(graph: &NodeGraph) -> Result<(), GraphSessionError> {
    if graph.nodes.len() > MAX_GRAPH_NODES {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::GraphLimit,
            format!("graph exceeds {MAX_GRAPH_NODES} nodes"),
        ));
    }
    if graph.links.len() > MAX_GRAPH_LINKS {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::GraphLimit,
            format!("graph exceeds {MAX_GRAPH_LINKS} links"),
        ));
    }
    let bytes = serde_json::to_vec(graph).map_err(|error| {
        GraphSessionError::new(
            GraphSessionErrorCode::GraphLimit,
            format!("could not measure graph: {error}"),
        )
    })?;
    if bytes.len() > MAX_GRAPH_BYTES {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::GraphLimit,
            format!("graph exceeds {MAX_GRAPH_BYTES} serialized bytes"),
        ));
    }
    for node in &graph.nodes {
        if node.type_name.is_empty() || node.type_name.len() > MAX_ID_BYTES {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::GraphLimit,
                "node type names must contain 1..=128 UTF-8 bytes",
            ));
        }
        for (field, value) in &node.values {
            if field.is_empty() || field.len() > MAX_ID_BYTES {
                return Err(GraphSessionError::new(
                    GraphSessionErrorCode::GraphLimit,
                    "node field names must contain 1..=128 UTF-8 bytes",
                ));
            }
            if node.type_name == "live.python" && field == "code" {
                let Some(code) = value.as_str() else {
                    continue;
                };
                if code.len() > MAX_CODE_BYTES {
                    return Err(GraphSessionError::new(
                        GraphSessionErrorCode::GraphLimit,
                        format!("Python code exceeds {MAX_CODE_BYTES} UTF-8 bytes"),
                    ));
                }
            }
            if node.type_name == "live.python" && field == "parameters" {
                let parameter_bytes = serde_json::to_vec(value).map_err(|error| {
                    GraphSessionError::new(
                        GraphSessionErrorCode::GraphLimit,
                        format!("could not measure Python parameters: {error}"),
                    )
                })?;
                if parameter_bytes.len() > MAX_PARAMETERS_BYTES {
                    return Err(GraphSessionError::new(
                        GraphSessionErrorCode::GraphLimit,
                        format!("Python parameters exceed {MAX_PARAMETERS_BYTES} serialized bytes"),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_mutation_bounds(request: &GraphMutationRequest) -> Result<(), GraphSessionError> {
    validate_mutation_shape(&request.mutation)?;
    validate_serialized_mutation(request)
}

fn validate_interactive_mutation_bounds(
    request: &GraphInteractiveMutationRequest,
) -> Result<(), GraphSessionError> {
    validate_mutation_shape(&request.mutation)?;
    validate_serialized_mutation(request)
}

fn validate_mutation_shape(mutation: &GraphMutation) -> Result<(), GraphSessionError> {
    if let GraphMutation::Patch { edits } = mutation
        && edits.len() > MAX_PATCH_EDITS
    {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::GraphLimit,
            format!("graph patch exceeds {MAX_PATCH_EDITS} edits"),
        ));
    }
    Ok(())
}

fn validate_serialized_mutation(value: &impl Serialize) -> Result<(), GraphSessionError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        GraphSessionError::new(
            GraphSessionErrorCode::GraphLimit,
            format!("could not measure graph mutation: {error}"),
        )
    })?;
    if bytes.len() > MAX_MUTATION_BYTES {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::GraphLimit,
            format!("graph mutation exceeds {MAX_MUTATION_BYTES} serialized bytes"),
        ));
    }
    Ok(())
}

fn validate_identity(identity: &GraphIdentity) -> Result<(), GraphSessionError> {
    validate_bounded("session_id", &identity.session_id, MAX_ID_BYTES)?;
    validate_bounded("document_epoch", &identity.document_epoch, MAX_ID_BYTES)
}

fn validate_actor_key(actor: &str, operation_key: &str) -> Result<(), GraphSessionError> {
    validate_bounded("actor", actor, MAX_ID_BYTES)?;
    validate_bounded("operation_key", operation_key, MAX_OPERATION_KEY_BYTES)
}

fn validate_bounded(name: &str, value: &str, max: usize) -> Result<(), GraphSessionError> {
    if value.is_empty() || value.len() > max {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::InvalidIdentity,
            format!("{name} must contain 1..={max} UTF-8 bytes"),
        ));
    }
    Ok(())
}

fn diagnostics(graph: &NodeGraph, registry: &Registry) -> Vec<GraphDiagnostic> {
    graph
        .validate(registry)
        .iter()
        .map(GraphDiagnostic::from)
        .collect()
}

fn finite_pos(pos: [f32; 2]) -> bool {
    pos.into_iter().all(f32::is_finite)
}

fn push_bounded(history: &mut Vec<NodeGraph>, graph: NodeGraph) {
    if history.len() == MAX_GRAPH_HISTORY {
        history.remove(0);
    }
    history.push(graph);
}

#[derive(Serialize)]
struct SemanticGraph {
    version: u32,
    nodes: Vec<SemanticNode>,
    links: Vec<Link>,
}

#[derive(Serialize)]
struct SemanticNode {
    id: u32,
    type_name: String,
    values: BTreeMap<String, Value>,
}

fn semantic_hash(graph: &NodeGraph) -> Result<String, GraphSessionError> {
    let mut nodes: Vec<SemanticNode> = graph
        .nodes
        .iter()
        .map(|node| SemanticNode {
            id: node.id,
            type_name: node.type_name.clone(),
            values: node.values.clone(),
        })
        .collect();
    nodes.sort_by_key(|node| node.id);
    let mut links = graph.links.clone();
    links.sort_by(|left, right| {
        (left.from(), left.output(), left.to(), left.input()).cmp(&(
            right.from(),
            right.output(),
            right.to(),
            right.input(),
        ))
    });
    digest_json(&SemanticGraph {
        version: graph.version,
        nodes,
        links,
    })
}

fn digest_json(value: &impl Serialize) -> Result<String, GraphSessionError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        GraphSessionError::new(
            GraphSessionErrorCode::InvalidIdentity,
            format!("could not encode graph identity: {error}"),
        )
    })?;
    Ok(hex_digest(Sha256::digest(bytes)))
}

fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn execution_plan(graph: &NodeGraph) -> Result<GraphExecutionPlan, GraphSessionError> {
    if graph.nodes.iter().any(|node| {
        matches!(
            node.type_name.as_str(),
            "live.apply" | "core.install" | "live.fork"
        )
    }) {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::UnsupportedTopology,
            "automatic live execution rejects apply, install and pre-existing fork nodes",
        ));
    }
    let nodes_of_type = |name: &str| {
        graph
            .nodes
            .iter()
            .filter(|node| node.type_name == name)
            .map(|node| node.id)
            .collect::<Vec<_>>()
    };
    let sources = nodes_of_type("live.font");
    let scripts = nodes_of_type("live.python");
    let proofs = nodes_of_type("live.proof");
    if sources.len() != 1 || scripts.len() != 1 || proofs.len() != 2 {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::UnsupportedTopology,
            "schema version 1 runs exactly one live.font, one live.python and two live.proof nodes",
        ));
    }
    if graph.nodes.len() != 4 || graph.links.len() != 3 {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::UnsupportedTopology,
            "schema version 1 runs only the four-node comparison graph",
        ));
    }
    let source_node = sources[0];
    let python_node = scripts[0];
    let direct = graph
        .link_into(proofs[0], "font")
        .map(Link::from)
        .ok_or_else(|| {
            GraphSessionError::new(
                GraphSessionErrorCode::UnsupportedTopology,
                "each proof must have one connected font input",
            )
        })?;
    let second = graph
        .link_into(proofs[1], "font")
        .map(Link::from)
        .ok_or_else(|| {
            GraphSessionError::new(
                GraphSessionErrorCode::UnsupportedTopology,
                "each proof must have one connected font input",
            )
        })?;
    let (unchanged_proof, changed_proof) = match (direct, second) {
        (from, other) if from == source_node && other == python_node => (proofs[0], proofs[1]),
        (from, other) if from == python_node && other == source_node => (proofs[1], proofs[0]),
        _ => {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::UnsupportedTopology,
                "proofs must compare the shared base with the Python-derived version",
            ));
        }
    };
    let python_input = graph
        .link_into(python_node, "font")
        .map(Link::from)
        .ok_or_else(|| {
            GraphSessionError::new(
                GraphSessionErrorCode::UnsupportedTopology,
                "Python recipe must read the shared base font",
            )
        })?;
    if python_input != source_node {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::UnsupportedTopology,
            "Python recipe and unchanged proof must share one base font",
        ));
    }
    Ok(GraphExecutionPlan {
        source_node,
        unchanged_proof,
        python_node,
        changed_proof,
    })
}

fn validate_capture(
    graph: &NodeGraph,
    plan: &GraphExecutionPlan,
    capture: &GraphRunCapture,
) -> Result<(), GraphSessionError> {
    validate_digest("font capture", &capture.font.capture_sha256)?;
    let source = graph
        .node(plan.source_node)
        .and_then(|node| node.values.get("source"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            GraphSessionError::new(
                GraphSessionErrorCode::CaptureMismatch,
                "live.font must name one stable source",
            )
        })?;
    if source != capture.font.source {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::CaptureMismatch,
            "font capture source does not match live.font",
        ));
    }
    if capture.scripts.len() != 1 || capture.scripts[0].node != plan.python_node {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::CaptureMismatch,
            "run must bind exactly the one live.python node",
        ));
    }
    let script = &capture.scripts[0];
    validate_digest("script", &script.script_sha256)?;
    validate_digest("recipe input", &script.input_sha256)?;
    validate_digest("parameters", &script.parameters_sha256)?;
    let python = graph.node(plan.python_node).unwrap();
    let code = python
        .values
        .get("code")
        .and_then(Value::as_str)
        .filter(|code| !code.is_empty())
        .ok_or_else(|| {
            GraphSessionError::new(
                GraphSessionErrorCode::CaptureMismatch,
                "live.python code must be non-empty before Run",
            )
        })?;
    if sha256(code.as_bytes()) != script.script_sha256 {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::CaptureMismatch,
            "script capture does not match live.python code",
        ));
    }
    let parameters = python
        .values
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if !parameters.is_object() || digest_json(&parameters)? != script.parameters_sha256 {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::CaptureMismatch,
            "parameter capture does not match live.python parameters",
        ));
    }
    let expected_proofs = BTreeSet::from([plan.unchanged_proof, plan.changed_proof]);
    let actual_proofs: BTreeSet<u32> = capture.proofs.iter().map(|proof| proof.node).collect();
    if capture.proofs.len() != 2 || actual_proofs != expected_proofs {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::CaptureMismatch,
            "run must bind exactly the two comparison proofs",
        ));
    }
    for proof in &capture.proofs {
        validate_digest("proof recipe", &proof.recipe_sha256)?;
        let recipe = graph
            .node(proof.node)
            .and_then(|node| node.values.get("recipe"))
            .cloned()
            .unwrap_or_else(super::nodes_live::default_proof_recipe);
        if !recipe.is_object() || digest_json(&recipe)? != proof.recipe_sha256 {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::CaptureMismatch,
                "proof capture does not match live.proof recipe",
            ));
        }
    }
    if capture.proofs[0].recipe_sha256 != capture.proofs[1].recipe_sha256 {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::CaptureMismatch,
            "comparison proofs must use identical rendering settings",
        ));
    }
    Ok(())
}

fn validate_digest(name: &str, value: &str) -> Result<(), GraphSessionError> {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::CaptureMismatch,
            format!("{name} identity must be a lowercase SHA-256 digest"),
        ));
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes))
}

fn validate_outputs(
    record: &GraphRunRecord,
    outputs: &[GraphNodeOutput],
) -> Result<(), GraphSessionError> {
    if outputs.len() > MAX_RUN_OUTPUTS {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::InvalidOutput,
            format!("run exceeds {MAX_RUN_OUTPUTS} retained outputs"),
        ));
    }
    let derived_input_sha256 = outputs.iter().find_map(|output| match &output.value {
        GraphNodeOutputValue::FontVersion { content_sha256, .. }
            if output.node == record.plan.python_node =>
        {
            Some(content_sha256.as_str())
        }
        _ => None,
    });
    let mut unique = BTreeSet::new();
    for output in outputs {
        let kind = match &output.value {
            GraphNodeOutputValue::FontVersion { .. } => 0_u8,
            GraphNodeOutputValue::Proof { .. } => 1,
            GraphNodeOutputValue::Report { .. } => 2,
        };
        if !unique.insert((output.node, kind)) {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::InvalidOutput,
                "run repeats an output kind for one node",
            ));
        }
        match &output.value {
            GraphNodeOutputValue::FontVersion {
                source,
                version_id,
                content_sha256,
                ..
            } => {
                if output.node != record.plan.python_node
                    || *source != record.identity.capture.font.source
                {
                    return Err(GraphSessionError::new(
                        GraphSessionErrorCode::InvalidOutput,
                        "derived font output does not match the Python node and root source",
                    ));
                }
                validate_bounded("version_id", version_id, MAX_ID_BYTES)?;
                validate_digest("font version", content_sha256)?;
            }
            GraphNodeOutputValue::Proof {
                artifact_id,
                content_sha256,
                canonical_input_sha256,
                font_sha256,
                recipe_sha256,
                scope,
            } => {
                let Some(capture) = record
                    .identity
                    .capture
                    .proofs
                    .iter()
                    .find(|proof| proof.node == output.node)
                else {
                    return Err(GraphSessionError::new(
                        GraphSessionErrorCode::InvalidOutput,
                        "proof output does not belong to this run",
                    ));
                };
                validate_bounded("artifact_id", artifact_id, MAX_ID_BYTES)?;
                validate_digest("proof artifact", content_sha256)?;
                validate_digest("proof canonical input", canonical_input_sha256)?;
                validate_digest("proof compiled font", font_sha256)?;
                if recipe_sha256 != &capture.recipe_sha256 {
                    return Err(GraphSessionError::new(
                        GraphSessionErrorCode::InvalidOutput,
                        "proof output recipe does not match the captured run",
                    ));
                }
                if *scope != GraphProofScope::CompiledFamily {
                    return Err(GraphSessionError::new(
                        GraphSessionErrorCode::InvalidOutput,
                        "comparison proof must come from the canonical compiled-family path",
                    ));
                }
                let expected_input = if output.node == record.plan.unchanged_proof {
                    record.identity.capture.font.capture_sha256.as_str()
                } else if output.node == record.plan.changed_proof {
                    derived_input_sha256.ok_or_else(|| {
                        GraphSessionError::new(
                            GraphSessionErrorCode::InvalidOutput,
                            "derived proof has no matching font-version identity",
                        )
                    })?
                } else {
                    return Err(GraphSessionError::new(
                        GraphSessionErrorCode::InvalidOutput,
                        "proof output does not belong to either comparison branch",
                    ));
                };
                if canonical_input_sha256 != expected_input {
                    return Err(GraphSessionError::new(
                        GraphSessionErrorCode::InvalidOutput,
                        "proof canonical input does not match its comparison branch",
                    ));
                }
            }
            GraphNodeOutputValue::Report { text } => {
                if output.node != record.plan.python_node || text.len() > MAX_OUTPUT_TEXT_BYTES {
                    return Err(GraphSessionError::new(
                        GraphSessionErrorCode::InvalidOutput,
                        "Python report has the wrong node or exceeds 65536 UTF-8 bytes",
                    ));
                }
            }
        }
    }
    let has_version = outputs.iter().any(|output| {
        output.node == record.plan.python_node
            && matches!(output.value, GraphNodeOutputValue::FontVersion { .. })
    });
    let proof_nodes: BTreeSet<u32> = outputs
        .iter()
        .filter_map(|output| {
            matches!(output.value, GraphNodeOutputValue::Proof { .. }).then_some(output.node)
        })
        .collect();
    if !has_version
        || proof_nodes != BTreeSet::from([record.plan.unchanged_proof, record.plan.changed_proof])
    {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::InvalidOutput,
            "completed comparison run requires one derived font and both proof outputs",
        ));
    }
    Ok(())
}

fn terminalize_invalid_completion(record: &mut GraphRunRecord, error: GraphSessionError) {
    let mut message = error.message;
    while message.len() > MAX_ERROR_BYTES {
        message.pop();
    }
    record.status = GraphRunStatus::Failed;
    record.outputs.clear();
    record.errors = vec![GraphRunError {
        code: "invalid_worker_output".into(),
        message,
        node: error.node,
        port: error.port,
        field: error.field,
    }];
}

fn validate_run_errors(errors: &[GraphRunError]) -> Result<(), GraphSessionError> {
    if errors.is_empty() || errors.len() > MAX_RUN_ERRORS {
        return Err(GraphSessionError::new(
            GraphSessionErrorCode::InvalidOutput,
            format!("failed run must contain 1..={MAX_RUN_ERRORS} structured errors"),
        ));
    }
    for error in errors {
        validate_bounded("run error code", &error.code, MAX_ID_BYTES)?;
        if error.message.is_empty() || error.message.len() > MAX_ERROR_BYTES {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::InvalidOutput,
                format!("run error message must contain 1..={MAX_ERROR_BYTES} UTF-8 bytes"),
            ));
        }
        if error
            .port
            .as_ref()
            .is_some_and(|port| port.len() > MAX_ID_BYTES)
            || error
                .field
                .as_ref()
                .is_some_and(|field| field.len() > MAX_ID_BYTES)
        {
            return Err(GraphSessionError::new(
                GraphSessionErrorCode::InvalidOutput,
                "run error port and field names must not exceed 128 UTF-8 bytes",
            ));
        }
    }
    Ok(())
}

fn inspection(handle: GraphRunHandle, record: &GraphRunRecord) -> GraphRunInspection {
    GraphRunInspection {
        handle,
        identity: record.identity.clone(),
        status: record.status,
        outputs: record.outputs.clone(),
        errors: record.errors.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::nodes_live;
    use super::*;
    use crate::font::variable;
    use serde_json::json;

    fn new_session() -> GraphSession {
        let mut graph = nodes_live::comparison_starter(variable::SourceId(3));
        graph
            .node_mut(3)
            .unwrap()
            .values
            .insert("code".into(), json!("print('recipe')"));
        GraphSession::new("graph-1", "document-1", graph, Registry::core()).unwrap()
    }

    fn guard(session: &GraphSession) -> GraphGuard {
        let snapshot = session.snapshot();
        GraphGuard {
            identity: snapshot.identity,
            revision: snapshot.revision,
        }
    }

    fn semantic_guard(session: &GraphSession) -> GraphSemanticGuard {
        let snapshot = session.snapshot();
        GraphSemanticGuard {
            identity: snapshot.identity,
            semantic_revision: snapshot.semantic_revision,
            semantic_hash: snapshot.semantic_hash,
        }
    }

    fn mutation(
        session: &GraphSession,
        actor: &str,
        key: &str,
        edits: Vec<GraphEdit>,
    ) -> GraphMutationRequest {
        GraphMutationRequest {
            guard: guard(session),
            actor: actor.into(),
            operation_key: key.into(),
            mutation: GraphMutation::Patch { edits },
        }
    }

    fn run_request(session: &GraphSession, actor: &str, key: &str) -> GraphRunRequest {
        GraphRunRequest {
            guard: semantic_guard(session),
            actor: actor.into(),
            operation_key: key.into(),
            capture: session
                .capture_run(
                    GraphFontCapture {
                        source: 3,
                        document_revision: 12,
                        capture_sha256: sha256(b"font-capture"),
                    },
                    sha256(b"recipe-input"),
                )
                .unwrap(),
        }
    }

    fn outputs(identity: &GraphRunIdentity) -> Vec<GraphNodeOutput> {
        let recipe = identity.capture.proofs[0].recipe_sha256.clone();
        let derived_input_sha256 = sha256(b"derived-input");
        vec![
            GraphNodeOutput {
                node: 3,
                value: GraphNodeOutputValue::FontVersion {
                    source: 3,
                    version_id: "version-1".into(),
                    version_revision: 1,
                    content_sha256: derived_input_sha256.clone(),
                },
            },
            GraphNodeOutput {
                node: 2,
                value: GraphNodeOutputValue::Proof {
                    artifact_id: "proof-base".into(),
                    content_sha256: sha256(b"base-png"),
                    canonical_input_sha256: identity.capture.font.capture_sha256.clone(),
                    font_sha256: sha256(b"base-font"),
                    recipe_sha256: recipe.clone(),
                    scope: GraphProofScope::CompiledFamily,
                },
            },
            GraphNodeOutput {
                node: 4,
                value: GraphNodeOutputValue::Proof {
                    artifact_id: "proof-derived".into(),
                    content_sha256: sha256(b"derived-png"),
                    canonical_input_sha256: derived_input_sha256,
                    font_sha256: sha256(b"derived-font"),
                    recipe_sha256: recipe,
                    scope: GraphProofScope::CompiledFamily,
                },
            },
        ]
    }

    #[test]
    fn layout_changes_keep_semantic_identity_and_do_not_stale_outputs() {
        let mut session = new_session();
        let before = session.snapshot();
        let response = session
            .mutate(mutation(
                &session,
                "human",
                "move-proof",
                vec![GraphEdit::MoveNode {
                    node: 2,
                    pos: [800.0, 40.0],
                }],
            ))
            .unwrap();
        assert!(response.receipt.changed);
        assert!(!response.receipt.semantic_changed);
        assert_eq!(
            response.receipt.snapshot.semantic_hash,
            before.semantic_hash
        );
        assert_eq!(
            response.receipt.snapshot.semantic_revision,
            before.semantic_revision
        );
        assert_eq!(response.receipt.snapshot.revision, before.revision + 1);

        let start = session
            .start_run(run_request(&session, "human", "run-layout"))
            .unwrap();
        let work = session.claim_run(start.receipt.handle).unwrap();
        session
            .mutate(mutation(
                &session,
                "human",
                "move-again",
                vec![GraphEdit::MoveNode {
                    node: 4,
                    pos: [900.0, 420.0],
                }],
            ))
            .unwrap();
        let inspection = session
            .complete_run(
                GraphRunCompletion {
                    handle: work.handle,
                    identity: work.identity.clone(),
                    outcome: GraphRunOutcome::Completed(outputs(&work.identity)),
                },
                &GraphDocumentState {
                    document_epoch: "document-1".into(),
                    document_revision: 12,
                },
            )
            .unwrap();
        assert_eq!(inspection.status, GraphRunStatus::Completed);
    }

    #[test]
    fn stale_agent_patch_cannot_replace_a_human_edit() {
        let mut session = new_session();
        let stale = guard(&session);
        session
            .mutate(mutation(
                &session,
                "human",
                "edit-code",
                vec![GraphEdit::SetValue {
                    node: 3,
                    field: "code".into(),
                    value: json!("print('human')"),
                }],
            ))
            .unwrap();
        let error = session
            .mutate(GraphMutationRequest {
                guard: stale,
                actor: "agent".into(),
                operation_key: "edit-code".into(),
                mutation: GraphMutation::Patch {
                    edits: vec![GraphEdit::SetValue {
                        node: 3,
                        field: "code".into(),
                        value: json!("print('agent')"),
                    }],
                },
            })
            .unwrap_err();
        assert_eq!(error.code, GraphSessionErrorCode::StaleRevision);
        assert_eq!(
            session.snapshot().graph.node(3).unwrap().values["code"],
            json!("print('human')")
        );
    }

    #[test]
    fn invalid_types_links_and_cycles_have_structured_addresses() {
        let mut graph = nodes_live::comparison_starter(variable::SourceId(0));
        graph.node_mut(3).unwrap().type_name = "missing.type".into();
        graph
            .links
            .push(Link(2, "missing".into(), 1, "missing".into()));
        graph.links.push(Link(4, "font".into(), 3, "font".into()));
        let session = GraphSession::new("graph", "document", graph, Registry::core()).unwrap();
        let diagnostics = session.snapshot().diagnostics;
        assert!(diagnostics.iter().any(|problem| {
            problem.code == GraphDiagnosticCode::UnknownType && problem.node == Some(3)
        }));
        assert!(diagnostics.iter().any(|problem| {
            problem.code == GraphDiagnosticCode::UnknownPort
                && problem.node == Some(2)
                && problem.port.as_deref() == Some("missing")
        }));
        assert!(
            diagnostics
                .iter()
                .any(|problem| problem.code == GraphDiagnosticCode::Cycle)
        );
    }

    #[test]
    fn a_failed_multi_edit_patch_is_atomic() {
        let mut session = new_session();
        let before = session.snapshot();
        let error = session
            .mutate(mutation(
                &session,
                "human",
                "bad-batch",
                vec![
                    GraphEdit::MoveNode {
                        node: 2,
                        pos: [900.0, 10.0],
                    },
                    GraphEdit::SetValue {
                        node: 999,
                        field: "code".into(),
                        value: json!("bad"),
                    },
                ],
            ))
            .unwrap_err();
        assert_eq!(error.edit_index, Some(1));
        assert_eq!(session.snapshot(), before);
    }

    #[test]
    fn oversized_code_and_patch_reject_without_consuming_receipts() {
        let mut session = new_session();
        let before = session.snapshot();
        let oversized = session
            .mutate(mutation(
                &session,
                "agent",
                "oversized-code",
                vec![GraphEdit::SetValue {
                    node: 3,
                    field: "code".into(),
                    value: json!("x".repeat(MAX_CODE_BYTES + 1)),
                }],
            ))
            .unwrap_err();
        assert_eq!(oversized.code, GraphSessionErrorCode::GraphLimit);
        assert_eq!(session.snapshot(), before);
        assert!(session.graph_receipts.is_empty());

        let edits = (0..=MAX_PATCH_EDITS)
            .map(|index| GraphEdit::MoveNode {
                node: 2,
                pos: [index as f32, 0.0],
            })
            .collect();
        let oversized = session
            .mutate(mutation(&session, "agent", "oversized-patch", edits))
            .unwrap_err();
        assert_eq!(oversized.code, GraphSessionErrorCode::GraphLimit);
        assert_eq!(session.snapshot(), before);
        assert!(session.graph_receipts.is_empty());
    }

    #[test]
    fn interactive_edits_do_not_consume_agent_retry_capacity() {
        let mut session = new_session();
        for index in 0..100 {
            let result = session
                .mutate_interactive(GraphInteractiveMutationRequest {
                    guard: guard(&session),
                    mutation: GraphMutation::Patch {
                        edits: vec![GraphEdit::MoveNode {
                            node: 2,
                            pos: [index as f32, 32.0],
                        }],
                    },
                })
                .unwrap();
            assert!(!result.semantic_changed);
        }
        assert!(session.graph_receipts.is_empty());
        assert_eq!(session.snapshot().semantic_revision, 0);
        assert_eq!(session.snapshot().revision, 100);
    }

    #[test]
    fn graph_history_is_separate_and_preserves_semantic_rules() {
        let mut session = new_session();
        let initial = session.snapshot();
        session
            .mutate(mutation(
                &session,
                "human",
                "change-code",
                vec![GraphEdit::SetValue {
                    node: 3,
                    field: "code".into(),
                    value: json!("print('changed')"),
                }],
            ))
            .unwrap();
        let changed = session.snapshot();
        let undo = session
            .mutate(GraphMutationRequest {
                guard: guard(&session),
                actor: "human".into(),
                operation_key: "undo-code".into(),
                mutation: GraphMutation::Undo,
            })
            .unwrap();
        assert_eq!(undo.receipt.snapshot.graph, initial.graph);
        assert!(undo.receipt.semantic_changed);
        assert!(undo.receipt.snapshot.semantic_revision > changed.semantic_revision);
    }

    #[test]
    fn supported_run_uses_one_base_for_both_branches_and_never_applies() {
        let mut session = new_session();
        let source_before = session.snapshot().graph.node(1).unwrap().clone();
        let response = session
            .start_run(run_request(&session, "agent", "compare"))
            .unwrap();
        let work = session.claim_run(response.receipt.handle).unwrap();
        assert_eq!(
            work.graph
                .link_into(work.plan.python_node, "font")
                .unwrap()
                .from(),
            work.plan.source_node
        );
        assert_eq!(
            work.graph
                .link_into(work.plan.unchanged_proof, "font")
                .unwrap()
                .from(),
            work.plan.source_node
        );
        assert!(
            !work
                .graph
                .nodes
                .iter()
                .any(|node| { matches!(node.type_name.as_str(), "live.apply" | "core.install") })
        );
        assert_eq!(session.snapshot().graph.node(1).unwrap(), &source_before);
    }

    #[test]
    fn automatic_apply_and_mismatched_scope_are_rejected() {
        let mut session = new_session();
        let mut unsafe_request = run_request(&session, "agent", "unsafe-run");
        let apply = Node {
            id: 5,
            type_name: "live.apply".into(),
            pos: [900.0, 400.0],
            values: BTreeMap::new(),
        };
        session
            .mutate(mutation(
                &session,
                "human",
                "add-apply",
                vec![
                    GraphEdit::AddNode { node: apply },
                    GraphEdit::Connect {
                        link: Link(3, "font".into(), 5, "font".into()),
                    },
                ],
            ))
            .unwrap();
        unsafe_request.guard = semantic_guard(&session);
        let error = session.start_run(unsafe_request).unwrap_err();
        assert_eq!(error.code, GraphSessionErrorCode::UnsupportedTopology);

        let mut session = new_session();
        let mut request = run_request(&session, "agent", "wrong-source");
        request.capture.font.source = 4;
        let error = session.start_run(request).unwrap_err();
        assert_eq!(error.code, GraphSessionErrorCode::CaptureMismatch);
    }

    #[test]
    fn late_semantic_and_replacement_document_results_are_not_published() {
        let mut session = new_session();
        let start = session
            .start_run(run_request(&session, "agent", "late-semantic"))
            .unwrap();
        let work = session.claim_run(start.receipt.handle).unwrap();
        session
            .mutate(mutation(
                &session,
                "human",
                "new-code",
                vec![GraphEdit::SetValue {
                    node: 3,
                    field: "code".into(),
                    value: json!("print('new')"),
                }],
            ))
            .unwrap();
        let stale = session
            .complete_run(
                GraphRunCompletion {
                    handle: work.handle,
                    identity: work.identity.clone(),
                    outcome: GraphRunOutcome::Completed(outputs(&work.identity)),
                },
                &GraphDocumentState {
                    document_epoch: "document-1".into(),
                    document_revision: 12,
                },
            )
            .unwrap();
        assert_eq!(stale.status, GraphRunStatus::Stale);
        assert!(stale.outputs.is_empty());

        let mut session = new_session();
        let start = session
            .start_run(run_request(&session, "agent", "late-document"))
            .unwrap();
        let work = session.claim_run(start.receipt.handle).unwrap();
        let stale = session
            .complete_run(
                GraphRunCompletion {
                    handle: work.handle,
                    identity: work.identity.clone(),
                    outcome: GraphRunOutcome::Completed(outputs(&work.identity)),
                },
                &GraphDocumentState {
                    document_epoch: "replacement-document".into(),
                    document_revision: 12,
                },
            )
            .unwrap();
        assert_eq!(stale.status, GraphRunStatus::Stale);
        assert!(stale.outputs.is_empty());
    }

    #[test]
    fn edit_then_undo_cannot_publish_an_aba_completion() {
        let mut session = new_session();
        let start = session
            .start_run(run_request(&session, "agent", "aba-run"))
            .unwrap();
        let work = session.claim_run(start.receipt.handle).unwrap();
        session
            .mutate(mutation(
                &session,
                "human",
                "aba-edit",
                vec![GraphEdit::SetValue {
                    node: 3,
                    field: "code".into(),
                    value: json!("print('temporary')"),
                }],
            ))
            .unwrap();
        session
            .mutate(GraphMutationRequest {
                guard: guard(&session),
                actor: "human".into(),
                operation_key: "aba-undo".into(),
                mutation: GraphMutation::Undo,
            })
            .unwrap();
        assert_eq!(session.semantic_hash, work.identity.semantic_hash);
        assert_ne!(session.semantic_revision, work.identity.semantic_revision);

        let stale = session
            .complete_run(
                GraphRunCompletion {
                    handle: work.handle,
                    identity: work.identity.clone(),
                    outcome: GraphRunOutcome::Completed(outputs(&work.identity)),
                },
                &GraphDocumentState {
                    document_epoch: "document-1".into(),
                    document_revision: 12,
                },
            )
            .unwrap();
        assert_eq!(stale.status, GraphRunStatus::Stale);
        assert!(stale.outputs.is_empty());
    }

    #[test]
    fn invalid_proof_lineage_terminalizes_the_run() {
        let mut session = new_session();
        let start = session
            .start_run(run_request(&session, "agent", "bad-lineage"))
            .unwrap();
        let work = session.claim_run(start.receipt.handle).unwrap();
        let mut malformed = outputs(&work.identity);
        let derived_hash = match &malformed[2].value {
            GraphNodeOutputValue::Proof {
                canonical_input_sha256,
                ..
            } => canonical_input_sha256.clone(),
            _ => unreachable!(),
        };
        if let GraphNodeOutputValue::Proof {
            canonical_input_sha256,
            ..
        } = &mut malformed[1].value
        {
            *canonical_input_sha256 = derived_hash;
        }
        let failed = session
            .complete_run(
                GraphRunCompletion {
                    handle: work.handle,
                    identity: work.identity,
                    outcome: GraphRunOutcome::Completed(malformed),
                },
                &GraphDocumentState {
                    document_epoch: "document-1".into(),
                    document_revision: 12,
                },
            )
            .unwrap();
        assert_eq!(failed.status, GraphRunStatus::Failed);
        assert!(failed.outputs.is_empty());
        assert_eq!(failed.errors[0].code, "invalid_worker_output");
    }

    #[test]
    fn source_only_scope_cannot_claim_a_completed_comparison() {
        let mut session = new_session();
        let start = session
            .start_run(run_request(&session, "agent", "source-only"))
            .unwrap();
        let work = session.claim_run(start.receipt.handle).unwrap();
        let mut malformed = outputs(&work.identity);
        if let GraphNodeOutputValue::Proof { scope, .. } = &mut malformed[1].value {
            *scope = GraphProofScope::SourceOnly;
        }
        let failed = session
            .complete_run(
                GraphRunCompletion {
                    handle: work.handle,
                    identity: work.identity,
                    outcome: GraphRunOutcome::Completed(malformed),
                },
                &GraphDocumentState {
                    document_epoch: "document-1".into(),
                    document_revision: 12,
                },
            )
            .unwrap();
        assert_eq!(failed.status, GraphRunStatus::Failed);
        assert_eq!(failed.errors[0].code, "invalid_worker_output");
    }

    #[test]
    fn malformed_failure_terminalizes_with_bounded_error() {
        let mut session = new_session();
        let start = session
            .start_run(run_request(&session, "agent", "bad-failure"))
            .unwrap();
        let work = session.claim_run(start.receipt.handle).unwrap();
        let failed = session
            .complete_run(
                GraphRunCompletion {
                    handle: work.handle,
                    identity: work.identity,
                    outcome: GraphRunOutcome::Failed(Vec::new()),
                },
                &GraphDocumentState {
                    document_epoch: "document-1".into(),
                    document_revision: 12,
                },
            )
            .unwrap();
        assert_eq!(failed.status, GraphRunStatus::Failed);
        assert_eq!(failed.errors.len(), 1);
        assert_eq!(failed.errors[0].code, "invalid_worker_output");
        assert!(failed.errors[0].message.len() <= MAX_ERROR_BYTES);
    }

    #[test]
    fn python_report_can_share_a_node_with_the_derived_version() {
        let mut session = new_session();
        let start = session
            .start_run(run_request(&session, "agent", "report-and-version"))
            .unwrap();
        let work = session.claim_run(start.receipt.handle).unwrap();
        let mut completed = outputs(&work.identity);
        completed.push(GraphNodeOutput {
            node: work.plan.python_node,
            value: GraphNodeOutputValue::Report {
                text: "bounded report".into(),
            },
        });
        let inspection = session
            .complete_run(
                GraphRunCompletion {
                    handle: work.handle,
                    identity: work.identity,
                    outcome: GraphRunOutcome::Completed(completed),
                },
                &GraphDocumentState {
                    document_epoch: "document-1".into(),
                    document_revision: 12,
                },
            )
            .unwrap();
        assert_eq!(inspection.status, GraphRunStatus::Completed);
        assert_eq!(inspection.outputs.len(), 4);
    }

    #[test]
    fn cancellation_targets_one_run_and_blocks_late_publication() {
        let mut session = new_session();
        let first = session
            .start_run(run_request(&session, "human", "first"))
            .unwrap();
        let first_work = session.claim_run(first.receipt.handle).unwrap();
        let second = session
            .start_run(run_request(&session, "human", "second"))
            .unwrap();
        let response = session
            .cancel_run(GraphCancelRequest {
                identity: session.snapshot().identity,
                handle: first_work.handle,
                actor: "human".into(),
                operation_key: "cancel-first".into(),
            })
            .unwrap();
        assert_eq!(
            response.receipt.outcome,
            GraphCancelOutcome::CancellationRequested
        );
        assert_eq!(
            session.inspect_run(second.receipt.handle).unwrap().status,
            GraphRunStatus::Queued
        );
        let cancelled = session
            .complete_run(
                GraphRunCompletion {
                    handle: first_work.handle,
                    identity: first_work.identity.clone(),
                    outcome: GraphRunOutcome::Completed(outputs(&first_work.identity)),
                },
                &GraphDocumentState {
                    document_epoch: "document-1".into(),
                    document_revision: 12,
                },
            )
            .unwrap();
        assert_eq!(cancelled.status, GraphRunStatus::Cancelled);
        assert!(cancelled.outputs.is_empty());
    }

    #[test]
    fn retries_return_original_results_and_changed_payloads_reject() {
        let mut session = new_session();
        let request = mutation(
            &session,
            "agent",
            "move-once",
            vec![GraphEdit::MoveNode {
                node: 2,
                pos: [777.0, 32.0],
            }],
        );
        let first = session.mutate(request.clone()).unwrap();
        session
            .mutate(mutation(
                &session,
                "human",
                "move-later",
                vec![GraphEdit::MoveNode {
                    node: 2,
                    pos: [888.0, 32.0],
                }],
            ))
            .unwrap();
        let replay = session.mutate(request.clone()).unwrap();
        assert_eq!(replay.disposition, GraphReceiptDisposition::Replayed);
        assert_eq!(replay.receipt, first.receipt);
        let mut changed = request;
        changed.mutation = GraphMutation::Patch {
            edits: vec![GraphEdit::MoveNode {
                node: 2,
                pos: [999.0, 32.0],
            }],
        };
        assert_eq!(
            session.mutate(changed).unwrap_err().code,
            GraphSessionErrorCode::PayloadMismatch
        );

        let request = run_request(&session, "agent", "run-once");
        let first = session.start_run(request.clone()).unwrap();
        let replay = session.start_run(request.clone()).unwrap();
        assert_eq!(replay.disposition, GraphReceiptDisposition::Replayed);
        assert_eq!(replay.receipt.handle, first.receipt.handle);
        let mut changed = request;
        changed.capture.font.capture_sha256 = sha256(b"another-font");
        assert_eq!(
            session.start_run(changed).unwrap_err().code,
            GraphSessionErrorCode::PayloadMismatch
        );
    }
}
