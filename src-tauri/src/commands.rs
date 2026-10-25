//! Tauri command surface: argument marshalling, provider/backend selection, and
//! error text. No business logic beyond wiring (DESIGN.md §2).

use tauri::State;

use crate::ai::{self, AiProvider, AiRequest};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::judge;
use crate::models::*;
use crate::sandbox;

pub struct AppState {
    pub db: Db,
}

type CmdResult<T> = Result<T, String>;

fn to_msg(e: AppError) -> String {
    e.to_string()
}

/// Run a blocking job off the UI thread.
///
/// Every command is `async` for this reason: Tauri executes synchronous
/// commands on the main thread, and an Ollama call can take a minute.
macro_rules! blocking {
    ($body:expr) => {
        tauri::async_runtime::spawn_blocking(move || $body)
            .await
            .map_err(|e| format!("internal task error: {e}"))?
    };
}

// ---------------------------------------------------------------------------
// Problems
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_problems(state: State<'_, AppState>) -> CmdResult<Vec<Problem>> {
    state.db.list_problems().map_err(to_msg)
}

#[tauri::command]
pub fn create_problem(state: State<'_, AppState>, req: CreateProblemReq) -> CmdResult<Problem> {
    state.db.create_problem(req).map_err(to_msg)
}

#[tauri::command]
pub fn get_problem(state: State<'_, AppState>, problem_id: String) -> CmdResult<ProblemDetail> {
    state.db.detail(&problem_id).map_err(to_msg)
}

#[tauri::command]
pub fn update_problem(
    state: State<'_, AppState>,
    problem_id: String,
    req: UpdateProblemReq,
) -> CmdResult<Problem> {
    state.db.update_problem(&problem_id, req).map_err(to_msg)
}

#[tauri::command]
pub fn delete_problem(state: State<'_, AppState>, problem_id: String) -> CmdResult<()> {
    state.db.delete_problem(&problem_id).map_err(to_msg)
}

// ---------------------------------------------------------------------------
// Solution
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn save_solution(
    state: State<'_, AppState>,
    problem_id: String,
    language: String,
    code: String,
) -> CmdResult<Solution> {
    state
        .db
        .save_solution(&problem_id, &language, &code)
        .map_err(to_msg)
}

// ---------------------------------------------------------------------------
// AI generation
// ---------------------------------------------------------------------------

fn generate_impl(db: &Db, problem_id: &str) -> AppResult<GenerationDraft> {
    let settings = db.get_settings()?;
    let problem = db.get_problem(problem_id)?;

    let (provider, mut warnings) = ai::provider_for(&settings)?;
    let request = AiRequest {
        title: problem.title.clone(),
        statement: problem.statement.clone(),
        constraints: problem.constraints.clone(),
        examples: problem.examples.clone(),
        num_cases: settings.num_test_cases,
    };

    let draft = provider.generate(&request)?;
    warnings.extend(draft.warnings);

    // Validation by execution (DESIGN.md §4.2 step 4). The provider stays
    // sandbox-agnostic; this is the layer that knows about both.
    let resolved = sandbox::resolve(settings.sandbox_mode);
    let limits = settings.limits();
    let verdicts = match draft.reference.as_ref() {
        Some(reference) => judge::validate_cases(
            &resolved.report,
            &*resolved.sandbox,
            reference,
            &draft.cases,
            &limits,
        ),
        None => vec![(Validation::Unvalidated, None); draft.cases.len()],
    };

    let mut cases = draft.cases;
    for (case, (validation, reason)) in cases.iter_mut().zip(verdicts) {
        case.validation = validation;
        case.invalid_reason = reason;
    }

    if !resolved.report.can_execute {
        warnings.push(format!(
            "Cases could not be checked by running the reference solution: {}",
            resolved.report.detail
        ));
    }
    let invalid = cases
        .iter()
        .filter(|c| c.validation == Validation::Invalid)
        .count();
    if invalid > 0 {
        warnings.push(format!(
            "{invalid} case(s) disagree with the reference solution. They are excluded by default — review them before saving."
        ));
    }

    Ok(GenerationDraft {
        provider: provider.label(),
        constraints_summary: draft.constraints_summary,
        reference_solution: draft.reference,
        cases,
        warnings,
    })
}

