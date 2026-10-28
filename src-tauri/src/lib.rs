pub mod db;
pub mod equity;
pub mod import_worker;
pub mod importer;
pub mod model;
pub mod parser;
pub mod preflop;
pub mod rake;

use model::{Filter, Hand, HandRow, ImportReport, Overview};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};

#[derive(Clone)]
struct Database {
    path: PathBuf,
    importing: Arc<Mutex<()>>,
}

#[tauri::command]
async fn import_hands(
    app: tauri::AppHandle,
    state: tauri::State<'_, Database>,
    paths: Vec<String>,
    hero: String,
) -> Result<ImportReport, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = state
            .importing
            .try_lock()
            .map_err(|_| "已有导入任务正在运行")?;
        let mut conn = db::open(&state.path)?;
        importer::import_paths(&mut conn, &paths, &hero, |report| {
            let _ = app.emit("import-progress", report);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_overview(
    state: tauri::State<'_, Database>,
    filter: Filter,
) -> Result<Overview, String> {
    let path = state.path.clone();
    tauri::async_runtime::spawn_blocking(move || db::overview(&db::open(&path)?, filter))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_hands(
    state: tauri::State<'_, Database>,
    filter: Filter,
    offset: usize,
) -> Result<Vec<HandRow>, String> {
    let path = state.path.clone();
    tauri::async_runtime::spawn_blocking(move || db::list(&db::open(&path)?, filter, offset))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_hand(state: tauri::State<'_, Database>, id: String) -> Result<Hand, String> {
    let path = state.path.clone();
    tauri::async_runtime::spawn_blocking(move || db::detail(&db::open(&path)?, &id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_imports(state: tauri::State<'_, Database>) -> Result<Vec<serde_json::Value>, String> {
    let path = state.path.clone();
    tauri::async_runtime::spawn_blocking(move || db::history(&db::open(&path)?))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn database_path(state: tauri::State<'_, Database>) -> String {
    state.path.to_string_lossy().to_string()
}

/// Shared application setup; smoke tests supply an isolated database.
pub fn builder(test_database: Option<PathBuf>) -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let path = match test_database {
                Some(path) => path,
                None => {
                    let folder = app.path().app_data_dir()?;
                    std::fs::create_dir_all(&folder)?;
                    folder.join("tracker.sqlite3")
                }
            };
            db::open(&path).map_err(std::io::Error::other)?;
            app.manage(Database {
                path,
                importing: Arc::new(Mutex::new(())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            import_hands,
            get_overview,
            get_hands,
            get_hand,
            get_imports,
            database_path
        ])
}

pub fn run() {
    builder(None).run(context()).expect("无法启动 Pain2Note");
}

pub fn context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}
