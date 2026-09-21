// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded cancellation state for receipt-backed live agent operations.
//!
//! The registry is transport-independent and owns no font data.
//! A native endpoint admits an exact document-epoch, actor and operation-key identity before
//! queueing an apply request.
//! The application thread then claims the commit boundary after staging and records the terminal
//! result after publication.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::agent_session::{
    AgentOperationKey, AgentPayloadDigest, AgentSessionError, AgentSessionMetadata,
};

/// Largest supported cancellation registry for one live document epoch.
pub const MAX_AGENT_CANCELLATIONS: usize = 4096;

/// Exact identity of one receipt-backed operation in one open document lifetime.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AgentCancellationIdentity {
    document_epoch: String,
    actor: String,
    operation_key: AgentOperationKey,
}

impl AgentCancellationIdentity {
    /// Validate and retain a transport-supplied cancellation identity.
    pub fn new(
        document_epoch: impl Into<String>,
        actor: impl Into<String>,
        operation_key: impl Into<String>,
    ) -> Result<Self, AgentSessionError> {
        let metadata = AgentSessionMetadata::new(document_epoch, actor)?;
        Ok(Self {
            document_epoch: metadata.document_epoch().to_owned(),
            actor: metadata.actor().to_owned(),
            operation_key: AgentOperationKey::new(operation_key)?,
        })
    }

    /// Exact open-document epoch.
    pub fn document_epoch(&self) -> &str {
        &self.document_epoch
    }

    /// Actor-local receipt namespace.
    pub fn actor(&self) -> &str {
        &self.actor
    }

    /// Actor-local operation key.
    pub fn operation_key(&self) -> &AgentOperationKey {
        &self.operation_key
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CancellationState {
    Pending,
    Prevented,
    Committing,
    Committed,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CancellationEntry {
    payload_digest: AgentPayloadDigest,
    state: CancellationState,
}

/// Whether an apply identity was first admitted now or was already known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCancellationAdmission {
    /// A new bounded registry entry was inserted.
    New,
    /// The exact identity already has pending or terminal state.
    Existing,
}

/// Result of asking to prevent an admitted operation from committing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCancellationOutcome {
    /// Cancellation won before the application claimed its commit boundary.
    Prevented,
    /// This exact operation was already prevented.
    AlreadyPrevented,
    /// The application claimed the commit boundary, but has not reported a terminal result yet.
    TooLate,
    /// The canonical transaction already committed.
    Committed,
    /// The operation completed without committing, for example unchanged or rejected.
    Completed,
    /// No apply with this exact epoch, actor and operation key was admitted.
    Unknown,
}

/// Result of the single claim immediately before canonical publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCommitClaim {
    /// Publication may proceed.
    Claimed,
    /// Cancellation already prevented publication.
    Prevented,
    /// The identity was not admitted by this endpoint.
    Unknown,
}

/// Terminal state published by the application after an apply attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCancellationTerminal {
    /// The transaction committed.
    Committed,
    /// The operation finished without a commit.
    Completed,
    /// Cancellation prevented the commit.
    Prevented,
}

/// Errors that prevent admission to the bounded registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentCancellationError {
    /// The supplied identity does not belong to this document epoch.
    StaleDocument,
    /// The non-evicting registry has no room for another identity.
    CapacityExhausted {
        /// Configured operation capacity.
        capacity: usize,
    },
    /// This exact operation identity was already reserved for another payload.
    PayloadMismatch,
    /// The configured capacity is outside the supported bound.
    InvalidCapacity {
        /// Requested capacity.
        requested: usize,
        /// Largest supported capacity.
        maximum: usize,
    },
}

impl std::fmt::Display for AgentCancellationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleDocument => write!(
                formatter,
                "cancellation identity has a stale document epoch"
            ),
            Self::CapacityExhausted { capacity } => {
                write!(formatter, "cancellation capacity {capacity} is exhausted")
            }
            Self::PayloadMismatch => write!(
                formatter,
                "operation identity is already reserved for another payload"
            ),
            Self::InvalidCapacity { requested, maximum } => write!(
                formatter,
                "cancellation capacity {requested} must be in 1..={maximum}"
            ),
        }
    }
}

