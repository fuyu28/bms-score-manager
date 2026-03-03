use std::cmp::Ordering;

pub fn normalize_song_key(title: Option<&str>, artist: Option<&str>) -> Option<String> {
    let t = normalize_title(title.unwrap_or_default());
    let a = normalize_artist(artist.unwrap_or_default());
    if t.is_empty() || a.is_empty() {
        None
    } else {
        Some(format!("{t}|{a}"))
    }
}

pub fn estimate_package_meta(
    rows: &[(Option<String>, Option<String>)],
) -> (Option<String>, Option<String>) {
    let titles: Vec<String> = rows
        .iter()
        .filter_map(|(t, _)| t.as_ref())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let artists: Vec<String> = rows
        .iter()
        .filter_map(|(_, a)| a.as_ref())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let title_prefix = common_prefix_with_one_skip(&titles);
    let artist_prefix = common_prefix_with_one_skip(&artists);

    let title = normalize_title(&title_prefix);
    let artist = normalize_artist(&artist_prefix);

    let title = if title.is_empty() { None } else { Some(title) };
    let artist = if artist.is_empty() {
        None
    } else {
        Some(artist)
    };
    (title, artist)
}

pub fn normalize_title(s: &str) -> String {
    let mut text = s.trim().to_string();
    if let Some((head, _)) = text.split_once("  ") {
        text = head.to_string();
    }

    text = strip_trailing_delim_block(&text, '-');
    text = strip_trailing_delim_block(&text, '～');
    text = strip_trailing_delim_block(&text, '"');
    text = strip_trailing_pairs(text, '(', ')');
    text = strip_trailing_pairs(text, '（', '）');
    text = strip_trailing_pairs(text, '[', ']');
    text = strip_trailing_pairs(text, '<', '>');

    normalize_token(&text)
}

pub fn normalize_artist(s: &str) -> String {
    let mut text = s.trim().to_string();
    text = cut_at_case_insensitive(&text, "obj");
    text = cut_at_case_insensitive(&text, "note:");
    text = cut_at_case_insensitive(&text, "差分");

    let first = text
        .split('/')
        .map(str::trim)
        .find(|x| !x.is_empty())
        .unwrap_or_default();
    normalize_token(first)
}

fn normalize_token(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn cut_at_case_insensitive(s: &str, needle: &str) -> String {
    let lower = s.to_lowercase();
    let needle_lower = needle.to_lowercase();
    if let Some(idx) = lower.find(&needle_lower) {
        s[..idx].trim().to_string()
    } else {
        s.to_string()
    }
}

fn strip_trailing_delim_block(s: &str, delim: char) -> String {
    let t = s.trim();
    if !t.ends_with(delim) {
        return t.to_string();
    }
    let mut chars = t.chars();
    chars.next_back();
    let trimmed = chars.as_str().trim_end();
    if let Some(idx) = trimmed.rfind(delim) {
        trimmed[..idx].trim_end().to_string()
    } else {
        t.to_string()
    }
}

fn strip_trailing_pairs(mut s: String, open: char, close: char) -> String {
    loop {
        let trimmed = s.trim_end();
        if !trimmed.ends_with(close) {
            return trimmed.to_string();
        }
        match trimmed.rfind(open) {
            Some(idx) => s = trimmed[..idx].trim_end().to_string(),
            None => return trimmed.to_string(),
        }
    }
}

fn common_prefix_with_one_skip(values: &[String]) -> String {
    if values.is_empty() {
        return String::new();
    }
    if values.len() == 1 {
        return values[0].clone();
    }

    let mut best = common_prefix(values);
    if values.len() >= 3 {
        for i in 0..values.len() {
            let subset: Vec<String> = values
                .iter()
                .enumerate()
                .filter_map(|(idx, s)| if idx == i { None } else { Some(s.clone()) })
                .collect();
            let cand = common_prefix(&subset);
            if cand.chars().count().cmp(&best.chars().count()) == Ordering::Greater {
                best = cand;
            }
        }
    }

    trim_incomplete_chunks(best)
}

fn common_prefix(values: &[String]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let mut prefix = values[0].clone();
    for v in values.iter().skip(1) {
        let mut out = String::new();
        for (a, b) in prefix.chars().zip(v.chars()) {
            if a == b {
                out.push(a);
            } else {
                break;
            }
        }
        prefix = out;
        if prefix.is_empty() {
            break;
        }
    }
    prefix
}

fn trim_incomplete_chunks(mut s: String) -> String {
    s = s.trim_end_matches([' ', '/', '-', '_']).to_string();

    if s.matches('(').count() > s.matches(')').count() {
        if let Some(idx) = s.rfind('(') {
            s.truncate(idx);
        }
    }
    if s.matches('（').count() > s.matches('）').count() {
        if let Some(idx) = s.rfind('（') {
            s.truncate(idx);
        }
    }
    if s.matches('[').count() > s.matches(']').count() {
        if let Some(idx) = s.rfind('[') {
            s.truncate(idx);
        }
    }

    s.trim().to_string()
}
