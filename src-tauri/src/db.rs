//! SQLite persistence. All SQL in the app lives here.
//!
//! One connection guarded by a mutex: the app is single-user and every query is
//! small, so a pool would be ceremony. Long operations (AI calls, sandbox runs)
//! never hold the lock — they take a snapshot of the rows they need and return.

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::models::*;

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS problems (
  id          TEXT PRIMARY KEY,
  title       TEXT NOT NULL,
  statement   TEXT NOT NULL,
  constraints TEXT,
  examples    TEXT,
  notes       TEXT,
  language    TEXT NOT NULL DEFAULT 'python',
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS test_cases (
  id              TEXT PRIMARY KEY,
  problem_id      TEXT NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
  idx             INTEGER NOT NULL,
  input           TEXT NOT NULL,
  expected_output TEXT NOT NULL,
  is_edge_case    INTEGER NOT NULL DEFAULT 0,
  source          TEXT NOT NULL,
  explanation     TEXT
);

CREATE INDEX IF NOT EXISTS idx_test_cases_problem ON test_cases(problem_id, idx);

CREATE TABLE IF NOT EXISTS solutions (
  id         TEXT PRIMARY KEY,
  problem_id TEXT NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
  language   TEXT NOT NULL,
  code       TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_solutions_problem_lang
  ON solutions(problem_id, language);

-- AI-generated reference solutions are kept separate from the user's own
-- solution: generating cases must never clobber work in progress.
CREATE TABLE IF NOT EXISTS reference_solutions (
  problem_id TEXT PRIMARY KEY REFERENCES problems(id) ON DELETE CASCADE,
  language   TEXT NOT NULL,
  code       TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
  id           TEXT PRIMARY KEY,
  problem_id   TEXT NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
  solution_id  TEXT,
  mode         TEXT NOT NULL,
  status       TEXT NOT NULL,
  started_at   TEXT NOT NULL,
  results_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runs_problem ON runs(problem_id, started_at DESC);

CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute_batch(SCHEMA)?;
        Ok(())
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| AppError::Msg("database lock poisoned".into()))
    }

    // ------------------------------------------------------------------
    // Problems
    // ------------------------------------------------------------------

    pub fn list_problems(&self) -> AppResult<Vec<Problem>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, statement, constraints, examples, notes, language,
                    created_at, updated_at
             FROM problems ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], map_problem)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_problem(&self, id: &str) -> AppResult<Problem> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, title, statement, constraints, examples, notes, language,
                    created_at, updated_at
             FROM problems WHERE id = ?1",
            params![id],
            map_problem,
        )
        .optional()?
        .ok_or_else(|| AppError::Msg("problem not found".into()))
    }

    pub fn create_problem(&self, req: CreateProblemReq) -> AppResult<Problem> {
        let title = req.title.trim().to_string();
        if title.is_empty() {
            return Err(AppError::Msg("title is required".into()));
        }
        if req.statement.trim().is_empty() {
            return Err(AppError::Msg("statement is required".into()));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let ts = now_rfc3339();
        let examples = serde_json::to_string(&req.examples)?;
        let language = req
            .language
            .filter(|l| !l.trim().is_empty())
            .unwrap_or_else(|| "python".to_string());
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO problems
               (id, title, statement, constraints, examples, notes, language, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                id,
                title,
                req.statement,
                blank_to_none(req.constraints),
                examples,
                blank_to_none(req.notes),
                language,
                ts
            ],
        )?;
        drop(conn);
        self.get_problem(&id)
    }

    pub fn update_problem(&self, id: &str, req: UpdateProblemReq) -> AppResult<Problem> {
        let title = req.title.trim().to_string();
        if title.is_empty() {
            return Err(AppError::Msg("title is required".into()));
        }
        let ts = now_rfc3339();
        let conn = self.lock()?;
        let changed = conn.execute(
            "UPDATE problems
                SET title = ?2, statement = ?3, constraints = ?4, notes = ?5, updated_at = ?6
              WHERE id = ?1",
            params![
                id,
                title,
                req.statement,
                blank_to_none(req.constraints),
                blank_to_none(req.notes),
                ts
            ],
        )?;
        if changed == 0 {
            return Err(AppError::Msg("problem not found".into()));
        }
        drop(conn);
        self.get_problem(id)
    }

    pub fn delete_problem(&self, id: &str) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM problems WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Test cases
    // ------------------------------------------------------------------

    pub fn list_test_cases(&self, problem_id: &str) -> AppResult<Vec<TestCase>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, problem_id, idx, input, expected_output, is_edge_case, source, explanation
             FROM test_cases WHERE problem_id = ?1 ORDER BY idx ASC",
        )?;
        let rows = stmt.query_map(params![problem_id], map_test_case)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Replace the cases owned by `source`.
    ///
    /// - `ai` / `mock`: wipes every AI-produced case (both sources — they are
    ///   interchangeable drafts) and keeps manual ones.
    /// - `manual`: wipes and rewrites the manual set, so the UI can send its
    ///   whole edited list without diffing.
    pub fn replace_cases(
        &self,
        problem_id: &str,
        source: &str,
        cases: &[SaveCaseInput],
    ) -> AppResult<Vec<TestCase>> {
        if !matches!(source, "ai" | "mock" | "manual") {
            return Err(AppError::Msg(format!("unknown case source: {source}")));
        }
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        if source == "manual" {
            tx.execute(
                "DELETE FROM test_cases WHERE problem_id = ?1 AND source = 'manual'",
                params![problem_id],
            )?;
        } else {
            tx.execute(
                "DELETE FROM test_cases WHERE problem_id = ?1 AND source IN ('ai','mock')",
                params![problem_id],
            )?;
        }

        // Keep manual cases first so their indices stay stable across
        // regenerations; new AI cases are appended after them.
        let offset: i64 = tx.query_row(
            "SELECT COALESCE(MAX(idx) + 1, 0) FROM test_cases WHERE problem_id = ?1",
            params![problem_id],
            |r| r.get(0),
        )?;

        for (i, c) in cases.iter().enumerate() {
            if c.expected_output.trim().is_empty() && c.input.trim().is_empty() {
                continue; // fully empty row: ignore rather than persist junk
            }
            tx.execute(
                "INSERT INTO test_cases
                   (id, problem_id, idx, input, expected_output, is_edge_case, source, explanation)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    problem_id,
                    offset + i as i64,
                    c.input,
                    c.expected_output,
                    c.is_edge_case as i64,
                    source,
                    blank_to_none(c.explanation.clone()),
                ],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.list_test_cases(problem_id)
    }

    // ------------------------------------------------------------------
    // Solutions
    // ------------------------------------------------------------------

    pub fn get_solution(&self, problem_id: &str, language: &str) -> AppResult<Option<Solution>> {
        let conn = self.lock()?;
        let out = conn
            .query_row(
                "SELECT id, problem_id, language, code, updated_at
                   FROM solutions WHERE problem_id = ?1 AND language = ?2",
                params![problem_id, language],
                map_solution,
            )
            .optional()?;
        Ok(out)
    }

    pub fn save_solution(&self, problem_id: &str, language: &str, code: &str) -> AppResult<Solution> {
        let ts = now_rfc3339();
        let conn = self.lock()?;
        // Upsert keeps one row per (problem, language) as the schema promises.
        conn.execute(
            "INSERT INTO solutions (id, problem_id, language, code, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(problem_id, language) DO UPDATE
               SET code = excluded.code, updated_at = excluded.updated_at",
            params![
                uuid::Uuid::new_v4().to_string(),
                problem_id,
                language,
                code,
                ts
            ],
        )?;
        drop(conn);
        self.get_solution(problem_id, language)?
            .ok_or_else(|| AppError::Msg("solution vanished after save".into()))
    }

    // ------------------------------------------------------------------
    // Reference solutions
    // ------------------------------------------------------------------

    pub fn get_reference(&self, problem_id: &str) -> AppResult<Option<ReferenceSolution>> {
        let conn = self.lock()?;
        let out = conn
            .query_row(
                "SELECT language, code FROM reference_solutions WHERE problem_id = ?1",
                params![problem_id],
                |r| {
                    Ok(ReferenceSolution {
                        language: r.get(0)?,
                        code: r.get(1)?,
                    })
                },
            )
            .optional()?;
        Ok(out)
    }

    pub fn save_reference(&self, problem_id: &str, reference: &ReferenceSolution) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO reference_solutions (problem_id, language, code, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(problem_id) DO UPDATE
               SET language = excluded.language, code = excluded.code,
                   created_at = excluded.created_at",
            params![
                problem_id,
                reference.language,
                reference.code,
                now_rfc3339()
            ],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Runs
    // ------------------------------------------------------------------

    pub fn insert_run(
        &self,
        problem_id: &str,
        solution_id: Option<&str>,
        mode: RunMode,
        status: RunStatus,
        started_at: &str,
        results: &[CaseResult],
    ) -> AppResult<RunSummary> {
        let id = uuid::Uuid::new_v4().to_string();
        let results_json = serde_json::to_string(results)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO runs (id, problem_id, solution_id, mode, status, started_at, results_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                problem_id,
                solution_id,
                mode.as_str(),
                status.as_str(),
                started_at,
                results_json
            ],
        )?;
        Ok(RunSummary {
            id,
            problem_id: problem_id.to_string(),
            mode,
            status,
            started_at: started_at.to_string(),
            results: results.to_vec(),
        })
    }

    pub fn list_runs(&self, problem_id: &str, limit: usize) -> AppResult<Vec<RunSummary>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, problem_id, mode, status, started_at, results_json
               FROM runs WHERE problem_id = ?1 ORDER BY started_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![problem_id, limit as i64], map_run)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn last_run(&self, problem_id: &str) -> AppResult<Option<RunSummary>> {
        Ok(self.list_runs(problem_id, 1)?.into_iter().next())
    }

    // ------------------------------------------------------------------
    // Settings
    // ------------------------------------------------------------------

    pub fn get_settings(&self) -> AppResult<AppSettings> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut map = std::collections::HashMap::new();
        for r in rows {
            let (k, v) = r?;
            map.insert(k, v);
        }
        let mut s = AppSettings::default();
        if let Some(v) = map.get("aiEndpoint") {
            s.ai_endpoint = v.clone();
        }
        if let Some(v) = map.get("aiModel") {
            s.ai_model = v.clone();
        }
        if let Some(v) = map.get("aiMode") {
            s.ai_mode = AiMode::parse(v);
        }
        if let Some(v) = map.get("numTestCases").and_then(|v| v.parse().ok()) {
            s.num_test_cases = v;
        }
        if let Some(v) = map.get("timeLimitSec").and_then(|v| v.parse().ok()) {
            s.time_limit_sec = v;
        }
        if let Some(v) = map.get("memLimitMb").and_then(|v| v.parse().ok()) {
            s.mem_limit_mb = v;
        }
        if let Some(v) = map.get("sandboxMode") {
            s.sandbox_mode = SandboxMode::parse(v);
        }
        Ok(s.sanitized())
    }

    pub fn save_settings(&self, settings: &AppSettings) -> AppResult<AppSettings> {
        let s = settings.clone().sanitized();
        let pairs = [
            ("aiEndpoint", s.ai_endpoint.clone()),
            ("aiModel", s.ai_model.clone()),
            ("aiMode", s.ai_mode.as_str().to_string()),
            ("numTestCases", s.num_test_cases.to_string()),
            ("timeLimitSec", s.time_limit_sec.to_string()),
            ("memLimitMb", s.mem_limit_mb.to_string()),
            ("sandboxMode", s.sandbox_mode.as_str().to_string()),
        ];
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        for (k, v) in pairs {
            tx.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![k, v],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.get_settings()
    }

    pub fn detail(&self, problem_id: &str) -> AppResult<ProblemDetail> {
        let problem = self.get_problem(problem_id)?;
        let test_cases = self.list_test_cases(problem_id)?;
        let solution = self.get_solution(problem_id, &problem.language)?;
        let reference = self.get_reference(problem_id)?;
        let last_run = self.last_run(problem_id)?;
        Ok(ProblemDetail {
            problem,
            test_cases,
            solution,
            reference,
            last_run,
        })
    }
}

