//! §112 — LAUNCH SESSION STATE MACHINE.
//!
//! ```text
//! IDLE → PREFLIGHT → RESOLVING → DOWNLOADING → INSTALLING → PAIRING
//!      → SPAWNING → RUNNING → EXITING → ANALYZING → COMPLETED / CRASHED
//! ```
//!
//! Early exits: `PREFLIGHT → CANCELLED` (preflight fail), mọi phase trước `SPAWNING`
//! có thể → `CANCELLED` khi user huỷ. UI progress lấy từ state machine này (§112).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LaunchPhase {
    Idle,
    Preflight,
    Resolving,
    Downloading,
    Installing,
    Pairing,
    Spawning,
    Running,
    Exiting,
    Analyzing,
    Completed,
    Crashed,
    Cancelled,
}

impl LaunchPhase {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            LaunchPhase::Completed | LaunchPhase::Crashed | LaunchPhase::Cancelled
        )
    }

    fn can_transition(self, next: LaunchPhase) -> bool {
        use LaunchPhase::*;
        if self.is_terminal() {
            return false;
        }
        match (self, next) {
            // Cancel được phép từ mọi phase trước Spawning (chưa có process con).
            (Idle, Cancelled)
            | (Preflight, Cancelled)
            | (Resolving, Cancelled)
            | (Downloading, Cancelled)
            | (Installing, Cancelled)
            | (Pairing, Cancelled) => true,
            (Idle, Preflight)
            | (Preflight, Resolving)
            | (Resolving, Downloading)
            | (Downloading, Installing)
            | (Installing, Pairing)
            | (Pairing, Spawning)
            | (Spawning, Running)
            | (Running, Exiting)
            | (Exiting, Analyzing)
            // Crash detect trực tiếp khi exit code != 0 (không qua Analyzing).
            | (Exiting, Crashed)
            | (Analyzing, Completed)
            | (Analyzing, Crashed) => true,
            // Retry trong phase (progress tick) — không đổi phase.
            (Downloading, Downloading) | (Resolving, Resolving) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionTransitionError {
    #[error("invalid launch transition {from:?} -> {to:?}")]
    Invalid { from: LaunchPhase, to: LaunchPhase },
}

impl SessionTransitionError {
    pub fn code(&self) -> &'static str {
        "LAUNCH_INVALID_TRANSITION"
    }
}

/// §112 — Một launch session. Non-clone để đảm bảo single-owner.
pub struct LaunchSession {
    instance_id: String,
    phase: LaunchPhase,
}

impl LaunchSession {
    pub fn new(instance_id: impl Into<String>) -> Self {
        Self {
            instance_id: instance_id.into(),
            phase: LaunchPhase::Idle,
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn phase(&self) -> LaunchPhase {
        self.phase
    }

    pub fn advance(&mut self, next: LaunchPhase) -> Result<LaunchPhase, SessionTransitionError> {
        if !self.phase.can_transition(next) {
            return Err(SessionTransitionError::Invalid {
                from: self.phase,
                to: next,
            });
        }
        self.phase = next;
        Ok(self.phase)
    }
}
