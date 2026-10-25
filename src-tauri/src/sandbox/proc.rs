//! Child-process supervision.
//!
//! Everything that executes a program goes through [`run`]: it feeds stdin,
//! captures stdout/stderr, and enforces the wall-clock deadline by killing the
//! child. Backends layer their own isolation on top (sandbox-exec, nsjail);
//! this module never decides *whether* execution is safe.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Cap on captured output. A runaway program can otherwise fill memory with
/// its own stdout before the deadline fires.
pub const MAX_CAPTURE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct ProcOutput {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub time_ms: u64,
    /// Set when the process could not even be started (missing binary, EACCES…).
    pub spawn_error: Option<String>,
}

impl ProcOutput {
    fn failed(err: String, time_ms: u64) -> Self {
        Self {
            exit_code: None,
            timed_out: false,
            stdout: String::new(),
            stderr: String::new(),
            time_ms,
            spawn_error: Some(err),
        }
    }

    pub fn success(&self) -> bool {
        self.spawn_error.is_none() && !self.timed_out && self.exit_code == Some(0)
    }
}

/// Read a pipe to the end, keeping at most [`MAX_CAPTURE_BYTES`].
fn drain<R: Read + Send + 'static>(mut reader: R) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    if buf.len() < MAX_CAPTURE_BYTES {
                        let take = n.min(MAX_CAPTURE_BYTES - buf.len());
                        buf.extend_from_slice(&chunk[..take]);
                    }
                    // Keep draining past the cap so the child never blocks on a
                    // full pipe; we simply stop storing.
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(buf);
    });
    rx
}

/// Run `cmd`, feeding `stdin_data`, killing it after `timeout`.
pub fn run(mut cmd: Command, stdin_data: &[u8], timeout: Duration) -> ProcOutput {
    let started = Instant::now();
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return ProcOutput::failed(
                format!("failed to start process: {e}"),
                started.elapsed().as_millis() as u64,
            )
        }
    };

    // Feed stdin from a thread: a large input would otherwise deadlock against
    // a child that has not started reading yet.
    let mut child_stdin = child.stdin.take();
    let input = stdin_data.to_vec();
    let writer = thread::spawn(move || {
        if let Some(mut pipe) = child_stdin.take() {
            let _ = pipe.write_all(&input);
            let _ = pipe.flush();
            // Dropping closes the pipe → the child sees EOF.
        }
    });

    let stdout_rx = child.stdout.take().map(drain);
    let stderr_rx = child.stderr.take().map(drain);

    let deadline = started + timeout;
    let mut timed_out = false;
    let exit_code;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_code = status.code();
                break;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    let _ = child.kill();
                    let _ = child.wait();
                    exit_code = None;
                    break;
                }
                thread::sleep(Duration::from_millis(4));
            }
            Err(e) => {
                let _ = child.kill();
                return ProcOutput::failed(
                    format!("failed while waiting for process: {e}"),
                    started.elapsed().as_millis() as u64,
                );
            }
        }
    }

    let _ = writer.join();
    let stdout = stdout_rx
        .and_then(|rx| rx.recv_timeout(Duration::from_secs(2)).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let stderr = stderr_rx
        .and_then(|rx| rx.recv_timeout(Duration::from_secs(2)).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();

    ProcOutput {
        exit_code,
        timed_out,
        stdout,
        stderr,
        time_ms: started.elapsed().as_millis() as u64,
        spawn_error: None,
    }
}

/// First line or two of stderr, for the per-case report.
pub fn stderr_excerpt(stderr: &str, max: usize) -> Option<String> {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out: String = trimmed.chars().take(max).collect();
    if trimmed.chars().count() > max {
        out.push_str("\n… (truncated)");
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(script);
        c
    }

    #[test]
    fn captures_stdout_and_exit_code() {
        let out = run(sh("cat; echo done"), b"hello\n", Duration::from_secs(5));
        assert_eq!(out.stdout, "hello\ndone\n");
        assert_eq!(out.exit_code, Some(0));
        assert!(out.success());
        assert!(!out.timed_out);
    }

    #[test]
    fn kills_on_deadline() {
        let out = run(sh("sleep 5"), b"", Duration::from_millis(300));
        assert!(out.timed_out, "expected timeout, got {out:?}");
        assert!(out.time_ms < 4000, "kill took too long: {}ms", out.time_ms);
    }

    #[test]
    fn reports_nonzero_exit_and_stderr() {
        let out = run(
            sh("echo boom >&2; exit 3"),
            b"",
            Duration::from_secs(5),
        );
        assert_eq!(out.exit_code, Some(3));
        assert!(out.stderr.contains("boom"));
        assert!(!out.success());
    }

    #[test]
    fn missing_binary_is_a_spawn_error() {
        let mut c = Command::new("/nonexistent/binary/xyz");
        c.arg("--help");
        let out = run(c, b"", Duration::from_secs(1));
        assert!(out.spawn_error.is_some());
        assert!(!out.success());
    }

    #[test]
    fn large_stdin_does_not_deadlock() {
        // 1 MiB is far beyond a pipe buffer: without the writer thread this
        // would hang until the deadline.
        let input = vec![b'a'; 1024 * 1024];
        let out = run(
            sh("wc -c | tr -d ' \\n'"),
            &input,
            Duration::from_secs(10),
        );
        assert_eq!(out.stdout.trim(), (1024 * 1024).to_string());
    }

    #[test]
    fn output_is_capped() {
        let out = run(
            sh("head -c 200000 /dev/zero | tr '\\0' 'x'"),
            b"",
            Duration::from_secs(10),
        );
        assert_eq!(out.stdout.len(), MAX_CAPTURE_BYTES);
    }
}
