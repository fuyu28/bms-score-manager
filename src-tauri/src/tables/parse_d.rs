use crate::tables::{lower_md5, string_field, to_raw_json, ParsedTable, TableEntryParsed};
use serde_json::Value;

pub fn parse(header: &Value, data: &Value) -> ParsedTable {
    let entries = data
        .as_array()
        .into_iter()
        .flat_map(|arr| arr.iter())
        .filter_map(|v| {
            let md5 = lower_md5(v)?;
            let artist = string_field(v, "artist").or_else(|| string_field(v, "song_artist"));
            Some(TableEntryParsed {
                md5,
                sha256: string_field(v, "sha256"),
                level_text: string_field(v, "level"),
                title: string_field(v, "title"),
                artist,
                charter: string_field(v, "charter"),
                url: string_field(v, "url"),
                url_diff: string_field(v, "url_diff"),
                comment: string_field(v, "comment"),
                raw_json: to_raw_json(v),
            })
        })
        .collect::<Vec<_>>();

    ParsedTable {
        name: string_field(header, "name"),
        symbol: string_field(header, "symbol"),
        tag: string_field(header, "tag"),
        mode: string_field(header, "mode"),
        level_order_json: header.get("level_order").map(to_raw_json),
        attr_json: header.get("attr").map(to_raw_json),
        entries,
        groups: Vec::new(),
    }
}
