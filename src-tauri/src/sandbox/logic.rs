//! Logic verification backend: the honest "we cannot execute this safely" mode.
//!
//! It reports `can_execute = false` and `run` returns nothing. The UI turns the
//! Run button into an export/paste/verify flow (DESIGN.md §4.4).

use super::{CaseInput, ExecOutcome, Limits, Sandbox};
use crate::models::SandboxReport;

pub struct LogicSandbox;

impl Sandbox for LogicSandbox {
    fn name(&self) -> &'static str {
        "logic"
    }

    fn detect(&self) -> SandboxReport {
        SandboxReport {
            requested_mode: "logic".to_string(),
            backend: "logic".to_string(),
            can_execute: false,
            python_available: super::python_available(),
            warnings: Vec::new(),
            detail: "Logic verification mode: your code is never executed by this app. \
                     Run it yourself, then paste the outputs back for comparison."
                .to_string(),
        }
    }

    fn can_run(&self, _language: &str) -> bool {
        false
    }

    fn run(&self, _code: &str, _cases: &[CaseInput], _limits: &Limits) -> Vec<ExecOutcome> {
        // Deliberately empty. The judge refuses to call this; if it ever does,
        // an empty result is treated as an internal error, never a pass.
        Vec::new()
    }
}
