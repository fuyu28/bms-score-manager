pub mod classify;
pub mod fetch;
pub mod parse_a;
pub mod parse_b;
pub mod parse_c;
pub mod parse_d;
pub mod upsert;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableEntryParsed {
    pub md5: String,
    pub sha256: Option<String>,
    pub level_text: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub charter: Option<String>,
    pub url: Option<String>,
    pub url_diff: Option<String>,
    pub comment: Option<String>,
    pub raw_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupItemParsed {
    pub md5: String,
    pub title_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableGroupParsed {
    pub group_type: String,
    pub group_set_index: i64,
    pub name: Option<String>,
    pub style: Option<String>,
    pub constraints_json: Option<String>,
    pub trophies_json: Option<String>,
    pub raw_json: String,
    pub items: Vec<GroupItemParsed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedTable {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub tag: Option<String>,
    pub mode: Option<String>,
    pub level_order_json: Option<String>,
    pub attr_json: Option<String>,
    pub entries: Vec<TableEntryParsed>,
    pub groups: Vec<TableGroupParsed>,
}

pub fn string_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn lower_md5(v: &Value) -> Option<String> {
    let md5 = v.get("md5")?.as_str()?.trim().to_ascii_lowercase();
    if md5.is_empty() {
        None
    } else {
        Some(md5)
    }
}

pub fn to_raw_json(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string())
}