// ---------------------------------------------------------------------------
// Row mapping helpers
// ---------------------------------------------------------------------------

fn blank_to_none(v: Option<String>) -> Option<String> {
    v.and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
}

fn map_problem(row: &Row<'_>) -> rusqlite::Result<Problem> {
    let examples_json: Option<String> = row.get(4)?;
    let examples: Vec<Example> = examples_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    Ok(Problem {
        id: row.get(0)?,
        title: row.get(1)?,
        statement: row.get(2)?,
        constraints: row.get(3)?,
        examples,
        notes: row.get(5)?,
        language: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_test_case(row: &Row<'_>) -> rusqlite::Result<TestCase> {
    Ok(TestCase {
        id: row.get(0)?,
        problem_id: row.get(1)?,
        idx: row.get::<_, i64>(2)? as usize,
        input: row.get(3)?,
        expected_output: row.get(4)?,
        is_edge_case: row.get::<_, i64>(5)? != 0,
        source: row.get(6)?,
        explanation: row.get(7)?,
    })
}

fn map_solution(row: &Row<'_>) -> rusqlite::Result<Solution> {
    Ok(Solution {
        id: row.get(0)?,
        problem_id: row.get(1)?,
        language: row.get(2)?,
        code: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn map_run(row: &Row<'_>) -> rusqlite::Result<RunSummary> {
    let mode_s: String = row.get(2)?;
    let status_s: String = row.get(3)?;
    let results_json: String = row.get(5)?;
    let results: Vec<CaseResult> = serde_json::from_str(&results_json).unwrap_or_default();
    Ok(RunSummary {
        id: row.get(0)?,
        problem_id: row.get(1)?,
        mode: match mode_s.as_str() {
            "logic" => RunMode::Logic,
            "mock" => RunMode::Mock,
            _ => RunMode::Sandbox,
        },
        status: match status_s.as_str() {
            "passed" => RunStatus::Passed,
            "failed" => RunStatus::Failed,
            _ => RunStatus::Error,
        },
        started_at: row.get(4)?,
        results,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_req() -> CreateProblemReq {
        CreateProblemReq {
            title: "Two Sum".into(),
            statement: "Given an array…".into(),
            constraints: Some("2 <= n".into()),
            examples: vec![Example {
                input: "[2,7]\n9".into(),
                output: "0 1".into(),
                explanation: None,
            }],
            notes: None,
            language: None,
        }
    }

    #[test]
    fn problem_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let p = db.create_problem(sample_req()).unwrap();
        assert_eq!(p.title, "Two Sum");
        assert_eq!(p.language, "python");
        assert_eq!(p.examples.len(), 1);

        let listed = db.list_problems().unwrap();
        assert_eq!(listed.len(), 1);

        let updated = db
            .update_problem(
                &p.id,
                UpdateProblemReq {
                    title: "Two Sum II".into(),
                    statement: "…".into(),
                    constraints: None,
                    notes: Some("note".into()),
                },
            )
            .unwrap();
        assert_eq!(updated.title, "Two Sum II");
        assert!(updated.constraints.is_none());

        db.delete_problem(&p.id).unwrap();
        assert!(db.list_problems().unwrap().is_empty());
    }

    #[test]
    fn create_rejects_blank_fields() {
        let db = Db::open_in_memory().unwrap();
        let mut req = sample_req();
        req.title = "   ".into();
        assert!(db.create_problem(req).is_err());
    }

    #[test]
    fn replacing_ai_cases_preserves_manual_ones() {
        let db = Db::open_in_memory().unwrap();
        let p = db.create_problem(sample_req()).unwrap();

        let manual = vec![SaveCaseInput {
            input: "1\n2\n".into(),
            expected_output: "3\n".into(),
            explanation: None,
            is_edge_case: false,
        }];
        db.replace_cases(&p.id, "manual", &manual).unwrap();

        let ai = vec![
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
        ];
        let after = db.replace_cases(&p.id, "ai", &ai).unwrap();
        assert_eq!(after.len(), 3);
        assert_eq!(after[0].source, "manual");

        // Regenerating replaces AI cases only, and indices stay ordered.
        let after2 = db.replace_cases(&p.id, "ai", &ai[..1]).unwrap();
        assert_eq!(after2.len(), 2);
        assert_eq!(after2[0].source, "manual");
        assert_eq!(after2[1].source, "ai");
        assert!(after2[0].idx < after2[1].idx);
    }

    #[test]
    fn solution_upsert_keeps_single_row() {
        let db = Db::open_in_memory().unwrap();
        let p = db.create_problem(sample_req()).unwrap();
        db.save_solution(&p.id, "python", "print(1)").unwrap();
        let s = db.save_solution(&p.id, "python", "print(2)").unwrap();
        assert_eq!(s.code, "print(2)");
        let all = db.list_test_cases(&p.id).unwrap();
        assert!(all.is_empty());
    }

    #[test]
    fn settings_roundtrip_and_clamp() {
        let db = Db::open_in_memory().unwrap();
        let defaults = db.get_settings().unwrap();
        assert_eq!(defaults.ai_mode, AiMode::Auto);
        assert_eq!(defaults.sandbox_mode, SandboxMode::Auto);

        let mut s = defaults.clone();
        s.ai_mode = AiMode::Ollama;
        s.sandbox_mode = SandboxMode::Logic;
        s.num_test_cases = 99;
        s.time_limit_sec = 0;
        s.ai_endpoint = "http://127.0.0.1:11434/".into();
        let saved = db.save_settings(&s).unwrap();
        assert_eq!(saved.num_test_cases, 20);
        assert_eq!(saved.time_limit_sec, 1);
        assert_eq!(saved.ai_endpoint, "http://127.0.0.1:11434");
        assert_eq!(saved.ai_mode, AiMode::Ollama);
        assert_eq!(saved.sandbox_mode, SandboxMode::Logic);
    }

    #[test]
    fn runs_are_persisted_newest_first() {
        let db = Db::open_in_memory().unwrap();
        let p = db.create_problem(sample_req()).unwrap();
        let results = vec![CaseResult {
            case_id: "c1".into(),
            idx: 0,
            status: CaseStatus::Passed,
            expected: "1".into(),
            actual: "1".into(),
            stderr: None,
            time_ms: Some(12),
        }];
        let r = db
            .insert_run(
                &p.id,
                None,
                RunMode::Sandbox,
                RunStatus::Passed,
                "2026-01-01T00:00:00Z",
                &results,
            )
            .unwrap();
        assert_eq!(r.results.len(), 1);
        let runs = db.list_runs(&p.id, 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].mode, RunMode::Sandbox);
    }
}
