use crate::db::Database;
use crate::logging::JsonlLogger;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct DuplicateChart {
    pub chart_id: i64,
    pub root_id: i64,
    pub root_path: String,
    pub package_path: String,
    pub rel_path: String,
    pub full_path: String,
    pub file_md5: String,
    pub title: Option<String>,
    pub artist: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DuplicateGroup {
    pub key: String,
    pub kind: String,
    pub charts: Vec<DuplicateChart>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DedupePreviewRequest {
    pub keep_chart_id: i64,
    pub remove_chart_ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct DedupeOperationPreview {
    pub source_path: String,
    pub backup_path: String,
    pub backup_conflict: bool,
}

#[derive(Debug, Serialize)]
pub struct DedupePreview {
    pub keep_chart_id: i64,
    pub remove_count: usize,
    pub cross_root: bool,
    pub targets: Vec<String>,
    pub operations: Vec<DedupeOperationPreview>,
    pub confirmation_phrase: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DedupeExecuteRequest {
    pub keep_chart_id: i64,
    pub remove_chart_ids: Vec<i64>,
    pub allow_cross_root: bool,
    pub confirmation_text: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DedupeExecuteResult {
    pub removed: usize,
    pub backed_up: usize,
    pub rescanned_root_ids: Vec<i64>,
}

pub fn detect_duplicates(db: Database) -> anyhow::Result<Vec<DuplicateGroup>> {
    let conn = db.connect()?;
    let mut groups = Vec::new();

    let mut stmt = conn.prepare(
        "SELECT c.file_md5,
                c.id, p.root_id, r.path, p.path, c.rel_path, c.title, c.artist
         FROM charts c
         JOIN packages p ON p.id=c.package_id
         JOIN roots r ON r.id=p.root_id
         WHERE c.file_md5 IS NOT NULL AND c.file_md5<>''
         ORDER BY c.file_md5, c.id",
    )?;

    let rows = stmt.query_map([], |r| {
        let file_md5: String = r.get(0)?;
        Ok((
            file_md5,
            DuplicateChart {
                chart_id: r.get(1)?,
                root_id: r.get(2)?,
                root_path: r.get(3)?,
                package_path: r.get(4)?,
                rel_path: r.get(5)?,
                full_path: String::new(),
                file_md5: String::new(),
                title: r.get(6)?,
                artist: r.get(7)?,
            },
        ))
    })?;

    let mut map: HashMap<String, Vec<DuplicateChart>> = HashMap::new();
    for row in rows {
        let (md5, mut c) = row?;
        c.file_md5 = md5.clone();
        c.full_path = format!("{}/{}/{}", c.root_path, c.package_path, c.rel_path);
        map.entry(md5).or_default().push(c);
    }

    for (md5, charts) in map {
        if charts.len() >= 2 {
            groups.push(DuplicateGroup {
                key: md5,
                kind: "file_md5".to_string(),
                charts,
            });
        }
    }

    Ok(groups)
}

pub fn preview_merge(db: Database, req: DedupePreviewRequest) -> anyhow::Result<DedupePreview> {
    let conn = db.connect()?;
    let (keep_root, keep_root_path, keep_package_path): (i64, String, String) = conn.query_row(
        "SELECT p.root_id, r.path, p.path
         FROM charts c JOIN packages p ON p.id=c.package_id JOIN roots r ON r.id=p.root_id
         WHERE c.id=?1",
        [req.keep_chart_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    let keep_package_abs = PathBuf::from(&keep_root_path).join(&keep_package_path);

    let mut targets = Vec::new();
    let mut operations = Vec::new();
    let mut cross_root = false;
    for id in &req.remove_chart_ids {
        let row: (i64, String, String, String) = conn.query_row(
            "SELECT p.root_id, r.path, p.path, c.rel_path
             FROM charts c JOIN packages p ON p.id=c.package_id JOIN roots r ON r.id=p.root_id
             WHERE c.id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        if row.0 != keep_root {
            cross_root = true;
        }

        let source = PathBuf::from(&row.1).join(&row.2).join(&row.3);
        let backup = backup_path(&keep_package_abs, *id, &row.3);

        targets.push(path_display(&source));
        operations.push(DedupeOperationPreview {
            source_path: path_display(&source),
            backup_path: path_display(&backup),
            backup_conflict: backup.exists(),
        });
    }

    Ok(DedupePreview {
        keep_chart_id: req.keep_chart_id,
        remove_count: req.remove_chart_ids.len(),
        cross_root,
        targets,
        operations,
        confirmation_phrase: confirmation_phrase(req.keep_chart_id, &req.remove_chart_ids),
    })
}

pub fn execute_merge(
    db: Database,
    logger: Arc<JsonlLogger>,
    req: DedupeExecuteRequest,
) -> anyhow::Result<DedupeExecuteResult> {
    let conn = db.connect()?;
    let (keep_root, keep_root_path, keep_package_path): (i64, String, String) = conn.query_row(
        "SELECT p.root_id, r.path, p.path
         FROM charts c JOIN packages p ON p.id=c.package_id JOIN roots r ON r.id=p.root_id
         WHERE c.id=?1",
        [req.keep_chart_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    let required_phrase = confirmation_phrase(req.keep_chart_id, &req.remove_chart_ids);
    if req.confirmation_text.as_deref() != Some(required_phrase.as_str()) {
        return Err(anyhow::anyhow!(
            "confirmation mismatch: previewの確認フレーズを入力してください"
        ));
    }

    let keep_package_abs = PathBuf::from(&keep_root_path).join(&keep_package_path);

    let mut removed = 0usize;
    let mut backed_up = 0usize;
    let mut rescanned_root_ids = vec![keep_root];

    for id in &req.remove_chart_ids {
        let (root_id, root_path, package_path, rel_path): (i64, String, String, String) = conn
            .query_row(
                "SELECT p.root_id, r.path, p.path, c.rel_path
                 FROM charts c JOIN packages p ON p.id=c.package_id JOIN roots r ON r.id=p.root_id
                 WHERE c.id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;

        if root_id != keep_root && !req.allow_cross_root {
            return Err(anyhow::anyhow!(
                "cross-root deletion is disabled: keep_root={}, target_root={}",
                keep_root,
                root_id
            ));
        }
        if !rescanned_root_ids.contains(&root_id) {
            rescanned_root_ids.push(root_id);
        }

        let full_path = PathBuf::from(root_path).join(&package_path).join(&rel_path);
        if !full_path.exists() {
            conn.execute("DELETE FROM charts WHERE id=?1", params![id])?;
            continue;
        }

        let backup = backup_path(&keep_package_abs, *id, &rel_path);
        copy_with_parent(&full_path, &backup)?;
        backed_up += 1;

        if !backup.exists() {
            return Err(anyhow::anyhow!(
                "backup copy failed: {}",
                path_display(&backup)
            ));
        }

        trash::delete(&full_path)?;
        conn.execute("DELETE FROM charts WHERE id=?1", params![id])?;
        removed += 1;
    }

    logger.log("dedupe_execute", {
        let mut m = Map::new();
        m.insert("keep_chart_id".into(), json!(req.keep_chart_id));
        m.insert("removed".into(), json!(removed));
        m.insert("backed_up".into(), json!(backed_up));
        m.insert("allow_cross_root".into(), json!(req.allow_cross_root));
        m
    });

    Ok(DedupeExecuteResult {
        removed,
        backed_up,
        rescanned_root_ids,
    })
}

fn backup_path(keep_package_abs: &Path, remove_chart_id: i64, rel_path: &str) -> PathBuf {
    let file_name = Path::new(rel_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("chart.bms");
    keep_package_abs
        .join(".dedupe_backup")
        .join(format!("{}_{}", remove_chart_id, file_name))
}

fn copy_with_parent(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst)?;
    Ok(())
}

fn path_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn confirmation_phrase(keep_chart_id: i64, remove_chart_ids: &[i64]) -> String {
    let mut ids = remove_chart_ids.to_vec();
    ids.sort_unstable();
    let mut hasher = Sha256::new();
    hasher.update(format!("{}:{:?}", keep_chart_id, ids));
    let digest = format!("{:x}", hasher.finalize());
    format!("CONFIRM-{}", &digest[..12])
}
