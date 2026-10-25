//! Development-only backend that simulates results without running anything.
//!
//! Exists so the whole UI (per-case results, statuses, history) can be exercised
//! on a machine with no sandbox tooling. Every surface that can show a mock run
//! must label it, and `mode` is persisted as `mock` so history stays honest.

use super::{CaseInput, ExecOutcome, Limits, Sandbox};
use crate::models::SandboxReport;

pub struct MockSandbox;

impl Sandbox for MockSandbox {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn detect(&self) -> SandboxReport {
        SandboxReport {
            requested_mode: "mock".to_string(),
            backend: "mock".to_string(),
            can_execute: true,
            python_available: super::python_available(),
            warnings: vec![
                "MOCK — no code is executed. Results are simulated and must not be trusted."
                    .to_string(),
            ],
            detail: "Development runner: every case is reported as passed by construction."
                .to_string(),
        }
    }

    fn can_run(&self, language: &str) -> bool {
        language == "python"
    }

    fn run(&self, _code: &str, cases: &[CaseInput], _limits: &Limits) -> Vec<ExecOutcome> {
        cases
            .iter()
            .map(|c| ExecOutcome {
                idx: c.idx,
                exit_code: Some(0),
                timed_out: false,
                stdout: c.expected.clone(),
                stderr: "mock runner: your code was not executed".to_string(),
                time_ms: 1,
                error: None,
            })
            .collect()
    }
}
