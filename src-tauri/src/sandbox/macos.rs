//! macOS backend: `sandbox-exec` (Seatbelt) profile, best effort.
//!
//! Apple deprecated `sandbox-exec`; it is not a security boundary against a
//! determined attacker. It is still worth using: it denies network access and
//! file writes outside the run's temp directory, and the supervisor enforces
//! the deadline. The UI labels it "best-effort" everywhere it appears.
//!
//! `detect()` does not merely look for the binary — it dry-runs the sandbox,
//! because `sandbox_apply` fails in nested/restricted environments (some CI
//! runners, or a Tauri app launched from inside another sandbox). Without that
//! probe the first real run would be the first failure.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::proc::{self, ProcOutput};
use super::{CaseInput, ExecOutcome, Limits, Sandbox};
use crate::models::SandboxReport;

pub const BACKEND: &str = "macos-sandbox-exec";

pub struct MacSandbox;

impl Sandbox for MacSandbox {
    fn name(&self) -> &'static str {
        BACKEND
    }

    fn detect(&self) -> SandboxReport {
        let mut warnings = vec![
            "sandbox-exec is deprecated by Apple: this is best-effort isolation, not a security boundary."
                .to_string(),
            "Memory limit is not enforced on macOS; only the wall-clock limit is.".to_string(),
        ];

        let Some(se) = sandbox_exec() else {
            return SandboxReport {
                requested_mode: "auto".into(),
                backend: BACKEND.into(),
                can_execute: false,
                python_available: super::python_available(),
                warnings,
                detail: "sandbox-exec not found (expected /usr/bin/sandbox-exec).".into(),
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

        match dry_probe(&se) {
            Ok(()) => SandboxReport {
                requested_mode: "auto".into(),
                backend: BACKEND.into(),
                can_execute: true,
                python_available: true,
                warnings,
                detail: format!(
                    "sandbox-exec OK; python interpreter: {}",
                    python.display()
                ),
            },
            Err(e) => {
                warnings.push(
                    "The sandbox could not be applied in this environment, so local judging is disabled. \
                     This usually means the app is running inside another sandbox (container, CI runner, \
                     or a nested macOS sandbox)."
                        .to_string(),
                );
                SandboxReport {
                    requested_mode: "auto".into(),
                    backend: BACKEND.into(),
                    can_execute: false,
                    python_available: true,
                    warnings,
                    detail: format!("sandbox-exec dry run failed: {e}"),
                }
            }
        }
    }

    fn run(&self, code: &str, cases: &[CaseInput], limits: &Limits) -> Vec<ExecOutcome> {
        let Some(se) = sandbox_exec() else {
            return cases
                .iter()
                .map(|c| ExecOutcome::internal(c.idx, "sandbox-exec disappeared", 0))
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
            let profile_path = workdir.join("profile.sb");
            std::fs::write(&profile_path, profile_for(&workdir))
                .map_err(|e| format!("write profile.sb: {e}"))?;

            let mut out = Vec::with_capacity(cases.len());
            for case in cases {
                let mut cmd = Command::new(&se);
                cmd.arg("-f")
                    .arg(&profile_path)
                    .arg(&python)
                    .arg("solution.py")
                    .current_dir(&workdir);
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

fn sandbox_exec() -> Option<PathBuf> {
    super::find_in_path("sandbox-exec").or_else(|| {
        let p = PathBuf::from("/usr/bin/sandbox-exec");
        p.is_file().then_some(p)
    })
}

/// Run the most permissive profile possible against `/usr/bin/true`. If even
/// that fails, nothing will work at run time.
fn dry_probe(se: &Path) -> Result<(), String> {
    let mut cmd = Command::new(se);
    cmd.arg("-p")
        .arg("(version 1)(allow default)")
        .arg("/usr/bin/true");
    let out: ProcOutput = proc::run(cmd, b"", Duration::from_secs(5));
    if out.spawn_error.is_some() {
        return Err(out.spawn_error.unwrap_or_default());
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
    // Resolve symlinks (/var → /private/var on macOS): the profile matches on
    // the real path, and a symlinked path would silently not match.
    std::fs::canonicalize(&dir).map_err(|e| e.to_string())
}

/// Seatbelt profile for one run.
///
/// `allow default` keeps Python's stdlib readable; the two `deny` lines then
/// carve out network and writes, with the run's workdir (plus /dev for
/// stdin/stdout plumbing) re-allowed.
pub fn profile_for(workdir: &Path) -> String {
    let dir = workdir.display();
    format!(
        "(version 1)\n\
         (allow default)\n\
         (deny network*)\n\
         (deny file-write*)\n\
         (allow file-write* (subpath \"{dir}\"))\n\
         (allow file-write* (subpath \"/dev\"))\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_denies_network_and_confines_writes() {
        let p = profile_for(Path::new("/private/var/folders/x/T/local-leet-abc"));
        assert!(p.contains("(deny network*)"));
        assert!(p.contains("(deny file-write*)"));
        assert!(p.contains("(subpath \"/private/var/folders/x/T/local-leet-abc\")"));
        // The workdir allow must come after the deny, otherwise it is shadowed.
        let deny_at = p.find("(deny file-write*)").unwrap();
        let allow_at = p.find("(allow file-write* (subpath \"/private").unwrap();
        assert!(allow_at > deny_at);
    }
}