impl std::error::Error for AgentCancellationError {}

#[derive(Debug)]
struct RegistryInner {
    document_epoch: String,
    capacity: usize,
    states: Mutex<BTreeMap<(String, AgentOperationKey), CancellationEntry>>,
}

/// Shared, bounded and non-evicting cancellation state for one document epoch.
#[derive(Clone, Debug)]
pub struct AgentCancellationRegistry {
    inner: Arc<RegistryInner>,
}

impl AgentCancellationRegistry {
    /// Create an empty registry bound to one exact endpoint epoch.
    pub fn new(
        document_epoch: impl Into<String>,
        capacity: usize,
    ) -> Result<Self, AgentCancellationError> {
        if !(1..=MAX_AGENT_CANCELLATIONS).contains(&capacity) {
            return Err(AgentCancellationError::InvalidCapacity {
                requested: capacity,
                maximum: MAX_AGENT_CANCELLATIONS,
            });
        }
        Ok(Self {
            inner: Arc::new(RegistryInner {
                document_epoch: document_epoch.into(),
                capacity,
                states: Mutex::new(BTreeMap::new()),
            }),
        })
    }

    /// Exact document epoch owned by this registry.
    pub fn document_epoch(&self) -> &str {
        &self.inner.document_epoch
    }

    /// Admit one apply before it enters the bounded application mailbox.
    pub fn admit(
        &self,
        identity: &AgentCancellationIdentity,
        payload_digest: AgentPayloadDigest,
    ) -> Result<AgentCancellationAdmission, AgentCancellationError> {
        self.check_epoch(identity)?;
        let mut states = self
            .inner
            .states
            .lock()
            .expect("cancellation mutex poisoned");
        let key = (identity.actor.clone(), identity.operation_key.clone());
        if let Some(entry) = states.get(&key) {
            if entry.payload_digest != payload_digest {
                return Err(AgentCancellationError::PayloadMismatch);
            }
            return Ok(AgentCancellationAdmission::Existing);
        }
        if states.len() == self.inner.capacity {
            return Err(AgentCancellationError::CapacityExhausted {
                capacity: self.inner.capacity,
            });
        }
        states.insert(
            key,
            CancellationEntry {
                payload_digest,
                state: CancellationState::Pending,
            },
        );
        Ok(AgentCancellationAdmission::New)
    }

    /// Release a newly admitted request with the same payload that could not enter the mailbox.
    ///
    /// A concurrent cancellation retains its prevented state so an exact retry records the
    /// terminal cancellation receipt instead of executing accidentally.
    pub fn release_unqueued(
        &self,
        identity: &AgentCancellationIdentity,
        payload_digest: AgentPayloadDigest,
    ) -> Result<(), AgentCancellationError> {
        self.check_epoch(identity)?;
        let mut states = self
            .inner
            .states
            .lock()
            .expect("cancellation mutex poisoned");
        let key = (identity.actor.clone(), identity.operation_key.clone());
        if let Some(entry) = states.get(&key) {
            if entry.payload_digest != payload_digest {
                return Err(AgentCancellationError::PayloadMismatch);
            }
            if entry.state == CancellationState::Pending {
                states.remove(&key);
            }
        }
        Ok(())
    }

    /// Attempt to prevent an admitted operation from reaching its commit boundary.
    pub fn cancel(
        &self,
        identity: &AgentCancellationIdentity,
    ) -> Result<AgentCancellationOutcome, AgentCancellationError> {
        self.check_epoch(identity)?;
        let mut states = self
            .inner
            .states
            .lock()
            .expect("cancellation mutex poisoned");
        let Some(entry) = states.get_mut(&(identity.actor.clone(), identity.operation_key.clone()))
        else {
            return Ok(AgentCancellationOutcome::Unknown);
        };
        Ok(match &mut entry.state {
            state @ CancellationState::Pending => {
                *state = CancellationState::Prevented;
                AgentCancellationOutcome::Prevented
            }
            CancellationState::Prevented => AgentCancellationOutcome::AlreadyPrevented,
            CancellationState::Committing => AgentCancellationOutcome::TooLate,
            CancellationState::Committed => AgentCancellationOutcome::Committed,
            CancellationState::Completed => AgentCancellationOutcome::Completed,
        })
    }

