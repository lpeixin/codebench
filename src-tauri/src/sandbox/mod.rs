//! Sandbox abstraction and backend resolution.
//!
//! Hard invariant (see DESIGN.md §5): user code is only ever executed by a
//! backend whose `detect()` reported `can_execute == true`. There is no path
//! from a Tauri command to `std::process` for user code except through a
//! `Sandbox` implementation in this module.

pub mod linux;
pub mod logic;
pub mod macos;
pub mod mock;
pub mod proc;

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::models::{SandboxMode, SandboxReport};

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub time_limit_ms: u64,
    pub mem_limit_mb: u64,
}

impl Limits {
    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.time_limit_ms.max(100))
    }
}

#[derive(Debug, Clone)]
pub struct CaseInput {
    pub idx: usize,
    pub input: String,
    pub expected: String,
}

/// What a backend reports for one case. Comparison and classification are the
/// judge's job, not the sandbox's.
#[derive(Debug, Clone)]
pub struct ExecOutcome {
    pub idx: usize,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub time_ms: u64,
    /// Infrastructure failure: could not spawn, sandbox refused to start, etc.
    pub error: Option<String>,
}

impl ExecOutcome {
    pub fn internal(idx: usize, err: impl Into<String>, time_ms: u64) -> Self {
        Self {
            idx,
            exit_code: None,
            timed_out: false,
            stdout: String::new(),
            stderr: String::new(),
            time_ms,
            error: Some(err.into()),
        }
    }

    pub fn from_proc(idx: usize, out: proc::ProcOutput) -> Self {
        Self {
            idx,
            exit_code: out.exit_code,
            timed_out: out.timed_out,
            stdout: out.stdout,
            stderr: out.stderr,
            time_ms: out.time_ms,
            error: out.spawn_error,
        }
    }
}

pub trait Sandbox: Send + Sync {
    /// Stable identifier persisted with runs ("macos-sandbox-exec", "logic"…).
    fn name(&self) -> &'static str;
    /// Always safe to call: probes binaries and dry-runs the sandbox.
    fn detect(&self) -> SandboxReport;
    /// MVP supports Python only; the trait keeps the door open.
    fn can_run(&self, language: &str) -> bool {
        language == "python"
    }
    /// Execute `code` against every case. Must never be called when
    /// `detect().can_execute` is false.
    fn run(&self, code: &str, cases: &[CaseInput], limits: &Limits) -> Vec<ExecOutcome>;
}

pub struct Resolved {
    pub sandbox: Box<dyn Sandbox>,
    pub report: SandboxReport,
}

/// Pick the backend for `mode` and always return the report that explains it.
pub fn resolve(mode: SandboxMode) -> Resolved {
    let mut resolved = match mode {
        SandboxMode::Mock => {
            let backend = mock::MockSandbox;
            let report = backend.detect();
            Resolved {
                sandbox: Box::new(backend),
                report,
            }
        }
        SandboxMode::Logic => {
            let backend = logic::LogicSandbox;
            let report = backend.detect();
            Resolved {
                sandbox: Box::new(backend),
                report,
            }
        }
        SandboxMode::Auto | SandboxMode::Native => {
            let native = native_backend();
            resolve_native_or_logic(mode, native)
        }
    };
    // The backends report a generic "auto"; the report must echo what the user
    // actually asked for, because the UI banner explains the difference.
    resolved.report.requested_mode = mode.as_str().to_string();
    resolved
}

/// Platform backend. Windows has none in the MVP, which is exactly why the
/// fallback exists.
pub fn native_backend() -> Box<dyn Sandbox> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacSandbox)
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxSandbox)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Box::new(logic::LogicSandbox)
    }
}

/// Fallback composition, factored out so it can be tested on any OS.
pub fn resolve_native_or_logic(mode: SandboxMode, native: Box<dyn Sandbox>) -> Resolved {
    let native_report = native.detect();
    if native_report.can_execute {
        return Resolved {
            sandbox: native,
            report: native_report,
        };
    }

    let mut warnings = native_report.warnings.clone();
    let detail = if mode == SandboxMode::Native {
        warnings.push(
            "Native sandbox was requested but is unavailable — code will NOT be executed. \
             Switch to logic verification or fix the sandbox, then try again."
                .to_string(),
        );
        format!(
            "Native sandbox unavailable ({}): {}",
            native_report.backend, native_report.detail
        )
    } else {
        warnings.push(
            "No safe execution sandbox is available, so local judging is disabled. \
             Logic verification mode is active: you run the code yourself and paste outputs."
                .to_string(),
        );
        format!(
            "Automatic fallback to logic verification ({} reported: {})",
            native_report.backend, native_report.detail
        )
    };

    Resolved {
        sandbox: Box::new(logic::LogicSandbox),
        report: SandboxReport {
            requested_mode: mode.as_str().to_string(),
            backend: "logic".to_string(),
            can_execute: false,
            python_available: native_report.python_available,
            warnings,
            detail,
        },
    }
}

