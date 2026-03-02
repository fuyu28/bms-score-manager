use crate::bms_parse;
use crate::db::Database;
use crate::logging::JsonlLogger;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
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

pub fn run_scan(
    db: Database,
    logger: Arc<JsonlLogger>,
    root_id: i64,
) -> anyhow::Result<ScanResult> {
    let start = Instant::now();
    logger.log("scan_start", map_with_int("root_id", root_id));

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
    for pkg in &packages {
        tx.execute(
            "INSERT INTO packages(root_id, path, mtime, total_size, file_count, chart_count, last_scanned_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                root_id,
                pkg.rel_path,
                pkg.mtime,
                pkg.total_size,
                pkg.file_count,
                pkg.chart_count,
                Utc::now().to_rfc3339(),
            ],
        )?;
        let package_id = tx.last_insert_rowid();
        for chart in charts.iter().filter(|c| c.package_rel == pkg.rel_path) {
            tx.execute(
                "INSERT INTO charts(package_id, rel_path, ext, file_size, mtime) VALUES(?1, ?2, ?3, ?4, ?5)",
                params![package_id, chart.rel_path, chart.ext, chart.file_size, chart.mtime],
            )?;
            inserted_charts += 1;
        }
    }
    tx.commit()?;
    logger.log("db_commit", {
        let mut m = Map::new();
        m.insert("event_scope".into(), json!("scan_structure"));
        m.insert("root_id".into(), json!(root_id));
        m.insert("package_count".into(), json!(packages.len()));
        m.insert("chart_count".into(), json!(inserted_charts));
        m
    });

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

        let tx = conn.transaction()?;
        for (chart_id, pkg_path, rel_path) in charts_to_parse {
            let file_path = root.join(pkg_path).join(rel_path);
            if !file_path.exists() {
                continue;
            }
            if let Ok(parsed_bms) = bms_parse::parse_chart(&file_path) {
                tx.execute(
                    "UPDATE charts
                     SET title=?2, subtitle=?3, artist=?4, subartist=?5, genre=?6, playlevel=?7,
                         bpm=?8, total=?9, player=?10,
                         wav_count=?11, bmp_count=?12, wav_list_json=?13, bmp_list_json=?14,
                         file_md5=?15, bms_norm_hash=?16
                     WHERE id=?1",
                    params![
                        chart_id,
                        parsed_bms.title,
                        parsed_bms.subtitle,
                        parsed_bms.artist,
                        parsed_bms.subartist,
                        parsed_bms.genre,
                        parsed_bms.playlevel,
                        parsed_bms.bpm,
                        parsed_bms.total,
                        parsed_bms.player,
                        parsed_bms.wav_list.len() as i64,
                        parsed_bms.bmp_list.len() as i64,
                        serde_json::to_string(&parsed_bms.wav_list)?,
                        serde_json::to_string(&parsed_bms.bmp_list)?,
                        parsed_bms.file_md5,
                        parsed_bms.bms_norm_hash,
                    ],
                )?;
                parsed += 1;
            }
        }
        tx.commit()?;
        logger.log("db_commit", {
            let mut m = Map::new();
            m.insert("event_scope".into(), json!("scan_parse"));
            m.insert("root_id".into(), json!(root_id));
            m.insert("parsed_count".into(), json!(parsed));
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