    /// Atomically claim the point immediately before canonical publication.
    pub fn claim_commit(
        &self,
        identity: &AgentCancellationIdentity,
    ) -> Result<AgentCommitClaim, AgentCancellationError> {
        self.check_epoch(identity)?;
        let mut states = self
            .inner
            .states
            .lock()
            .expect("cancellation mutex poisoned");
        let Some(entry) = states.get_mut(&(identity.actor.clone(), identity.operation_key.clone()))
        else {
            return Ok(AgentCommitClaim::Unknown);
        };
        Ok(match &mut entry.state {
            state @ CancellationState::Pending => {
                *state = CancellationState::Committing;
                AgentCommitClaim::Claimed
            }
            CancellationState::Prevented => AgentCommitClaim::Prevented,
            CancellationState::Committing
            | CancellationState::Committed
            | CancellationState::Completed => AgentCommitClaim::Unknown,
        })
    }

    /// Record the terminal result after the application finishes the exact apply attempt.
    pub fn finish(
        &self,
        identity: &AgentCancellationIdentity,
        terminal: AgentCancellationTerminal,
    ) -> Result<(), AgentCancellationError> {
        self.check_epoch(identity)?;
        let mut states = self
            .inner
            .states
            .lock()
            .expect("cancellation mutex poisoned");
        let Some(entry) = states.get_mut(&(identity.actor.clone(), identity.operation_key.clone()))
        else {
            return Ok(());
        };
        entry.state = match (entry.state, terminal) {
            (CancellationState::Committed, _) => CancellationState::Committed,
            (CancellationState::Prevented, _) => CancellationState::Prevented,
            (CancellationState::Completed, _) => CancellationState::Completed,
            (_, AgentCancellationTerminal::Committed) => CancellationState::Committed,
            (_, AgentCancellationTerminal::Completed) => CancellationState::Completed,
            (_, AgentCancellationTerminal::Prevented) => CancellationState::Prevented,
        };
        Ok(())
    }