#[tauri::command]
pub async fn generate_cases(
    state: State<'_, AppState>,
    problem_id: String,
) -> CmdResult<GenerationDraft> {
    let db = state.db.clone();
    blocking!(generate_impl(&db, &problem_id).map_err(to_msg))
}

#[tauri::command]
pub fn save_test_cases(
    state: State<'_, AppState>,
    problem_id: String,
    source: String,
    cases: Vec<SaveCaseInput>,
    reference: Option<ReferenceSolution>,
) -> CmdResult<Vec<TestCase>> {
    let db = &state.db;
    if let Some(reference) = reference {
        db.save_reference(&problem_id, &reference).map_err(to_msg)?;
    }
    db.replace_cases(&problem_id, &source, &cases).map_err(to_msg)
}

// ---------------------------------------------------------------------------
// Judging
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn run_solution(
    state: State<'_, AppState>,
    problem_id: String,
) -> CmdResult<RunSummary> {
    let db = state.db.clone();
    blocking!(judge::run_solution(&db, &problem_id).map_err(to_msg))
}

#[tauri::command]
pub async fn verify_outputs(
    state: State<'_, AppState>,
    problem_id: String,
    outputs: Vec<String>,
) -> CmdResult<RunSummary> {
    let db = state.db.clone();
    blocking!(judge::verify_outputs(&db, &problem_id, &outputs).map_err(to_msg))
}

#[tauri::command]
pub fn export_case_inputs(state: State<'_, AppState>, problem_id: String) -> CmdResult<String> {
    judge::export_case_inputs(&state.db, &problem_id).map_err(to_msg)
}

#[tauri::command]
pub fn list_runs(state: State<'_, AppState>, problem_id: String) -> CmdResult<Vec<RunSummary>> {
    state.db.list_runs(&problem_id, 20).map_err(to_msg)
}

// ---------------------------------------------------------------------------
// Detection & settings
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn detect_sandbox(state: State<'_, AppState>) -> CmdResult<SandboxReport> {
    let db = state.db.clone();
    blocking!((|| -> AppResult<SandboxReport> {
        let settings = db.get_settings()?;
        Ok(sandbox::resolve(settings.sandbox_mode).report)
    })()
    .map_err(to_msg))
}

fn check_ai_impl(db: &Db) -> AppResult<AiReport> {
    let settings = db.get_settings()?;
    if settings.ai_mode == AiMode::Mock {
        return Ok(AiReport {
            provider: "mock".into(),
            ok: true,
            detail: "Mock provider selected: always available, works offline, but its cases are \
                     for a built-in demo problem."
                .into(),
            models: Vec::new(),
        });
    }

    let provider = ai::ollama::OllamaProvider::new(&settings.ai_endpoint, &settings.ai_model);
    match provider.ping() {
        Ok(models) => Ok(AiReport {
            provider: provider.label(),
            ok: true,
            detail: format!(
                "Ollama reachable at {} ({} model(s) available).",
                settings.ai_endpoint,
                models.len()
            ),
            models,
        }),
        Err(e) => {
            let fallback = if settings.ai_mode == AiMode::Auto {
                " In auto mode, generation will fall back to the mock provider."
            } else {
                ""
            };
            Ok(AiReport {
                provider: provider.label(),
                ok: false,
                detail: format!("Cannot reach {}: {e}.{fallback}", settings.ai_endpoint),
                models: Vec::new(),
            })
        }
    }
}

#[tauri::command]
pub async fn check_ai(state: State<'_, AppState>) -> CmdResult<AiReport> {
    let db = state.db.clone();
    blocking!(check_ai_impl(&db).map_err(to_msg))
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<AppSettings> {
    state.db.get_settings().map_err(to_msg)
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> CmdResult<AppSettings> {
    state.db.save_settings(&settings).map_err(to_msg)
}
