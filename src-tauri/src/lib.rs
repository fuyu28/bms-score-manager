mod bms_parse;
mod db;
mod dedupe;
mod logging;
mod scan;
mod tables;

use chrono::Utc;
use db::Database;
use logging::JsonlLogger;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

#[derive(Clone)]
struct AppState {
    db: Database,
    logger: Arc<JsonlLogger>,
    http: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct RootRow {
    id: i64,
    path: String,
    enabled: bool,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct TableSourceRow {
    id: i64,
    input_url: String,
    enabled: bool,
    last_fetch_at: Option<String>,
    last_success_at: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct ChartSearchRow {
    chart_id: i64,
    root_id: i64,
    package_id: i64,
    title: Option<String>,
    artist: Option<String>,
    rel_path: String,
    file_md5: Option<String>,
    root_path: String,
    package_path: String,
}

#[derive(Debug, Serialize)]
struct OwnershipSummary {
    table_id: i64,
    total_entries: i64,
    owned_entries: i64,
    missing_entries: i64,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    query: String,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[tauri::command]
async fn add_root(state: tauri::State<'_, AppState>, path: String) -> Result<i64, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<i64> {
        let conn = db.connect()?;
        let path_for_insert = path.clone();
        conn.execute(
            "INSERT INTO roots(path, enabled, created_at) VALUES(?1, 1, ?2)
             ON CONFLICT(path) DO UPDATE SET enabled=1",
            params![path_for_insert, Utc::now().to_rfc3339()],
        )?;
        let id = conn.query_row("SELECT id FROM roots WHERE path=?1", [path], |r| r.get(0))?;
        Ok(id)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_roots(state: tauri::State<'_, AppState>) -> Result<Vec<RootRow>, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Vec<RootRow>> {
        let conn = db.connect()?;
        let mut stmt =
            conn.prepare("SELECT id, path, enabled, created_at FROM roots ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(RootRow {
                id: r.get(0)?,
                path: r.get(1)?,
                enabled: r.get::<_, i64>(2)? != 0,
                created_at: r.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn scan_root(
    state: tauri::State<'_, AppState>,
    root_id: i64,
    app: tauri::AppHandle,
) -> Result<scan::ScanResult, String> {
    let db = state.db.clone();
    let logger = state.logger.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || scan::run_scan(db, logger, Some(handle), root_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn register_table_source(
    state: tauri::State<'_, AppState>,
    input_url: String,
) -> Result<i64, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<i64> {
        let conn = db.connect()?;
        conn.execute(
            "INSERT INTO table_sources(input_url, enabled) VALUES(?1, 1)
             ON CONFLICT(input_url) DO UPDATE SET enabled=1",
            [input_url.clone()],
        )?;
        let id = conn.query_row(
            "SELECT id FROM table_sources WHERE input_url=?1",
            [input_url],
            |r| r.get(0),
        )?;
        Ok(id)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_table_sources(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<TableSourceRow>, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Vec<TableSourceRow>> {
        let conn = db.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, input_url, enabled, last_fetch_at, last_success_at, last_error
             FROM table_sources
             ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(TableSourceRow {
                id: r.get(0)?,
                input_url: r.get(1)?,
                enabled: r.get::<_, i64>(2)? != 0,
                last_fetch_at: r.get(3)?,
                last_success_at: r.get(4)?,
                last_error: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn import_table_source(
    state: tauri::State<'_, AppState>,
    source_id: i64,
) -> Result<tables::upsert::ImportResult, String> {
    tables::upsert::import_table(
        state.db.clone(),
        state.logger.clone(),
        state.http.clone(),
        source_id,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ownership_summary(
    state: tauri::State<'_, AppState>,
    table_id: i64,
) -> Result<OwnershipSummary, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<OwnershipSummary> {
        let conn = db.connect()?;
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM table_entries WHERE table_id=?1",
            [table_id],
            |r| r.get(0),
        )?;
        let owned: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT te.id)
             FROM table_entries te
             JOIN charts c ON c.file_md5=te.md5
             WHERE te.table_id=?1",
            [table_id],
            |r| r.get(0),
        )?;
        Ok(OwnershipSummary {
            table_id,
            total_entries: total,
            owned_entries: owned,
            missing_entries: total - owned,
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_charts(
    state: tauri::State<'_, AppState>,
    params: SearchParams,
) -> Result<Vec<ChartSearchRow>, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Vec<ChartSearchRow>> {
        let conn = db.connect()?;
        let limit = params.limit.unwrap_or(100).max(1);
        let offset = params.offset.unwrap_or(0).max(0);
        let q = if params.query.trim().is_empty() {
            "*".to_string()
        } else {
            params.query
        };
        let mut stmt = conn.prepare(
            "SELECT c.id, p.root_id, p.id, c.title, c.artist, c.rel_path, c.file_md5, r.path, p.path
             FROM charts c
             JOIN charts_fts ON charts_fts.rowid=c.id
             JOIN packages p ON p.id=c.package_id
             JOIN roots r ON r.id=p.root_id
             WHERE charts_fts MATCH ?1
             ORDER BY c.id
             LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![q, limit, offset], |r| {
            Ok(ChartSearchRow {
                chart_id: r.get(0)?,
                root_id: r.get(1)?,
                package_id: r.get(2)?,
                title: r.get(3)?,
                artist: r.get(4)?,
                rel_path: r.get(5)?,
                file_md5: r.get(6)?,
                root_path: r.get(7)?,
                package_path: r.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn detect_duplicates(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<dedupe::DuplicateGroup>, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || dedupe::detect_duplicates(db))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn preview_dedupe(
    state: tauri::State<'_, AppState>,
    req: dedupe::DedupePreviewRequest,
) -> Result<dedupe::DedupePreview, String> {
    let db = state.db.clone();
    let logger = state.logger.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let preview = dedupe::preview_merge(db, req)?;
        logger.log("dedupe_preview", {
            let mut m = serde_json::Map::new();
            m.insert("keep_chart_id".into(), json!(preview.keep_chart_id));
            m.insert("remove_count".into(), json!(preview.remove_count));
            m.insert("cross_root".into(), json!(preview.cross_root));
            m
        });
        Ok::<_, anyhow::Error>(preview)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn execute_dedupe(
    state: tauri::State<'_, AppState>,
    req: dedupe::DedupeExecuteRequest,
) -> Result<dedupe::DedupeExecuteResult, String> {
    let db = state.db.clone();
    let logger = state.logger.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = dedupe::execute_merge(db.clone(), logger.clone(), req)?;
        for root_id in &result.rescanned_root_ids {
            let _ = scan::run_scan(db.clone(), logger.clone(), None, *root_id);
        }
        Ok::<_, anyhow::Error>(result)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir().unwrap_or_else(|_| {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(".bms-score-manager")
            });
            std::fs::create_dir_all(&app_data)?;

            let db_path = app_data.join("bms-score-manager.sqlite3");
            let log_path = app_data.join("logs").join("events.jsonl");

            let db = Database::new(db_path);
            db.init()?;

            let logger = Arc::new(JsonlLogger::new(log_path)?);
            logger.log("app_start", {
                let mut m = serde_json::Map::new();
                m.insert("app_data_dir".into(), json!(app_data));
                m
            });

            let http = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()?;

            app.manage(AppState { db, logger, http });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_root,
            list_roots,
            scan_root,
            register_table_source,
            list_table_sources,
            import_table_source,
            ownership_summary,
            search_charts,
            detect_duplicates,
            preview_dedupe,
            execute_dedupe,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
