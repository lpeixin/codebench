//! Run orchestration: build the case list, hand it to a sandbox, compare
//! outputs, classify, persist.
//!
//! Comparison lives here (not in the backends) so every backend gets identical
//! judging semantics — DESIGN.md §2.

use crate::db::{now_rfc3339, Db};
use crate::error::{AppError, AppResult};
use crate::models::*;
// `SandboxReport` comes from `models` (via the glob below): the sandbox module
// only imports it privately, it is not a re-export.
use crate::sandbox::{self, CaseInput, ExecOutcome, Limits, Sandbox};

/// How much stderr to keep per case. Enough to see a traceback, short enough
/// to render.
pub const STDERR_EXCERPT_CHARS: usize = 1024;

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

/// Normalise an output for comparison: unify newlines, strip trailing
/// whitespace on each line, drop trailing blank lines.
///
/// Generated and hand-written outputs routinely differ on trailing newlines;
/// failing a correct answer over that would train the user to distrust the
/// judge.
pub fn normalize_output(s: &str) -> String {
    let unified = s.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<&str> = unified.split('\n').map(|l| l.trim_end()).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

pub fn outputs_match(expected: &str, actual: &str) -> bool {
    normalize_output(expected) == normalize_output(actual)
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

pub fn classify(outcome: &ExecOutcome, expected: &str) -> CaseStatus {
    if outcome.error.is_some() {
        return CaseStatus::Internal;
    }
    if outcome.timed_out {
        return CaseStatus::Timeout;
    }
    if outcome.exit_code != Some(0) {
        return CaseStatus::RuntimeError;
    }
    if outputs_match(expected, &outcome.stdout) {
        CaseStatus::Passed
    } else {
        CaseStatus::WrongAnswer
    }
}

pub fn summarize(results: &[CaseResult]) -> RunStatus {
    if results.is_empty() {
        return RunStatus::Error;
    }
    if results.iter().any(|r| r.status == CaseStatus::Internal) {
        return RunStatus::Error;
    }
    if results.iter().all(|r| r.status == CaseStatus::Passed) {
        RunStatus::Passed
    } else {
        RunStatus::Failed
    }
}

fn mode_for_backend(backend: &str) -> RunMode {
    if backend == "mock" {
        RunMode::Mock
    } else {
        RunMode::Sandbox
    }
}

// ---------------------------------------------------------------------------
// Executing runs
// ---------------------------------------------------------------------------

pub fn run_solution(db: &Db, problem_id: &str) -> AppResult<RunSummary> {
    let settings = db.get_settings()?;
    let resolved = sandbox::resolve(settings.sandbox_mode);
    let limits = settings.limits();
    run_with(
        db,
        problem_id,
        &*resolved.sandbox,
        &resolved.report,
        &limits,
    )
}

/// Testable core of [`run_solution`].
pub fn run_with(
    db: &Db,
    problem_id: &str,
    sandbox: &dyn Sandbox,
    report: &SandboxReport,
    limits: &Limits,
) -> AppResult<RunSummary> {
    let problem = db.get_problem(problem_id)?;
    let cases = db.list_test_cases(problem_id)?;
    if cases.is_empty() {
        return Err(AppError::Msg(
            "This problem has no test cases yet. Generate or add some on the Test cases tab.".into(),
        ));
    }
    let solution = db
        .get_solution(problem_id, &problem.language)?
        .filter(|s| !s.code.trim().is_empty())
        .ok_or_else(|| {
            AppError::Msg("Write a solution before running it — the editor is still empty.".into())
        })?;

    // Invariant: no execution without a backend that reported can_execute.
    if !report.can_execute {
        return Err(AppError::Msg(format!(
            "Your code was NOT executed.\n\n{}",
            report.detail
        )));
    }
    if !sandbox.can_run(&problem.language) {
        return Err(AppError::Msg(format!(
            "Backend `{}` cannot run `{}` (MVP supports Python only).",
            report.backend, problem.language
        )));
    }

    let inputs: Vec<CaseInput> = cases
        .iter()
        .map(|c| CaseInput {
            idx: c.idx,
            input: c.input.clone(),
            expected: c.expected_output.clone(),
        })
        .collect();

    let started_at = now_rfc3339();
    let outcomes = sandbox.run(&solution.code, &inputs, limits);

    // A backend that returns fewer results than cases is a bug, but it must
    // surface as an error rather than silently shrinking the report.
    let mut results: Vec<CaseResult> = Vec::with_capacity(cases.len());
    for case in &cases {
        let outcome = outcomes.iter().find(|o| o.idx == case.idx);
        let result = match outcome {
            Some(o) => CaseResult {
                case_id: case.id.clone(),
                idx: case.idx,
                status: classify(o, &case.expected_output),
                expected: case.expected_output.clone(),
                actual: o.stdout.clone(),
                stderr: crate::sandbox::proc::stderr_excerpt(&o.stderr, STDERR_EXCERPT_CHARS),
                time_ms: Some(o.time_ms),
            },
            None => CaseResult {
                case_id: case.id.clone(),
                idx: case.idx,
                status: CaseStatus::Internal,
                expected: case.expected_output.clone(),
                actual: String::new(),
                stderr: Some(format!(
                    "backend `{}` returned no result for this case",
                    report.backend
                )),
                time_ms: None,
            },
        };
        results.push(result);
    }

    let status = summarize(&results);
    let mode = mode_for_backend(&report.backend);
    db.insert_run(
        problem_id,
        Some(&solution.id),
        mode,
        status,
        &started_at,
        &results,
    )
}

// ---------------------------------------------------------------------------
// Logic verification
// ---------------------------------------------------------------------------

/// Compare user-pasted outputs against the expected ones. Never executes.
pub fn verify_outputs(db: &Db, problem_id: &str, outputs: &[String]) -> AppResult<RunSummary> {
    let cases = db.list_test_cases(problem_id)?;
    if cases.is_empty() {
        return Err(AppError::Msg(
            "This problem has no test cases to verify against.".into(),
        ));
    }
    let solution = db.get_solution(problem_id, "python")?;

    let mut results = Vec::with_capacity(cases.len());
    for (i, case) in cases.iter().enumerate() {
        let result = match outputs.get(i) {
            Some(actual) => CaseResult {
                case_id: case.id.clone(),
                idx: case.idx,
                status: if outputs_match(&case.expected_output, actual) {
                    CaseStatus::Passed
                } else {
                    CaseStatus::WrongAnswer
                },
                expected: case.expected_output.clone(),
                actual: actual.clone(),
                stderr: None,
                time_ms: None,
            },
            None => CaseResult {
                case_id: case.id.clone(),
                idx: case.idx,
                status: CaseStatus::Internal,
                expected: case.expected_output.clone(),
                actual: String::new(),
                stderr: Some("No output was pasted for this case.".into()),
                time_ms: None,
            },
        };
        results.push(result);
    }

    let status = summarize(&results);
    db.insert_run(
        problem_id,
        solution.as_ref().map(|s| s.id.as_str()),
        RunMode::Logic,
        status,
        &now_rfc3339(),
        &results,
    )
}

/// Copy-paste block the user feeds to their program outside the app.
pub fn export_case_inputs(db: &Db, problem_id: &str) -> AppResult<String> {
    let problem = db.get_problem(problem_id)?;
    let cases = db.list_test_cases(problem_id)?;
    if cases.is_empty() {
        return Err(AppError::Msg("There are no test cases to export yet.".into()));
    }

    let mut out = String::new();
    out.push_str(&format!("# local-leet — inputs for: {}\n", problem.title));
    out.push_str(
        "# Run your program once per case, feeding ONLY the text between the markers as stdin.\n\
         # Then paste each program's stdout into the matching case on the Run tab.\n\n",
    );
    for (i, case) in cases.iter().enumerate() {
        out.push_str(&format!(
            "##### CASE {} of {} ({}) #####\n",
            i + 1,
            cases.len(),
            if case.is_edge_case { "edge case" } else { "normal" }
        ));
        out.push_str(&case.input);
        if !case.input.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("##### END CASE {} #####\n\n", i + 1));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Validation-by-execution for generated cases
// ---------------------------------------------------------------------------

/// Run the AI's reference solution against its own cases.
///
/// Returns one verdict per case. When no executing sandbox exists, everything
/// is `Unvalidated` — never "validated" by hope.
pub fn validate_cases(
    report: &SandboxReport,
    sandbox: &dyn Sandbox,
    reference: &ReferenceSolution,
    cases: &[GeneratedCase],
    limits: &Limits,
) -> Vec<(Validation, Option<String>)> {
    if !report.can_execute || !sandbox.can_run(&reference.language) {
        return cases.iter().map(|_| (Validation::Unvalidated, None)).collect();
    }

    let inputs: Vec<CaseInput> = cases
        .iter()
        .enumerate()
        .map(|(i, c)| CaseInput {
            idx: i,
            input: c.input.clone(),
            expected: c.expected_output.clone(),
        })
        .collect();
    let outcomes = sandbox.run(&reference.code, &inputs, limits);

    cases
        .iter()
        .enumerate()
        .map(|(i, c)| match outcomes.iter().find(|o| o.idx == i) {
            None => (
                Validation::Invalid,
                Some("the reference solution produced no result".to_string()),
            ),
            Some(o) if o.error.is_some() => (
                Validation::Invalid,
                Some(o.error.clone().unwrap_or_default()),
            ),
            Some(o) if o.timed_out => (
                Validation::Invalid,
                Some("the reference solution timed out".to_string()),
            ),
            Some(o) if o.exit_code != Some(0) => (
                Validation::Invalid,
                Some(format!(
                    "the reference solution failed: {}",
                    o.stderr.trim().lines().last().unwrap_or("non-zero exit")
                )),
            ),
            Some(o) if !outputs_match(&c.expected_output, &o.stdout) => (
                Validation::Invalid,
                Some(format!(
                    "expected {:?} but the reference printed {:?}",
                    c.expected_output.trim(),
                    o.stdout.trim()
                )),
            ),
            Some(_) => (Validation::Validated, None),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::logic::LogicSandbox;
    use crate::sandbox::mock::MockSandbox;

    fn outcome(stdout: &str, exit: Option<i32>) -> ExecOutcome {
        ExecOutcome {
            idx: 0,
            exit_code: exit,
            timed_out: false,
            stdout: stdout.to_string(),
            stderr: String::new(),
            time_ms: 5,
            error: None,
        }
    }

    fn db_with_cases() -> (Db, String) {
        let db = Db::open_in_memory().unwrap();
        let p = db
            .create_problem(CreateProblemReq {
                title: "Sum".into(),
                statement: "sum".into(),
                constraints: None,
                examples: vec![],
                notes: None,
                language: None,
            })
            .unwrap();
        db.replace_cases(
            &p.id,
            "manual",
            &[
                SaveCaseInput {
                    input: "3\n1 2 3\n".into(),
                    expected_output: "6\n".into(),
                    explanation: None,
                    is_edge_case: false,
                },
                SaveCaseInput {
                    input: "0\n".into(),
                    expected_output: "0\n".into(),
                    explanation: None,
                    is_edge_case: true,
                },
            ],
        )
        .unwrap();
        db.save_solution(&p.id, "python", "print(6)").unwrap();
        (db, p.id)
    }

    fn mock_report() -> SandboxReport {
        MockSandbox.detect()
    }

    #[test]
    fn trailing_newlines_do_not_cause_false_negatives() {
        assert!(outputs_match("6\n", "6"));
        assert!(outputs_match("6", "6\n\n"));
        assert!(outputs_match("1\n2\n", "1  \r\n2\r\n"));
        assert!(!outputs_match("6", "7"));
        assert!(!outputs_match("1 2", "1  2"));
        // Leading whitespace is significant: it can be part of the answer.
        assert!(!outputs_match(" 6", "6"));
    }

    #[test]
    fn normalize_keeps_inner_blank_lines() {
        assert_eq!(normalize_output("a\n\nb\n\n"), "a\n\nb");
    }

    #[test]
    fn classification_order_matters() {
        assert_eq!(classify(&outcome("6", Some(0)), "6"), CaseStatus::Passed);
        assert_eq!(classify(&outcome("7", Some(0)), "6"), CaseStatus::WrongAnswer);
        assert_eq!(classify(&outcome("6", Some(1)), "6"), CaseStatus::RuntimeError);

        let mut timed = outcome("", None);
        timed.timed_out = true;
        assert_eq!(classify(&timed, "6"), CaseStatus::Timeout);

        let mut broken = outcome("", None);
        broken.error = Some("spawn failed".into());
        assert_eq!(classify(&broken, "6"), CaseStatus::Internal);
    }

    #[test]
    fn summary_rules() {
        let mk = |s: CaseStatus| CaseResult {
            case_id: "c".into(),
            idx: 0,
            status: s,
            expected: String::new(),
            actual: String::new(),
            stderr: None,
            time_ms: None,
        };
        assert_eq!(summarize(&[]), RunStatus::Error);
        assert_eq!(summarize(&[mk(CaseStatus::Passed)]), RunStatus::Passed);
        assert_eq!(summarize(&[mk(CaseStatus::Passed), mk(CaseStatus::WrongAnswer)]), RunStatus::Failed);
        assert_eq!(summarize(&[mk(CaseStatus::Internal)]), RunStatus::Error);
    }

    #[test]
    fn mock_run_is_persisted_and_passes() {
        let (db, id) = db_with_cases();
        let summary = run_with(
            &db,
            &id,
            &MockSandbox,
            &mock_report(),
            &Limits {
                time_limit_ms: 1000,
                mem_limit_mb: 64,
            },
        )
        .unwrap();
        assert_eq!(summary.mode, RunMode::Mock);
        assert_eq!(summary.status, RunStatus::Passed);
        assert_eq!(summary.results.len(), 2);
        assert_eq!(db.last_run(&id).unwrap().unwrap().id, summary.id);
    }

    #[test]
    fn logic_backend_refuses_to_run() {
        let (db, id) = db_with_cases();
        let err = run_with(
            &db,
            &id,
            &LogicSandbox,
            &LogicSandbox.detect(),
            &Limits {
                time_limit_ms: 1000,
                mem_limit_mb: 64,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("NOT executed"), "{err}");
        // Nothing was recorded: a refused run must not look like a failed one.
        assert!(db.last_run(&id).unwrap().is_none());
    }

    #[test]
    fn missing_solution_is_a_clear_error() {
        let (db, id) = db_with_cases();
        db.save_solution(&id, "python", "   ").unwrap();
        let err = run_with(
            &db,
            &id,
            &MockSandbox,
            &mock_report(),
            &Limits {
                time_limit_ms: 1000,
                mem_limit_mb: 64,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("Write a solution"), "{err}");
    }

    #[test]
    fn verify_compares_pasted_outputs() {
        let (db, id) = db_with_cases();
        let run = verify_outputs(&db, &id, &["6\n".into(), "0".into()]).unwrap();
        assert_eq!(run.mode, RunMode::Logic);
        assert_eq!(run.status, RunStatus::Passed);
        assert_eq!(run.results.len(), 2);

        let run = verify_outputs(&db, &id, &["6\n".into(), "1".into()]).unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(run.results[1].status, CaseStatus::WrongAnswer);
    }

    #[test]
    fn verify_flags_missing_outputs() {
        let (db, id) = db_with_cases();
        let run = verify_outputs(&db, &id, &["6\n".into()]).unwrap();
        assert_eq!(run.status, RunStatus::Error);
        assert_eq!(run.results[1].status, CaseStatus::Internal);
    }

    #[test]
    fn export_block_contains_every_case_and_markers() {
        let (db, id) = db_with_cases();
        let text = export_case_inputs(&db, &id).unwrap();
        assert!(text.contains("CASE 1 of 2"));
        assert!(text.contains("CASE 2 of 2"));
        assert!(text.contains("edge case"));
        assert!(text.contains("3\n1 2 3"));
        assert!(text.contains("END CASE 2"));
    }

    #[test]
    fn validation_without_sandbox_is_unvalidated_not_validated() {
        let (db, id) = db_with_cases();
        let cases: Vec<GeneratedCase> = db
            .list_test_cases(&id)
            .unwrap()
            .into_iter()
            .map(|c| GeneratedCase {
                input: c.input,
                expected_output: c.expected_output,
                explanation: None,
                is_edge_case: c.is_edge_case,
                validation: Validation::Unvalidated,
                invalid_reason: None,
            })
            .collect();
        let verdicts = validate_cases(
            &LogicSandbox.detect(),
            &LogicSandbox,
            &ReferenceSolution {
                language: "python".into(),
                code: "print(1)".into(),
            },
            &cases,
            &Limits {
                time_limit_ms: 1000,
                mem_limit_mb: 64,
            },
        );
        assert_eq!(verdicts.len(), cases.len());
        assert!(verdicts.iter().all(|(v, _)| *v == Validation::Unvalidated));
    }
}
