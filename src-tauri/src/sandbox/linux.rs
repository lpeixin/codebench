//! Linux backend: `nsjail`.
//!
//! Chosen because it needs no daemon and no Docker: it is a single binary that
//! uses namespaces + rlimits. `detect()` looks for the binary and then dry-runs
//! it, because namespaces are unavailable in some containers even when the
//! binary is installed.
//!
//! NOTE: this backend is only exercised on Linux CI (the development machine
//! for this repo is macOS). The flag list follows `nsjail --help`; if a future
//! nsjail release renames an option the dry probe fails loudly and the app
//! falls back to logic verification rather than running unsandboxed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::proc;
use super::{CaseInput, ExecOutcome, Limits, Sandbox};
use crate::models::SandboxReport;

pub const BACKEND: &str = "linux-nsjail";

pub struct LinuxSandbox;

impl Sandbox for LinuxSandbox {
    fn name(&self) -> &'static str {
        BACKEND
    }

    fn detect(&self) -> SandboxReport {
        let mut warnings = vec![
            "nsjail provides namespace isolation with rlimits; it is not a hardened VM.".to_string(),
        ];
        let Some(nsjail) = super::find_in_path("nsjail") else {
            return SandboxReport {
                requested_mode: "auto".into(),
                backend: BACKEND.into(),
                can_execute: false,
                python_available: super::python_available(),
                warnings,
                detail: "nsjail was not found on PATH. Install it (e.g. `apt install nsjail`) \
                         or use logic verification mode."
                    .into(),
            };
        };
        let Some(python) = super::python_probe() else {
            return SandboxReport {
                requested_mode: "auto".into(),
                backend: BACKEND.into(),
                can_execute: false,
                python_available: false,
                warnings,
                detail: "python3 was not found on PATH, so nothing can be executed.".into(),
            };
        };

        match dry_probe(&nsjail) {
            Ok(()) => SandboxReport {
                requested_mode: "auto".into(),
                backend: BACKEND.into(),
                can_execute: true,
                python_available: true,
                warnings,
                detail: format!("nsjail OK; python interpreter: {}", python.display()),
            },
            Err(e) => {
                warnings.push(
                    "nsjail could not start a jailed process here (missing user namespaces or \
                     insufficient privileges), so local judging is disabled."
                        .to_string(),
                );
                SandboxReport {
                    requested_mode: "auto".into(),
                    backend: BACKEND.into(),
                    can_execute: false,
                    python_available: true,
                    warnings,
                    detail: format!("nsjail dry run failed: {e}"),
                }
            }
        }
    }

    fn run(&self, code: &str, cases: &[CaseInput], limits: &Limits) -> Vec<ExecOutcome> {
        let Some(nsjail) = super::find_in_path("nsjail") else {
            return cases
                .iter()
                .map(|c| ExecOutcome::internal(c.idx, "nsjail disappeared", 0))
                .collect();
        };
        let Some(python) = super::python_probe() else {
            return cases
                .iter()
                .map(|c| ExecOutcome::internal(c.idx, "python3 disappeared", 0))
                .collect();
        };

        let workdir = match make_workdir() {
            Ok(d) => d,
            Err(e) => {
                return cases
                    .iter()
                    .map(|c| ExecOutcome::internal(c.idx, format!("temp dir: {e}"), 0))
                    .collect()
            }
        };

        let result = (|| -> Result<Vec<ExecOutcome>, String> {
            std::fs::write(workdir.join("solution.py"), code)
                .map_err(|e| format!("write solution.py: {e}"))?;
            let mut out = Vec::with_capacity(cases.len());
            for case in cases {
                let mut cmd = Command::new(&nsjail);
                cmd.args(nsjail_args(&python, &workdir, limits));
                let proc_out = proc::run(cmd, case.input.as_bytes(), limits.timeout());
                out.push(ExecOutcome::from_proc(case.idx, proc_out));
            }
            Ok(out)
        })();

        let _ = std::fs::remove_dir_all(&workdir);
        match result {
            Ok(v) => v,
            Err(e) => cases
                .iter()
                .map(|c| ExecOutcome::internal(c.idx, e.clone(), 0))
                .collect(),
        }
    }
}

/// Argument list for one jailed run.
///
/// Split out from `run` so it can be asserted in a unit test on any platform.
pub fn nsjail_args(python: &Path, workdir: &Path, limits: &Limits) -> Vec<String> {
    let secs = limits.time_limit_ms.div_ceil(1000).max(1);
    let dir = workdir.display().to_string();
    vec![
        // once, quiet
        "-Mo".into(),
        "--quiet".into(),
        // working directory inside the jail
        "--cwd".into(),
        dir.clone(),
        // read-only root, then the workdir mounted read-write on top
        "-R".into(),
        "/".into(),
        "-B".into(),
        dir,
        // writable scratch space for the interpreter
        "-T".into(),
        "/tmp".into(),
        // limits
        "--time_limit".into(),
        secs.to_string(),
        "--rlimit_cpu".into(),
        secs.to_string(),
        "--rlimit_as".into(),
        limits.mem_limit_mb.to_string(),
        "--rlimit_fsize".into(),
        "8".into(),
        "--rlimit_nproc".into(),
        "64".into(),
        // drop privileges
        "--user".into(),
        "65534".into(),
        "--group".into(),
        "65534".into(),
        "--".into(),
        python.display().to_string(),
        "solution.py".into(),
    ]
}

fn dry_probe(nsjail: &Path) -> Result<(), String> {
    let mut cmd = Command::new(nsjail);
    cmd.args(["-Mo", "--quiet", "-R", "/", "--", "/bin/true"]);
    let out = proc::run(cmd, b"", Duration::from_secs(5));
    if let Some(e) = out.spawn_error {
        return Err(e);
    }
    if out.timed_out {
        return Err("dry run timed out".into());
    }
    if out.exit_code != Some(0) {
        let detail = proc::stderr_excerpt(&out.stderr, 400).unwrap_or_default();
        return Err(format!("exit {:?}: {}", out.exit_code, detail));
    }
    Ok(())
}

fn make_workdir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("local-leet-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::canonicalize(&dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_contain_isolation_and_limits() {
        let args = nsjail_args(
            Path::new("/usr/bin/python3"),
            Path::new("/tmp/local-leet-1"),
            &Limits {
                time_limit_ms: 3000,
                mem_limit_mb: 256,
            },
        );
        let joined = args.join(" ");
        assert!(joined.contains("--cwd /tmp/local-leet-1"));
        assert!(joined.contains("-R /"));
        assert!(joined.contains("-B /tmp/local-leet-1"));
        assert!(joined.contains("--time_limit 3"));
        assert!(joined.contains("--rlimit_as 256"));
        assert!(joined.contains("--user 65534"));
        // The program must come after the `--` separator, last.
        let sep = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[sep + 1], "/usr/bin/python3");
        assert_eq!(args[sep + 2], "solution.py");
    }

    #[test]
    fn sub_second_limits_round_up() {
        let args = nsjail_args(
            Path::new("/usr/bin/python3"),
            Path::new("/tmp/x"),
            &Limits {
                time_limit_ms: 200,
                mem_limit_mb: 64,
            },
        );
        let i = args.iter().position(|a| a == "--time_limit").unwrap();
        assert_eq!(args[i + 1], "1");
    }
}
