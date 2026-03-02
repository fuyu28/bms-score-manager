use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TablePattern {
    A,
    B,
    C,
    D,
}

pub fn classify(
    header: &Value,
    data: &Value,
    data_final_url: &str,
    data_url: &str,
) -> TablePattern {
    let has_course = header.get("course").is_some();
    let has_grade = header.get("grade").is_some();
    let has_ext =
        header.get("mode").is_some() || header.get("attr").is_some() || header.get("tag").is_some();
    let likely_api = data_url.contains("script")
        || data_final_url.contains("script")
        || data_final_url.contains("exec")
        || data_final_url.contains("api");

    if likely_api {
        return TablePattern::D;
    }
    if has_grade || has_ext {
        return TablePattern::C;
    }
    if has_course && !has_grade {
        return TablePattern::B;
    }
    if data.is_array() {
        return TablePattern::A;
    }
    TablePattern::D
}
