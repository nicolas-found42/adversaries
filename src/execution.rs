//! A reviewable execution plan. Building a plan never performs I/O.
use crate::transport::RunConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewSource {
    Composer,
    Repeat,
    Continue,
}

#[derive(Debug, Clone)]
pub struct ExecutionReview {
    pub source: ReviewSource,
    pub config: RunConfig,
    pub message: String,
    pub preflight: Option<crate::preflight::PreflightReport>,
}

impl ExecutionReview {
    pub fn ready(&self) -> bool {
        self.preflight.as_ref().is_some_and(|p| p.ready)
    }

    pub fn total(&self) -> usize {
        self.config.planned_stages()
    }

    pub fn retained(&self) -> usize {
        self.config.completed_turns.len()
    }

    pub fn remaining(&self) -> usize {
        self.total().saturating_sub(self.retained())
    }

    pub fn action_label(&self) -> &'static str {
        if self.config.dry_run {
            "Run practice example"
        } else {
            "Send to providers and start"
        }
    }
}

pub fn same_plan(a: &RunConfig, b: &RunConfig) -> bool {
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// A launch failure belongs to the attempted run, not the retained workspace.
#[derive(Debug, Clone)]
pub struct LaunchFailure {
    pub question: String,
    pub message: String,
}