    fn check_epoch(
        &self,
        identity: &AgentCancellationIdentity,
    ) -> Result<(), AgentCancellationError> {
        if identity.document_epoch == self.inner.document_epoch {
            Ok(())
        } else {
            Err(AgentCancellationError::StaleDocument)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(epoch: &str, actor: &str, key: &str) -> AgentCancellationIdentity {
        AgentCancellationIdentity::new(epoch, actor, key).unwrap()
    }

    fn digest(value: &str) -> AgentPayloadDigest {
        AgentPayloadDigest::sha256(value.as_bytes())
    }

    #[test]
    fn exact_identity_isolated_across_actor_and_epoch() {
        let registry = AgentCancellationRegistry::new("epoch", 4).unwrap();
        let first = identity("epoch", "first", "same-key");
        let second = identity("epoch", "second", "same-key");
        registry.admit(&first, digest("first")).unwrap();
        registry.admit(&second, digest("second")).unwrap();
        assert_eq!(
            registry.cancel(&first).unwrap(),
            AgentCancellationOutcome::Prevented
        );
        assert_eq!(
            registry.claim_commit(&first).unwrap(),
            AgentCommitClaim::Prevented
        );
        assert_eq!(
            registry.claim_commit(&second).unwrap(),
            AgentCommitClaim::Claimed
        );
        let stale = identity("other", "first", "same-key");
        assert_eq!(
            registry.cancel(&stale),
            Err(AgentCancellationError::StaleDocument)
        );
    }

    #[test]
    fn commit_claim_and_terminal_result_are_distinct() {
        let registry = AgentCancellationRegistry::new("epoch", 2).unwrap();
        let operation = identity("epoch", "actor", "operation");
        registry.admit(&operation, digest("operation")).unwrap();
        assert_eq!(
            registry.claim_commit(&operation).unwrap(),
            AgentCommitClaim::Claimed
        );
        assert_eq!(
            registry.cancel(&operation).unwrap(),
            AgentCancellationOutcome::TooLate
        );
        registry
            .finish(&operation, AgentCancellationTerminal::Committed)
            .unwrap();
        assert_eq!(
            registry.cancel(&operation).unwrap(),
            AgentCancellationOutcome::Committed
        );
    }

    #[test]
    fn concurrent_cancel_and_commit_claim_have_one_winner() {
        for index in 0..128 {
            let registry = AgentCancellationRegistry::new("epoch", 1).unwrap();
            let operation = identity("epoch", "actor", &format!("race-{index}"));
            registry.admit(&operation, digest("race")).unwrap();
            let barrier = Arc::new(std::sync::Barrier::new(3));
            let cancel_registry = registry.clone();
            let cancel_operation = operation.clone();
            let cancel_barrier = barrier.clone();
            let cancel = std::thread::spawn(move || {
                cancel_barrier.wait();
                cancel_registry.cancel(&cancel_operation).unwrap()
            });
            let claim_registry = registry.clone();
            let claim_operation = operation.clone();
            let claim_barrier = barrier.clone();
            let claim = std::thread::spawn(move || {
                claim_barrier.wait();
                claim_registry.claim_commit(&claim_operation).unwrap()
            });
            barrier.wait();
            match (cancel.join().unwrap(), claim.join().unwrap()) {
                (AgentCancellationOutcome::Prevented, AgentCommitClaim::Prevented)
                | (AgentCancellationOutcome::TooLate, AgentCommitClaim::Claimed) => {}
                outcomes => panic!("inconsistent cancellation race: {outcomes:?}"),
            }
        }
    }

    #[test]
    fn capacity_never_evicts_terminal_identity() {
        let registry = AgentCancellationRegistry::new("epoch", 1).unwrap();
        let first = identity("epoch", "actor", "first");
        registry.admit(&first, digest("first")).unwrap();
        registry
            .finish(&first, AgentCancellationTerminal::Completed)
            .unwrap();
        assert_eq!(
            registry.admit(&identity("epoch", "actor", "second"), digest("second")),
            Err(AgentCancellationError::CapacityExhausted { capacity: 1 })
        );
        assert_eq!(
            registry.cancel(&first).unwrap(),
            AgentCancellationOutcome::Completed
        );
    }

    #[test]
    fn payload_conflict_and_late_failure_cannot_rewrite_committed_state() {
        let registry = AgentCancellationRegistry::new("epoch", 1).unwrap();
        let operation = identity("epoch", "actor", "operation");
        registry.admit(&operation, digest("original")).unwrap();
        assert_eq!(
            registry.admit(&operation, digest("different")),
            Err(AgentCancellationError::PayloadMismatch)
        );
        assert_eq!(
            registry.release_unqueued(&operation, digest("different")),
            Err(AgentCancellationError::PayloadMismatch)
        );
        assert_eq!(
            registry.claim_commit(&operation).unwrap(),
            AgentCommitClaim::Claimed
        );
        registry
            .finish(&operation, AgentCancellationTerminal::Committed)
            .unwrap();
        registry
            .finish(&operation, AgentCancellationTerminal::Completed)
            .unwrap();
        assert_eq!(
            registry.cancel(&operation).unwrap(),
            AgentCancellationOutcome::Committed
        );
    }

    #[test]
    fn release_keeps_concurrent_prevention_but_removes_plain_pending() {
        let registry = AgentCancellationRegistry::new("epoch", 2).unwrap();
        let pending = identity("epoch", "actor", "pending");
        registry.admit(&pending, digest("pending")).unwrap();
        registry
            .release_unqueued(&pending, digest("pending"))
            .unwrap();
        assert_eq!(
            registry.cancel(&pending).unwrap(),
            AgentCancellationOutcome::Unknown
        );

        let prevented = identity("epoch", "actor", "prevented");
        registry.admit(&prevented, digest("prevented")).unwrap();
        registry.cancel(&prevented).unwrap();
        registry
            .release_unqueued(&prevented, digest("prevented"))
            .unwrap();
        registry
            .finish(&prevented, AgentCancellationTerminal::Completed)
            .unwrap();
        assert_eq!(
            registry.cancel(&prevented).unwrap(),
            AgentCancellationOutcome::AlreadyPrevented
        );
    }
}
