use crate::tables::{
    lower_md5, string_field, to_raw_json, GroupItemParsed, ParsedTable, TableEntryParsed,
    TableGroupParsed,
};
use serde_json::Value;

pub fn parse(header: &Value, data: &Value) -> ParsedTable {
    let entries = data
        .as_array()
        .into_iter()
        .flat_map(|arr| arr.iter())
        .filter_map(|v| {
            let md5 = lower_md5(v)?;
            Some(TableEntryParsed {
                md5,
                sha256: string_field(v, "sha256"),
                level_text: string_field(v, "level"),
                title: string_field(v, "title"),
                artist: string_field(v, "artist"),
                charter: string_field(v, "charter"),
                url: string_field(v, "url"),
                url_diff: string_field(v, "url_diff"),
                comment: string_field(v, "comment"),
                raw_json: to_raw_json(v),
            })
        })
        .collect::<Vec<_>>();

    let mut groups = Vec::new();
    if let Some(course_arr) = header.get("course").and_then(|v| v.as_array()) {
        for (idx, group) in course_arr.iter().enumerate() {
            let mut items = Vec::new();
            if let Some(md5s) = group.get("md5").and_then(|v| v.as_array()) {
                for m in md5s {
                    if let Some(md5) = m
                        .as_str()
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty())
                    {
                        items.push(GroupItemParsed {
                            md5,
                            title_hint: None,
                        });
                    }
                }
            }
            if let Some(charts) = group.get("charts").and_then(|v| v.as_array()) {
                for c in charts {
                    if let Some(md5) = lower_md5(c) {
                        items.push(GroupItemParsed {
                            md5,
                            title_hint: string_field(c, "title"),
                        });
                    }
                }
            }

            groups.push(TableGroupParsed {
                group_type: "course".to_string(),
                group_set_index: idx as i64,
                name: string_field(group, "name"),
                style: string_field(group, "style"),
                constraints_json: group
                    .get("constraint")
                    .or_else(|| group.get("constraints"))
                    .map(to_raw_json),
                trophies_json: group
                    .get("trophy")
                    .or_else(|| group.get("trophies"))
                    .map(to_raw_json),
                raw_json: to_raw_json(group),
                items,
            });
        }
    }

    ParsedTable {
        name: string_field(header, "name"),
        symbol: string_field(header, "symbol"),
        tag: string_field(header, "tag"),
        mode: string_field(header, "mode"),
        level_order_json: header.get("level_order").map(to_raw_json),
        attr_json: header.get("attr").map(to_raw_json),
        entries,
        groups,
    }
}
