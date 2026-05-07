use crate::types::VlmRoleReport;
use std::io::{Read, Write};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct LfmVlmClient {
    base_url: String,
    model: String,
}

impl LfmVlmClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
        }
    }

    pub fn run_role(
        &self,
        role: &str,
        system_prompt: &str,
        user_text: &str,
        image_data_urls: &[String],
    ) -> Result<VlmRoleReport, String> {
        if image_data_urls.is_empty() {
            return Err(format!(
                "LFM VLM role '{role}' requires at least one SimSat RGB image"
            ));
        }
        let started = Instant::now();
        let body = chat_request_json(&self.model, system_prompt, user_text, image_data_urls);
        let raw = http_post_json(&self.base_url, "/v1/chat/completions", &body)?;
        let content = extract_content(&raw)?;
        if !content.trim_start().starts_with('{') || !content.trim_end().ends_with('}') {
            return Err(format!(
                "LFM VLM role '{role}' did not return a JSON object"
            ));
        }
        Ok(VlmRoleReport {
            role: role.to_string(),
            model: self.model.clone(),
            prompt_hash: stable_hash(format!("{system_prompt}\n{user_text}").as_bytes()),
            input_image_hashes: image_data_urls
                .iter()
                .map(|image| stable_hash(image.as_bytes()))
                .collect(),
            response_json: content,
            latency_ms: started.elapsed().as_millis(),
        })
    }
}

pub fn png_data_url(base64_png: &str) -> String {
    format!("data:image/png;base64,{base64_png}")
}

fn chat_request_json(
    model: &str,
    system_prompt: &str,
    user_text: &str,
    image_data_urls: &[String],
) -> String {
    let mut user_content = format!(
        "{{\"type\":\"text\",\"text\":\"{}\"}}",
        escape_json(user_text)
    );
    for image in image_data_urls {
        user_content.push_str(&format!(
            ",{{\"type\":\"image_url\",\"image_url\":{{\"url\":\"{}\"}}}}",
            escape_json(image)
        ));
    }
    format!(
        "{{\"model\":\"{}\",\"messages\":[{{\"role\":\"system\",\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}]}},{{\"role\":\"user\",\"content\":[{}]}}],\"response_format\":{{\"type\":\"json_object\"}},\"temperature\":0.0,\"max_tokens\":800,\"seed\":42}}",
        escape_json(model),
        escape_json(system_prompt),
        user_content
    )
}

fn http_post_json(base_url: &str, path: &str, body: &str) -> Result<String, String> {
    if !base_url.starts_with("http://") {
        return Err(
            "only http:// LFM VLM URLs are supported by the dependency-free client".to_string(),
        );
    }
    let without_scheme = &base_url[7..];
    let (host_port, prefix) = match without_scheme.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (without_scheme, String::new()),
    };
    let (host, port) = match host_port.split_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|e| e.to_string())?),
        None => (host_port, 80),
    };
    let mut stream = std::net::TcpStream::connect((host, port))
        .map_err(|e| format!("connecting to {host}:{port}: {e}"))?;
    let request_path = format!("{prefix}{path}");
    let request = format!(
        "POST {request_path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| e.to_string())?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "invalid HTTP response".to_string())?;
    if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
        return Err(format!("LFM VLM returned non-200 response: {head}"));
    }
    Ok(body.to_string())
}

fn extract_content(body: &str) -> Result<String, String> {
    let key = "\"content\"";
    let pos = body
        .find(key)
        .ok_or_else(|| "LFM VLM response omitted choices[0].message.content".to_string())?;
    let after_colon = body[pos + key.len()..]
        .find(':')
        .map(|i| pos + key.len() + i + 1)
        .ok_or_else(|| "content key missing colon".to_string())?;
    parse_json_string(&body[after_colon..])
}

fn parse_json_string(input: &str) -> Result<String, String> {
    let rest = input.trim_start();
    if !rest.starts_with('"') {
        return Err("content is not a JSON string".to_string());
    }
    let mut escaped = false;
    let mut out = String::new();
    for ch in rest[1..].chars() {
        if escaped {
            match ch {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                other => out.push(other),
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Ok(out);
        } else {
            out.push(ch);
        }
    }
    Err("unterminated content string".to_string())
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn stable_hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 1469598103934665603;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}
