use crate::bms_parse;
use crate::db::Database;
use crate::logging::JsonlLogger;
use crate::song_norm;
use chrono::Utc;
use rayon::prelude::*;
use rusqlite::{params, OptionalExtension, Statement};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

const CHART_EXTS: [&str; 4] = ["bms", "bme", "bml", "pms"];

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub root_id: i64,
    pub package_count: usize,
    pub chart_count: usize,
    pub parsed_count: usize,
}

#[derive(Debug)]
struct ChartFile {
    package_rel: String,
    rel_path: String,
    full_path: PathBuf,
    ext: String,
    file_size: i64,
    mtime: i64,
}

#[derive(Debug)]
struct PackageRow {
    rel_path: String,
    mtime: i64,
    total_size: i64,
    file_count: i64,
    chart_count: i64,
}

#[derive(Debug)]
struct ParsedChartRow {
    chart_id: i64,
    title: Option<String>,
    subtitle: Option<String>,
    artist: Option<String>,
    subartist: Option<String>,
    genre: Option<String>,
    playlevel: Option<String>,
    bpm: Option<String>,
    total: Option<String>,
    player: Option<String>,
    wav_count: i64,
    bmp_count: i64,
    wav_list_json: String,
    bmp_list_json: String,
    file_md5: String,
    bms_norm_hash: Option<String>,
}

type PackageChartMetaRow = (i64, Option<String>, Option<String>);
type PackageChartMetaMap = HashMap<i64, Vec<PackageChartMetaRow>>;

