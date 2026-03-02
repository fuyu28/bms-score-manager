use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedBms {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub artist: Option<String>,
    pub subartist: Option<String>,
    pub genre: Option<String>,
    pub playlevel: Option<String>,
    pub bpm: Option<String>,
    pub total: Option<String>,
    pub player: Option<String>,
    pub wav_list: Vec<String>,
    pub bmp_list: Vec<String>,
    pub file_md5: String,
    pub bms_norm_hash: Option<String>,
}

pub fn parse_chart(path: &Path) -> anyhow::Result<ParsedBms> {
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);

    let mut title = None;
    let mut subtitle = None;
    let mut artist = None;
    let mut subartist = None;
    let mut genre = None;
    let mut playlevel = None;
    let mut bpm = None;
    let mut total = None;
    let mut player = None;
    let mut wav = BTreeSet::new();
    let mut bmp = BTreeSet::new();
    let mut normalized = Vec::new();

    for line in text.lines() {
        let l = line.trim();
        if !l.starts_with('#') {
            continue;
        }
        normalized.push(l.to_ascii_lowercase());

        if let Some((k, v)) = split_header(l) {
            let key = k.to_ascii_uppercase();
            let value = v.trim().to_string();
            match key.as_str() {
                "TITLE" => title = some_if_not_empty(value),
                "SUBTITLE" => subtitle = some_if_not_empty(value),
                "ARTIST" => artist = some_if_not_empty(value),
                "SUBARTIST" => subartist = some_if_not_empty(value),
                "GENRE" => genre = some_if_not_empty(value),
                "PLAYLEVEL" => playlevel = some_if_not_empty(value),
                "BPM" => bpm = some_if_not_empty(value),
                "TOTAL" => total = some_if_not_empty(value),
                "PLAYER" => player = some_if_not_empty(value),
                _ => {
                    if key.starts_with("WAV") {
                        if !value.is_empty() {
                            wav.insert(value);
                        }
                    } else if key.starts_with("BMP") && !value.is_empty() {
                        bmp.insert(value);
                    }
                }
            }
        }
    }

    let file_md5 = format!("{:x}", md5::compute(&bytes));
    let norm_joined = normalized.join("\n");
    let bms_norm_hash = if norm_joined.is_empty() {
        None
    } else {
        Some(format!("{:x}", md5::compute(norm_joined.as_bytes())))
    };

    Ok(ParsedBms {
        title,
        subtitle,
        artist,
        subartist,
        genre,
        playlevel,
        bpm,
        total,
        player,
        wav_list: wav.into_iter().collect(),
        bmp_list: bmp.into_iter().collect(),
        file_md5,
        bms_norm_hash,
    })
}

fn split_header(line: &str) -> Option<(&str, &str)> {
    let content = line.strip_prefix('#')?;
    let mut idx = None;
    for (i, c) in content.char_indices() {
        if c.is_ascii_whitespace() || c == ':' {
            idx = Some(i);
            break;
        }
    }
    let i = idx?;
    let (k, rest) = content.split_at(i);
    Some((k, rest.trim_start_matches([' ', '\t', ':'])))
}

fn some_if_not_empty(v: String) -> Option<String> {
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}
