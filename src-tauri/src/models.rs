//! Domain types and the JSON DTO shapes shared with the frontend.
//!
//! Every struct that crosses the IPC boundary uses
//! `#[serde(rename_all = "camelCase")]` so it lines up with `src/types.ts`
//! without a translation layer. Behaviour lives elsewhere: this module is
//! deliberately data-only.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Problem
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    pub input: String,
    pub output: String,
    #[serde(default)]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub id: String,
    pub title: String,
    pub statement: String,
    pub constraints: Option<String>,
    pub examples: Vec<Example>,
    pub notes: Option<String>,
    pub language: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProblemReq {
    pub title: String,
    pub statement: String,
    #[serde(default)]
    pub constraints: Option<String>,
    #[serde(default)]
    pub examples: Vec<Example>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProblemReq {
    pub title: String,
    pub statement: String,
    #[serde(default)]
    pub constraints: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// Test cases, solutions, runs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCase {
    pub id: String,
    pub problem_id: String,
    pub idx: usize,
    pub input: String,
    pub expected_output: String,
    pub is_edge_case: bool,
    pub source: String,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCaseInput {
    pub input: String,
    pub expected_output: String,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub is_edge_case: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Solution {
    pub id: String,
    pub problem_id: String,
    pub language: String,
    pub code: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceSolution {
    pub language: String,
    pub code: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Passed,
    WrongAnswer,
    Timeout,
    RuntimeError,
    Internal,
}

impl CaseStatus {
    /// Short label used in log lines and the persisted run summary.
    pub fn as_str(self) -> &'static str {
        match self {
            CaseStatus::Passed => "passed",
            CaseStatus::WrongAnswer => "wrong_answer",
            CaseStatus::Timeout => "timeout",
            CaseStatus::RuntimeError => "runtime_error",
            CaseStatus::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseResult {
    pub case_id: String,
    pub idx: usize,
    pub status: CaseStatus,
    pub expected: String,
    pub actual: String,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub time_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Sandbox,
    Logic,
    Mock,
}

impl RunMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RunMode::Sandbox => "sandbox",
            RunMode::Logic => "logic",
            RunMode::Mock => "mock",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Passed,
    Failed,
    Error,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Passed => "passed",
            RunStatus::Failed => "failed",
            RunStatus::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: String,
    pub problem_id: String,
    pub mode: RunMode,
    pub status: RunStatus,
    pub started_at: String,
    pub results: Vec<CaseResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetail {
    pub problem: Problem,
    pub test_cases: Vec<TestCase>,
    pub solution: Option<Solution>,
    /// Reference solution produced by the AI during generation, kept apart from
    /// the user's own solution so it never overwrites their work.
    pub reference: Option<ReferenceSolution>,
    pub last_run: Option<RunSummary>,
}

// ---------------------------------------------------------------------------
// AI generation draft (review-before-save)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Validation {
    /// Reference solution reproduced this case's expected output.
    Validated,
    /// No executing sandbox was available, so the case could not be checked.
    Unvalidated,
    /// Reference solution disagreed with the expected output, or failed to run.
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedCase {
    pub input: String,
    pub expected_output: String,
    pub explanation: Option<String>,
    pub is_edge_case: bool,
    pub validation: Validation,
    pub invalid_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationDraft {
    pub provider: String,
    pub constraints_summary: Option<String>,
    pub reference_solution: Option<ReferenceSolution>,
    pub cases: Vec<GeneratedCase>,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMode {
    Auto,
    Ollama,
    Mock,
}

impl AiMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AiMode::Auto => "auto",
            AiMode::Ollama => "ollama",
            AiMode::Mock => "mock",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "ollama" => AiMode::Ollama,
            "mock" => AiMode::Mock,
            _ => AiMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    Auto,
    Native,
    Logic,
    Mock,
}

impl SandboxMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SandboxMode::Auto => "auto",
            SandboxMode::Native => "native",
            SandboxMode::Logic => "logic",
            SandboxMode::Mock => "mock",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "native" => SandboxMode::Native,
            "logic" => SandboxMode::Logic,
            "mock" => SandboxMode::Mock,
            _ => SandboxMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub ai_endpoint: String,
    pub ai_model: String,
    pub ai_mode: AiMode,
    pub num_test_cases: usize,
    pub time_limit_sec: u64,
    pub mem_limit_mb: u64,
    pub sandbox_mode: SandboxMode,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai_endpoint: "http://localhost:11434".to_string(),
            ai_model: "llama3.1".to_string(),
            ai_mode: AiMode::Auto,
            num_test_cases: 6,
            time_limit_sec: 3,
            mem_limit_mb: 256,
            sandbox_mode: SandboxMode::Auto,
        }
    }
}

impl AppSettings {
    /// Clamp user input into ranges the judge can actually honour.
    pub fn sanitized(mut self) -> Self {
        self.num_test_cases = self.num_test_cases.clamp(1, 20);
        self.time_limit_sec = self.time_limit_sec.clamp(1, 60);
        self.mem_limit_mb = self.mem_limit_mb.clamp(32, 4096);
        if self.ai_endpoint.trim().is_empty() {
            self.ai_endpoint = AppSettings::default().ai_endpoint;
        }
        if self.ai_model.trim().is_empty() {
            self.ai_model = AppSettings::default().ai_model;
        }
        self.ai_endpoint = self.ai_endpoint.trim_end_matches('/').to_string();
        self
    }

    pub fn limits(&self) -> crate::sandbox::Limits {
        crate::sandbox::Limits {
            time_limit_ms: self.time_limit_sec.saturating_mul(1000),
            mem_limit_mb: self.mem_limit_mb,
        }
    }
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxReport {
    /// The mode the user asked for in settings.
    pub requested_mode: String,
    /// The backend that will actually be used.
    pub backend: String,
    pub can_execute: bool,
    pub python_available: bool,
    pub warnings: Vec<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReport {
    pub provider: String,
    pub ok: bool,
    pub detail: String,
    pub models: Vec<String>,
}
