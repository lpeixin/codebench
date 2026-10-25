//! local-leet core.
//!
//! Layering (DESIGN.md §2):
//! - [`db`] owns SQL, [`models`] owns shapes, [`error`] owns failure text.
//! - [`ai`] talks HTTP and knows nothing about sandboxes.
//! - [`sandbox`] owns every process that executes code.
//! - [`judge`] orchestrates a run and compares outputs.
//! - [`commands`] is the only module that knows about Tauri.

pub mod ai;
pub mod commands;
pub mod db;
pub mod error;
pub mod judge;
pub mod models;
pub mod sandbox;

use tauri::Manager;

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("local-leet.db");
            log::info!("opening database at {}", db_path.display());
            let db = db::Db::open(&db_path)?;
            app.manage(commands::AppState { db });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_problems,
            commands::create_problem,
            commands::get_problem,
            commands::update_problem,
            commands::delete_problem,
            commands::save_solution,
            commands::generate_cases,
            commands::save_test_cases,
            commands::run_solution,
            commands::verify_outputs,
            commands::export_case_inputs,
            commands::list_runs,
            commands::detect_sandbox,
            commands::check_ai,
            commands::get_settings,
            commands::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running local-leet");
}
