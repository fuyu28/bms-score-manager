use super::classify::{classify, TablePattern};
use super::fetch::fetch_table_payload;
use super::{parse_a, parse_b, parse_c, parse_d, ParsedTable};
use crate::db::Database;
use crate::logging::JsonlLogger;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map};
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub source_id: i64,
    pub table_id: i64,
    pub pattern: String,
    pub entry_count: usize,
    pub group_count: usize,
    pub skipped_by_hash: bool,
}

pub async fn import_table(
    db: Database,
    logger: Arc<JsonlLogger>,
    client: reqwest::Client,
    source_id: i64,
) -> anyhow::Result<ImportResult> {
    let mut conn = db.connect()?;
    let input_url: String = conn
        .query_row(
            "SELECT input_url FROM table_sources WHERE id=?1 AND enabled=1",
            [source_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("table source not found or disabled: {}", source_id))?;

    conn.execute(
        "UPDATE table_sources SET last_fetch_at=?2, last_error=NULL WHERE id=?1",
        params![source_id, Utc::now().to_rfc3339()],
    )?;

    logger.log("table_fetch_start", {
        let mut m = Map::new();
        m.insert("source_id".into(), json!(source_id));
        m.insert("url".into(), json!(input_url));
        m
    });

    let fetched = match fetch_table_payload(&client, &input_url).await {
        Ok(v) => v,
        Err(e) => {
            let mut conn = db.connect()?;
            conn.execute(
                "UPDATE table_sources SET last_error=?2 WHERE id=?1",
                params![source_id, format!("{}", e)],
            )?;
            return Err(e);
        }
    };
    logger.log("table_fetch", {
        let mut m = Map::new();
        m.insert("source_id".into(), json!(source_id));
        m.insert("url".into(), json!(input_url));
        m.insert("final_url".into(), json!(fetched.data_final_url));
        m
    });

    let pattern = classify(
        &fetched.header_json,
        &fetched.data_json,
        &fetched.data_final_url,
        &fetched.data_url,
    );
    let parsed = parse_by_pattern(pattern, &fetched.header_json, &fetched.data_json);

    let tx = conn.transaction()?;

    let existing: Option<(i64, String, String)> = tx
        .query_row(
            "SELECT id, header_hash, data_hash FROM tables WHERE source_id=?1",
            [source_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;

    if let Some((table_id, header_hash, data_hash)) = existing {
        if header_hash == fetched.header_hash && data_hash == fetched.data_hash {
            tx.execute(
                "UPDATE table_sources SET last_success_at=?2, last_error=NULL WHERE id=?1",
                params![source_id, Utc::now().to_rfc3339()],
            )?;
            tx.commit()?;
            logger.log("db_commit", {
                let mut m = Map::new();
                m.insert("event_scope".into(), json!("table_skip_hash"));
                m.insert("source_id".into(), json!(source_id));
                m.insert("table_id".into(), json!(table_id));
                m
            });
            logger.log("table_fetch_skipped", {
                let mut m = Map::new();
                m.insert("source_id".into(), json!(source_id));
                m.insert("table_id".into(), json!(table_id));
                m
            });
            return Ok(ImportResult {
                source_id,
                table_id,
                pattern: pattern_name(pattern).to_string(),
                entry_count: 0,
                group_count: 0,
                skipped_by_hash: true,
            });
        }
    }

    tx.execute(
        "INSERT INTO tables(
           source_id, page_url_resolved, header_url, data_url, data_final_url,
           name, symbol, tag, mode, level_order_json, attr_json,
           header_raw, data_raw, header_hash, data_hash, updated_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
         ON CONFLICT(source_id) DO UPDATE SET
           page_url_resolved=excluded.page_url_resolved,
           header_url=excluded.header_url,
           data_url=excluded.data_url,
           data_final_url=excluded.data_final_url,
           name=excluded.name,
           symbol=excluded.symbol,
           tag=excluded.tag,
           mode=excluded.mode,
           level_order_json=excluded.level_order_json,
           attr_json=excluded.attr_json,
           header_raw=excluded.header_raw,
           data_raw=excluded.data_raw,
           header_hash=excluded.header_hash,
           data_hash=excluded.data_hash,
           updated_at=excluded.updated_at",
        params![
            source_id,
            fetched.page_url_resolved,
            fetched.header_url,
            fetched.data_url,
            fetched.data_final_url,
            parsed.name,
            parsed.symbol,
            parsed.tag,
            parsed.mode,
            parsed.level_order_json,
            parsed.attr_json,
            fetched.header_raw,
            fetched.data_raw,
            fetched.header_hash,
            fetched.data_hash,
            Utc::now().to_rfc3339(),
        ],
    )?;

    let table_id: i64 = tx.query_row(
        "SELECT id FROM tables WHERE source_id=?1",
        [source_id],
        |r| r.get(0),
    )?;
    tx.execute("DELETE FROM table_entries WHERE table_id=?1", [table_id])?;
    tx.execute("DELETE FROM table_groups WHERE table_id=?1", [table_id])?;

    for e in &parsed.entries {
        tx.execute(
            "INSERT INTO table_entries(
              table_id, md5, sha256, level_text, title, artist, charter, url, url_diff, comment, raw_json
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                table_id,
                e.md5,
                e.sha256,
                e.level_text,
                e.title,
                e.artist,
                e.charter,
                e.url,
                e.url_diff,
                e.comment,
                e.raw_json,
            ],
        )?;
    }

    for g in &parsed.groups {
        tx.execute(
            "INSERT INTO table_groups(
              table_id, group_type, group_set_index, name, style, constraints_json, trophies_json, raw_json
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                table_id,
                g.group_type,
                g.group_set_index,
                g.name,
                g.style,
                g.constraints_json,
                g.trophies_json,
                g.raw_json,
            ],
        )?;
        let group_id = tx.last_insert_rowid();
        for item in &g.items {
            tx.execute(
                "INSERT INTO table_group_items(group_id, md5, title_hint) VALUES (?1,?2,?3)",
                params![group_id, item.md5, item.title_hint],
            )?;
        }
    }

    tx.execute(
        "UPDATE table_sources SET last_success_at=?2, last_error=NULL WHERE id=?1",
        params![source_id, Utc::now().to_rfc3339()],
    )?;

    tx.commit()?;
    logger.log("db_commit", {
        let mut m = Map::new();
        m.insert("event_scope".into(), json!("table_upsert"));
        m.insert("source_id".into(), json!(source_id));
        m.insert("table_id".into(), json!(table_id));
        m.insert("entry_count".into(), json!(parsed.entries.len()));
        m.insert("group_count".into(), json!(parsed.groups.len()));
        m
    });

    logger.log("table_parse_done", {
        let mut m = Map::new();
        m.insert("source_id".into(), json!(source_id));
        m.insert("table_id".into(), json!(table_id));
        m.insert("pattern".into(), json!(pattern_name(pattern)));
        m.insert("entry_count".into(), json!(parsed.entries.len()));
        m.insert("group_count".into(), json!(parsed.groups.len()));
        m
    });

    Ok(ImportResult {
        source_id,
        table_id,
        pattern: pattern_name(pattern).to_string(),
        entry_count: parsed.entries.len(),
        group_count: parsed.groups.len(),
        skipped_by_hash: false,
    })
}

fn parse_by_pattern(
    pattern: TablePattern,
    header: &serde_json::Value,
    data: &serde_json::Value,
) -> ParsedTable {
    match pattern {
        TablePattern::A => parse_a::parse(header, data),
        TablePattern::B => parse_b::parse(header, data),
        TablePattern::C => parse_c::parse(header, data),
        TablePattern::D => parse_d::parse(header, data),
    }
}

fn pattern_name(p: TablePattern) -> &'static str {
    match p {
        TablePattern::A => "A",
        TablePattern::B => "B",
        TablePattern::C => "C",
        TablePattern::D => "D",
    }
}