// ---------------------------------------------------------------------------
// Host probes
// ---------------------------------------------------------------------------

/// Locate an executable on PATH. Returns an absolute path, since sandbox
/// backends refuse to run relative program names.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<&str> = if cfg!(windows) {
        vec![".exe", ".cmd", ".bat", ""]
    } else {
        vec![""]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Resolve a Python 3 interpreter once per process.
///
/// Windows installs usually expose `python` rather than `python3`, so both are
/// probed; the probe actually executes `--version` so a broken shim is caught
/// here rather than in the middle of a run.
pub fn python_probe() -> Option<PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            for name in ["python3", "python"] {
                let Some(bin) = find_in_path(name) else {
                    continue;
                };
                let mut cmd = std::process::Command::new(&bin);
                cmd.arg("--version");
                let out = proc::run(cmd, b"", std::time::Duration::from_secs(5));
                let text = format!("{}{}", out.stdout, out.stderr);
                if out.success() && text.contains("Python 3") {
                    return Some(bin);
                }
            }
            None
        })
        .clone()
}

pub fn python_available() -> bool {
    python_probe().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSandbox {
        can_execute: bool,
    }

    impl Sandbox for FakeSandbox {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn detect(&self) -> SandboxReport {
            SandboxReport {
                requested_mode: "auto".into(),
                backend: "fake".into(),
                can_execute: self.can_execute,
                python_available: true,
                warnings: vec!["fake warning".into()],
                detail: "fake backend".into(),
            }
        }
        fn run(&self, _code: &str, cases: &[CaseInput], _limits: &Limits) -> Vec<ExecOutcome> {
            cases
                .iter()
                .map(|c| ExecOutcome::internal(c.idx, "fake sandbox must not run", 0))
                .collect()
        }
    }

    #[test]
    fn uses_native_backend_when_it_is_usable() {
        let r = resolve_native_or_logic(SandboxMode::Auto, Box::new(FakeSandbox { can_execute: true }));
        assert_eq!(r.report.backend, "fake");
        assert!(r.report.can_execute);
        assert_eq!(r.sandbox.name(), "fake");
    }

    #[test]
    fn auto_falls_back_to_logic_with_explanation() {
        let r = resolve_native_or_logic(
            SandboxMode::Auto,
            Box::new(FakeSandbox { can_execute: false }),
        );
        assert_eq!(r.report.backend, "logic");
        assert!(!r.report.can_execute);
        assert_eq!(r.sandbox.name(), "logic");
        // The original reason is preserved for the UI banner.
        assert!(r.report.detail.contains("fake"));
        assert!(r.report.warnings.iter().any(|w| w.contains("Logic verification")));
    }

    #[test]
    fn native_mode_fallback_is_loud() {
        let r = resolve_native_or_logic(
            SandboxMode::Native,
            Box::new(FakeSandbox { can_execute: false }),
        );
        assert_eq!(r.report.backend, "logic");
        assert!(r.report.warnings.iter().any(|w| w.contains("will NOT be executed")));
    }

    #[test]
    fn logic_backend_never_reports_execution() {
        let report = logic::LogicSandbox.detect();
        assert!(!report.can_execute);
        assert!(!logic::LogicSandbox.can_run("python"));
        assert!(logic::LogicSandbox.run("print(1)", &[], &Limits {
            time_limit_ms: 1000,
            mem_limit_mb: 64
        })
        .is_empty());
    }

    #[test]
    fn mock_backend_marks_results_as_simulated() {
        let report = mock::MockSandbox.detect();
        assert!(report.can_execute);
        assert!(report.warnings.iter().any(|w| w.contains("MOCK")));
        let cases = vec![CaseInput {
            idx: 0,
            input: "1 2\n".into(),
            expected: "3\n".into(),
        }];
        let out = mock::MockSandbox.run("print(0)", &cases, &Limits {
            time_limit_ms: 1000,
            mem_limit_mb: 64,
        });
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].stdout, "3\n");
        assert!(out[0].stderr.contains("mock"));
    }
}
