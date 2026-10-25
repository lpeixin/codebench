//! Deterministic offline provider.
//!
//! It does not pretend to understand the problem. It always returns a fixed set
//! of cases for a built-in "sum of n integers" mini-problem, plus a reference
//! solution that actually produces those outputs. That makes the whole review
//! flow testable with zero network access, and the warning returned with every
//! draft says exactly what happened.

use super::{AiDraft, AiProvider, AiRequest};
use crate::error::AppResult;
use crate::models::{GeneratedCase, ReferenceSolution, Validation};

pub struct MockAiProvider;

pub const REFERENCE_CODE: &str = r#"import sys


def main() -> None:
    data = sys.stdin.read().split()
    if not data:
        print(0)
        return
    n = int(data[0])
    values = [int(x) for x in data[1:1 + n]]
    print(sum(values))


if __name__ == "__main__":
    main()
"#;

/// (input, expected output, explanation, is_edge_case)
const CASES: &[(&str, &str, &str, bool)] = &[
    ("3\n1 2 3\n", "6\n", "Three positive integers.", false),
    ("1\n10\n", "10\n", "Single element.", false),
    ("5\n-2 4 0 7 -1\n", "8\n", "Includes negative values and zero.", false),
    ("0\n", "0\n", "Empty sequence: sum is zero.", true),
    (
        "4\n1000000000 1000000000 1000000000 1000000000\n",
        "4000000000\n",
        "Sum exceeds 32-bit range.",
        true,
    ),
    ("2\n7 -7\n", "0\n", "Values cancel out.", false),
    ("6\n1 1 1 1 1 1\n", "6\n", "All duplicates.", false),
    ("3\n-5 -5 -5\n", "-15\n", "All negative.", true),
];

impl AiProvider for MockAiProvider {
    fn label(&self) -> String {
        "mock".to_string()
    }

    fn generate(&self, req: &AiRequest) -> AppResult<AiDraft> {
        let wanted = req.num_cases.clamp(1, CASES.len());
        let mut warnings = vec![
            "Mock provider: these cases belong to a built-in demo problem (sum of n integers), \
             not to your statement. Configure Ollama in Settings for real generation."
                .to_string(),
        ];
        if req.num_cases > CASES.len() {
            warnings.push(format!(
                "The mock provider only has {} cases; you asked for {}.",
                CASES.len(),
                req.num_cases
            ));
        }

        let cases = CASES[..wanted]
            .iter()
            .map(|(input, expected, explanation, is_edge)| GeneratedCase {
                input: (*input).to_string(),
                expected_output: (*expected).to_string(),
                explanation: Some((*explanation).to_string()),
                is_edge_case: *is_edge,
                validation: Validation::Unvalidated,
                invalid_reason: None,
            })
            .collect();

        Ok(AiDraft {
            cases,
            reference: Some(ReferenceSolution {
                language: "python".to_string(),
                code: REFERENCE_CODE.to_string(),
            }),
            constraints_summary: Some(
                "0 <= n <= 10^5, each value fits in a signed 64-bit integer. \
                 Read n then n integers from stdin, print their sum."
                    .to_string(),
            ),
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::{proc, python_probe};
    use std::process::Command;
    use std::time::Duration;

    fn req(n: usize) -> AiRequest {
        AiRequest {
            title: "Sum of integers".into(),
            statement: "Read n and n integers, print their sum.".into(),
            constraints: None,
            examples: vec![],
            num_cases: n,
        }
    }

    #[test]
    fn deterministic_and_bounded() {
        let a = MockAiProvider.generate(&req(6)).unwrap();
        let b = MockAiProvider.generate(&req(6)).unwrap();
        assert_eq!(a.cases.len(), 6);
        assert_eq!(a.cases[0].input, b.cases[0].input);
        assert_eq!(a.cases[0].expected_output, b.cases[0].expected_output);
        assert!(a.warnings[0].contains("Mock provider"));
        assert!(a.reference.is_some());
    }

    #[test]
    fn asks_for_more_than_it_has() {
        let d = MockAiProvider.generate(&req(20)).unwrap();
        assert_eq!(d.cases.len(), CASES.len());
        assert!(d.warnings.iter().any(|w| w.contains("only has")));
    }

    /// The mock is only useful if its reference solution really reproduces its
    /// expected outputs. Runs the trusted fixture (not user code) directly.
    #[test]
    fn reference_matches_every_expected_output() {
        let Some(python) = python_probe() else {
            eprintln!("python3 unavailable, skipping consistency check");
            return;
        };
        let draft = MockAiProvider.generate(&req(CASES.len())).unwrap();
        for case in &draft.cases {
            let mut cmd = Command::new(&python);
            cmd.arg("-c").arg(REFERENCE_CODE);
            let out = proc::run(cmd, case.input.as_bytes(), Duration::from_secs(10));
            assert!(
                out.success(),
                "reference failed on input {:?}: {:?}",
                case.input,
                out.stderr
            );
            assert_eq!(
                out.stdout.trim(),
                case.expected_output.trim(),
                "mismatch for input {:?}",
                case.input
            );
        }
    }
}