pub fn run_scan(
    db: Database,
    logger: Arc<JsonlLogger>,
    app_handle: Option<AppHandle>,
    root_id: i64,
) -> anyhow::Result<ScanResult> {
    let start = Instant::now();
    logger.log("scan_start", map_with_int("root_id", root_id));
    if let Some(app) = &app_handle {
        let _ = app.emit("scan_progress", json!({"phase":"start","root_id":root_id}));
    }

    let mut conn = db.connect()?;
    let root_path: String = conn
        .query_row(
            "SELECT path FROM roots WHERE id=?1 AND enabled=1",
            [root_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("root not found or disabled: {}", root_id))?;

    let root = PathBuf::from(root_path);
    let (packages, charts) = collect_packages(&root)?;

    let tx = conn.transaction()?;
    tx.execute("DELETE FROM packages WHERE root_id=?1", params![root_id])?;

    let mut inserted_charts = 0usize;
    let mut charts_by_package: HashMap<&str, Vec<&ChartFile>> = HashMap::new();
    for chart in &charts {
        charts_by_package
            .entry(chart.package_rel.as_str())
            .or_default()
            .push(chart);
    }

    let mut pkg_stmt = tx.prepare_cached(
        "INSERT INTO packages(root_id, path, mtime, total_size, file_count, chart_count, last_scanned_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut chart_stmt = tx.prepare_cached(
        "INSERT INTO charts(package_id, rel_path, ext, file_size, mtime) VALUES(?1, ?2, ?3, ?4, ?5)",
    )?;

    for pkg in &packages {
        pkg_stmt.execute(params![
            root_id,
            pkg.rel_path,
            pkg.mtime,
            pkg.total_size,
            pkg.file_count,
            pkg.chart_count,
            Utc::now().to_rfc3339(),
        ])?;
        let package_id = tx.last_insert_rowid();
        if let Some(pkg_charts) = charts_by_package.get(pkg.rel_path.as_str()) {
            bulk_insert_charts(&mut chart_stmt, package_id, pkg_charts.iter().copied())?;
            inserted_charts += pkg_charts.len();
        }
    }
    drop(chart_stmt);
    drop(pkg_stmt);
    tx.commit()?;
    logger.log("db_commit", {
        let mut m = Map::new();
        m.insert("event_scope".into(), json!("scan_structure"));
        m.insert("root_id".into(), json!(root_id));
        m.insert("package_count".into(), json!(packages.len()));
        m.insert("chart_count".into(), json!(inserted_charts));
        m
    });
    if let Some(app) = &app_handle {
        let _ = app.emit(
            "scan_progress",
            json!({"phase":"structure_done","root_id":root_id,"packages":packages.len(),"charts":charts.len()}),
        );
    }

    let mut parsed = 0usize;
    {
        let mut conn = db.connect()?;
        let charts_to_parse: Vec<(i64, String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT c.id, p.path, c.rel_path
                 FROM charts c
                 JOIN packages p ON p.id=c.package_id
                 WHERE p.root_id=?1",
            )?;
            let rows = stmt.query_map([root_id], |row| {
                let id: i64 = row.get(0)?;
                let pkg_path: String = row.get(1)?;
                let rel_path: String = row.get(2)?;
                Ok((id, pkg_path, rel_path))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let parse_start = Instant::now();
        let total = charts_to_parse.len();
        let counter = AtomicUsize::new(0);
        let parsed_rows: Vec<ParsedChartRow> = charts_to_parse
            .into_par_iter()
            .filter_map(|(chart_id, pkg_path, rel_path)| {
                let file_path = root.join(pkg_path).join(rel_path);
                if !file_path.exists() {
                    return None;
                }
                let parsed_bms = bms_parse::parse_chart(&file_path).ok()?;
                let wav_list_json = serde_json::to_string(&parsed_bms.wav_list).ok()?;
                let bmp_list_json = serde_json::to_string(&parsed_bms.bmp_list).ok()?;
                let current = counter.fetch_add(1, Ordering::Relaxed) + 1;
                if (current).is_multiple_of(50) {
                    if let Some(app) = &app_handle {
                        let _ = app.emit(
                            "scan_progress",
                            json!({"phase":"parsing","root_id":root_id,"done":current,"total":total}),
                        );
                    }
                }
                Some(ParsedChartRow {
                    chart_id,
                    title: parsed_bms.title,
                    subtitle: parsed_bms.subtitle,
                    artist: parsed_bms.artist,
                    subartist: parsed_bms.subartist,
                    genre: parsed_bms.genre,
                    playlevel: parsed_bms.playlevel,
                    bpm: parsed_bms.bpm,
                    total: parsed_bms.total,
                    player: parsed_bms.player,
                    wav_count: parsed_bms.wav_list.len() as i64,
                    bmp_count: parsed_bms.bmp_list.len() as i64,
                    wav_list_json,
                    bmp_list_json,
                    file_md5: parsed_bms.file_md5,
                    bms_norm_hash: parsed_bms.bms_norm_hash,
                })
            })
            .collect();

        let tx = conn.transaction()?;
        let mut update_stmt = tx.prepare_cached(
            "UPDATE charts
             SET title=?2, subtitle=?3, artist=?4, subartist=?5, genre=?6, playlevel=?7,
                 bpm=?8, total=?9, player=?10,
                 wav_count=?11, bmp_count=?12, wav_list_json=?13, bmp_list_json=?14,
                 file_md5=?15, bms_norm_hash=?16
             WHERE id=?1",
        )?;
        for row in parsed_rows {
            update_stmt.execute(params![
                row.chart_id,
                row.title,
                row.subtitle,
                row.artist,
                row.subartist,
                row.genre,
                row.playlevel,
                row.bpm,
                row.total,
                row.player,
                row.wav_count,
                row.bmp_count,
                row.wav_list_json,
                row.bmp_list_json,
                row.file_md5,
                row.bms_norm_hash,
            ])?;
            parsed += 1;
        }
        drop(update_stmt);
        tx.commit()?;
        logger.log("db_commit", {
            let mut m = Map::new();
            m.insert("event_scope".into(), json!("scan_parse"));
            m.insert("root_id".into(), json!(root_id));
            m.insert("parsed_count".into(), json!(parsed));
            m.insert(
                "parse_duration_ms".into(),
                json!(parse_start.elapsed().as_millis()),
            );
            m
        });
        if let Some(app) = &app_handle {
            let _ = app.emit(
                "scan_progress",
                json!({"phase":"parse_done","root_id":root_id,"parsed":parsed,"total":total}),
            );
        }
    }
    {
        let mut conn = db.connect()?;
        let linked = refresh_song_links(&mut conn, root_id)?;
        logger.log("db_commit", {
            let mut m = Map::new();
            m.insert("event_scope".into(), json!("song_links_refresh"));
            m.insert("root_id".into(), json!(root_id));
            m.insert("linked_charts".into(), json!(linked));
            m
        });
    }
    {
        let conn = db.connect()?;
        conn.execute("DELETE FROM charts_fts", [])?;
        conn.execute(
            "INSERT INTO charts_fts(rowid, title, artist, path)
             SELECT c.id, COALESCE(c.title,''), COALESCE(c.artist,''), p.path || '/' || c.rel_path
             FROM charts c
             JOIN packages p ON p.id=c.package_id",
            [],
        )?;
    }

    logger.log("scan_done", {
        let mut m = Map::new();
        m.insert("root_id".to_string(), json!(root_id));
        m.insert("package_count".to_string(), json!(packages.len()));
        m.insert("chart_count".to_string(), json!(inserted_charts));
        m.insert("parsed_count".to_string(), json!(parsed));
        m.insert(
            "duration_ms".to_string(),
            json!(start.elapsed().as_millis()),
        );
        m
    });

    Ok(ScanResult {
        root_id,
        package_count: packages.len(),
        chart_count: inserted_charts,
        parsed_count: parsed,
    })
}

fn collect_packages(root: &Path) -> anyhow::Result<(Vec<PackageRow>, Vec<ChartFile>)> {
    let mut packages = Vec::new();
    let mut charts = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
    {
        let dir = entry.path();
        let mut has_chart = false;
        let mut file_count = 0i64;
        let mut total_size = 0i64;
        let mut dir_charts = Vec::new();

        let rd = match fs::read_dir(dir) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for child in rd.filter_map(Result::ok) {
            let meta = match child.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if !meta.is_file() {
                continue;
            }
            file_count += 1;
            total_size += meta.len() as i64;

            let child_path = child.path();
            let ext = child_path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if CHART_EXTS.contains(&ext.as_str()) {
                has_chart = true;
                let rel_pkg = relative(root, dir)?;
                let rel_chart = relative(dir, &child_path)?;
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or_default();
                dir_charts.push(ChartFile {
                    package_rel: rel_pkg,
                    rel_path: rel_chart,
                    full_path: child_path,
                    ext,
                    file_size: meta.len() as i64,
                    mtime,
                });
            }
        }

        if has_chart {
            let rel_pkg = relative(root, dir)?;
            let dir_meta = fs::metadata(dir)?;
            let mtime = dir_meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or_default();
            packages.push(PackageRow {
                rel_path: rel_pkg,
                mtime,
                total_size,
                file_count,
                chart_count: dir_charts.len() as i64,
            });
            charts.extend(dir_charts);
        }
    }

    packages.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    charts.sort_by(|a, b| a.full_path.cmp(&b.full_path));
    Ok((packages, charts))
}

fn relative(base: &Path, path: &Path) -> anyhow::Result<String> {
    let rel = path.strip_prefix(base)?;
    let s = rel.to_string_lossy().replace('\\', "/");
    if s.is_empty() {
        Ok(".".to_string())
    } else {
        Ok(s)
    }
}

fn map_with_int(key: &str, v: i64) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(key.to_string(), json!(v));
    m
}

fn bulk_insert_charts<'a, I>(
    stmt: &mut Statement<'_>,
    package_id: i64,
    charts: I,
) -> anyhow::Result<()>
where
    I: IntoIterator<Item = &'a ChartFile>,
{
    for chart in charts {
        stmt.execute(params![
            package_id,
            chart.rel_path,
            chart.ext,
            chart.file_size,
            chart.mtime
        ])?;
    }
    Ok(())
}

fn refresh_song_links(conn: &mut rusqlite::Connection, root_id: i64) -> anyhow::Result<usize> {
    let mut package_rows: PackageChartMetaMap = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT p.id, c.id, c.title, c.artist
             FROM packages p
             LEFT JOIN charts c ON c.package_id=p.id
             WHERE p.root_id=?1
             ORDER BY p.id, c.id",
        )?;
        let mut rows = stmt.query([root_id])?;
        while let Some(row) = rows.next()? {
            let package_id: i64 = row.get(0)?;
            let chart_id: Option<i64> = row.get(1)?;
            let title: Option<String> = row.get(2)?;
            let artist: Option<String> = row.get(3)?;
            if let Some(chart_id) = chart_id {
                package_rows
                    .entry(package_id)
                    .or_default()
                    .push((chart_id, title, artist));
            }
        }
    }

    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM song_links
         WHERE chart_id IN (
           SELECT c.id
           FROM charts c
           JOIN packages p ON p.id=c.package_id
           WHERE p.root_id=?1
         )",
        [root_id],
    )?;

    let mut linked_charts = 0usize;
    for charts in package_rows.values() {
        let rows: Vec<(Option<String>, Option<String>)> = charts
            .iter()
            .map(|(_, title, artist)| (title.clone(), artist.clone()))
            .collect();
        let (mut canonical_title, mut canonical_artist) = song_norm::estimate_package_meta(&rows);
        if canonical_title.is_none() || canonical_artist.is_none() {
            for (_, title, artist) in charts {
                let t = song_norm::normalize_title(title.as_deref().unwrap_or_default());
                let a = song_norm::normalize_artist(artist.as_deref().unwrap_or_default());
                if !t.is_empty() && !a.is_empty() {
                    canonical_title = Some(t);
                    canonical_artist = Some(a);
                    break;
                }
            }
        }

        let (Some(canonical_title), Some(canonical_artist)) = (canonical_title, canonical_artist)
        else {
            continue;
        };

        let song_id: i64 = if let Some(id) = tx
            .query_row(
                "SELECT id FROM songs WHERE canonical_title=?1 AND canonical_artist=?2 LIMIT 1",
                params![canonical_title, canonical_artist],
                |r| r.get(0),
            )
            .optional()?
        {
            id
        } else {
            tx.execute(
                "INSERT INTO songs(canonical_title, canonical_artist) VALUES(?1, ?2)",
                params![canonical_title, canonical_artist],
            )?;
            tx.last_insert_rowid()
        };

        let confidence = if charts.len() >= 2 { 0.95_f64 } else { 0.7_f64 };
        for (chart_id, _, _) in charts {
            tx.execute(
                "INSERT OR REPLACE INTO song_links(song_id, chart_id, confidence, user_confirmed)
                 VALUES(?1, ?2, ?3, 0)",
                params![song_id, chart_id, confidence],
            )?;
            linked_charts += 1;
        }
    }

    tx.execute(
        "DELETE FROM songs
         WHERE id NOT IN (SELECT DISTINCT song_id FROM song_links)",
        [],
    )?;
    tx.commit()?;
    Ok(linked_charts)
}
