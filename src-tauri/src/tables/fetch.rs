use reqwest::header::ACCEPT;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedTable {
    pub page_url_resolved: String,
    pub header_url: String,
    pub data_url: String,
    pub data_final_url: String,
    pub header_raw: String,
    pub data_raw: String,
    pub header_hash: String,
    pub data_hash: String,
    pub header_json: Value,
    pub data_json: Value,
}

pub async fn fetch_table_payload(
    client: &reqwest::Client,
    input_url: &str,
) -> anyhow::Result<FetchedTable> {
    let input = validate_http_url(input_url, "input_url")?;
    let page_res = client.get(input).send().await?;
    let page_final = page_res.url().to_string();
    validate_http_url(&page_final, "page_url_resolved")?;
    let page_html = page_res.text().await?;

    let header_ref = extract_meta_bmstable(&page_html)?;
    let page_url = Url::parse(&page_final)?;
    let header_url = page_url.join(&header_ref)?.to_string();
    validate_http_url(&header_url, "header_url")?;

    let (header_final, header_raw, header_json) = fetch_json(client, &header_url).await?;

    let data_ref = header_json
        .get("data_url")
        .or_else(|| header_json.get("data"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("header json does not contain data_url/data"))?;

    let header_url_resolved = Url::parse(&header_final)?;
    let data_url = header_url_resolved.join(data_ref)?.to_string();
    validate_http_url(&data_url, "data_url")?;
    let (data_final_url, data_raw, data_json) = fetch_json(client, &data_url).await?;

    let header_hash = sha256_hex(&header_raw);
    let data_hash = sha256_hex(&data_raw);

    Ok(FetchedTable {
        page_url_resolved: page_final,
        header_url: header_final,
        data_url,
        data_final_url,
        header_raw,
        data_raw,
        header_hash,
        data_hash,
        header_json,
        data_json,
    })
}

async fn fetch_json(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<(String, String, Value)> {
    validate_http_url(url, "fetch_url")?;
    let resp = client
        .get(url)
        .header(ACCEPT, "application/json")
        .send()
        .await?;

    let final_url = resp.url().to_string();
    validate_http_url(&final_url, "resolved_fetch_url")?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "http status error: {} ({})",
            status,
            final_url
        ));
    }

    let body = resp.text().await?;
    let json: Value = serde_json::from_str(&body)
        .map_err(|e| anyhow::anyhow!("json parse failed: {} ({})", e, final_url))?;

    Ok((final_url, body, json))
}

fn extract_meta_bmstable(html: &str) -> anyhow::Result<String> {
    let doc = Html::parse_document(html);
    let selector = Selector::parse("meta[name='bmstable']")
        .map_err(|e| anyhow::anyhow!("selector parse failed: {}", e))?;

    let content = doc
        .select(&selector)
        .find_map(|el| el.value().attr("content"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("meta name=bmstable not found"))?;

    Ok(content)
}

fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn validate_http_url(url: &str, field: &str) -> anyhow::Result<Url> {
    let parsed = Url::parse(url)
        .map_err(|e| anyhow::anyhow!("invalid url for {}: {} ({})", field, e, url))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        scheme => Err(anyhow::anyhow!(
            "unsupported url scheme for {}: {} ({})",
            field,
            scheme,
            url
        )),
    }
}
