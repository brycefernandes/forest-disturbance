use crate::types::{GeoPoint, Observation, REQUIRED_BANDS};
use std::collections::BTreeMap;
use std::io::{Read, Write};

#[derive(Debug, Clone)]
pub struct SimSatClient {
    base_url: String,
}

#[derive(Debug)]
struct HttpResponse {
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

impl SimSatClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    pub fn get_image_sentinel(
        &self,
        point: GeoPoint,
        timestamp: &str,
        size_km: f64,
        window_seconds: u64,
    ) -> Result<Observation, String> {
        let spectral_bands = REQUIRED_BANDS.join(",");
        let array_path = format!(
            "/data/image/sentinel?lon={}&lat={}&timestamp={}&spectral_bands={}&size_km={}&return_type=array&window_seconds={}",
            point.lon, point.lat, timestamp, spectral_bands, size_km, window_seconds
        );
        let array_response = http_get(&self.base_url, &array_path)?;
        let array_body = String::from_utf8(array_response.body)
            .map_err(|e| format!("SimSat array response was not UTF-8 JSON: {e}"))?;
        let mut obs = parse_simsat_json(&array_body)?;

        let rgb_path = format!(
            "/data/image/sentinel?lon={}&lat={}&timestamp={}&spectral_bands=red,green,blue&size_km={}&return_type=png&window_seconds={}",
            point.lon, point.lat, timestamp, size_km, window_seconds
        );
        let rgb_response = http_get(&self.base_url, &rgb_path)?;
        if let Some(metadata) = rgb_response.headers.get("sentinel_metadata")
            && (metadata.contains("\"image_available\": false")
                || metadata.contains("'image_available': False"))
        {
            return Err("SimSat RGB endpoint reported image_available=false".to_string());
        }
        obs.rgb_png_base64 = base64_encode(&rgb_response.body);
        obs.validate()?;
        Ok(obs)
    }
}

pub fn parse_simsat_json(body: &str) -> Result<Observation, String> {
    if body.contains("\"sentinel_metadata\"") && body.contains("\"image\"") {
        parse_dphi_simsat_array_json(body)
    } else {
        parse_legacy_array_json(body)
    }
}

fn parse_dphi_simsat_array_json(body: &str) -> Result<Observation, String> {
    let metadata = extract_raw_value(body, "sentinel_metadata")?;
    if metadata.contains("\"image_available\": false")
        || metadata.contains("'image_available': False")
    {
        return Err("SimSat reported image_available=false".to_string());
    }
    let image = extract_raw_value(body, "image")?;
    let image_meta = extract_raw_value(&image, "metadata")?;
    let shape = extract_usize_array(&image_meta, "shape")?;
    if shape.len() != 3 {
        return Err(format!(
            "SimSat array shape must be [height,width,bands], got {shape:?}"
        ));
    }
    let height = shape[0];
    let width = shape[1];
    let channels = shape[2];
    if channels != REQUIRED_BANDS.len() {
        return Err(format!(
            "SimSat returned {channels} channels; expected {} requested bands",
            REQUIRED_BANDS.len()
        ));
    }
    let dtype = extract_string(&image_meta, "dtype").unwrap_or_else(|_| "float32".to_string());
    let data_b64 = extract_string(&image, "data")
        .or_else(|_| extract_string(&image, "array"))
        .or_else(|_| extract_string(&image, "bytes"))?;
    let raw = base64_decode(&data_b64)?;
    let interleaved = decode_numeric_array(&raw, &dtype)?;
    let expected = width * height * channels;
    if interleaved.len() != expected {
        return Err(format!(
            "SimSat decoded array has {} values; expected {expected} for shape {shape:?}",
            interleaved.len()
        ));
    }

    let mut bands = BTreeMap::new();
    for (band_idx, band_name) in REQUIRED_BANDS.iter().enumerate() {
        let mut values = Vec::with_capacity(width * height);
        for pixel_idx in 0..(width * height) {
            values.push(interleaved[pixel_idx * channels + band_idx]);
        }
        bands.insert((*band_name).to_string(), normalize_reflectance(values));
    }

    let cloud_cover = normalize_cloud_cover(extract_f32(&metadata, "cloud_cover").unwrap_or(0.0));
    let footprint_geojson = footprint_geojson(&metadata);
    let obs = Observation {
        source: extract_string(&metadata, "source").unwrap_or_else(|_| "simsat".to_string()),
        image_datetime: extract_string(&metadata, "datetime")
            .or_else(|_| extract_string(&metadata, "timestamp"))?,
        width,
        height,
        bands,
        cloud_cover,
        footprint_geojson,
        rgb_png_base64: String::new(),
        content_hash: stable_hash(body.as_bytes()),
    };
    obs.validate().or_else(|err| {
        if err.contains("rgb_png_base64") {
            Ok(())
        } else {
            Err(err)
        }
    })?;
    Ok(obs)
}

fn parse_legacy_array_json(body: &str) -> Result<Observation, String> {
    let width = extract_usize(body, "width")?;
    let height = extract_usize(body, "height")?;
    let mut bands = BTreeMap::new();
    for band in REQUIRED_BANDS {
        bands.insert(band.to_string(), extract_f32_array(body, band)?);
    }
    let obs = Observation {
        source: extract_string(body, "source").unwrap_or_else(|_| "simsat".to_string()),
        image_datetime: extract_string(body, "image_datetime")
            .or_else(|_| extract_string(body, "datetime"))?,
        width,
        height,
        bands,
        cloud_cover: normalize_cloud_cover(extract_f32(body, "cloud_cover").unwrap_or(0.0)),
        footprint_geojson: extract_raw_value(body, "footprint_geojson")
            .unwrap_or_else(|_| "{}".to_string()),
        rgb_png_base64: extract_string(body, "rgb_png_base64").unwrap_or_default(),
        content_hash: stable_hash(body.as_bytes()),
    };
    obs.validate()?;
    Ok(obs)
}

fn http_get(base_url: &str, path_and_query: &str) -> Result<HttpResponse, String> {
    if !base_url.starts_with("http://") {
        return Err(
            "only http:// SimSat URLs are supported by the dependency-free client".to_string(),
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
    let request_path = format!("{prefix}{path_and_query}");
    let request = format!(
        "GET {request_path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nAccept: */*\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| e.to_string())?;
    let split = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "invalid HTTP response".to_string())?;
    let head = String::from_utf8_lossy(&response[..split]).to_string();
    let body = response[split + 4..].to_vec();
    if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
        return Err(format!("SimSat returned non-200 response: {head}"));
    }
    let mut headers = BTreeMap::new();
    for line in head.lines().skip(1) {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    Ok(HttpResponse { headers, body })
}

fn decode_numeric_array(raw: &[u8], dtype: &str) -> Result<Vec<f32>, String> {
    let dtype = dtype.to_ascii_lowercase();
    if dtype.contains("float32") || dtype == "<f4" || dtype == "f4" {
        if !raw.len().is_multiple_of(4) {
            return Err(format!(
                "float32 byte length is not divisible by 4: {}",
                raw.len()
            ));
        }
        Ok(raw
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect())
    } else if dtype.contains("float64") || dtype == "<f8" || dtype == "f8" {
        if !raw.len().is_multiple_of(8) {
            return Err(format!(
                "float64 byte length is not divisible by 8: {}",
                raw.len()
            ));
        }
        Ok(raw
            .chunks_exact(8)
            .map(|chunk| {
                f64::from_le_bytes([
                    chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
                ]) as f32
            })
            .collect())
    } else if dtype.contains("uint16") || dtype == "<u2" || dtype == "u2" {
        if !raw.len().is_multiple_of(2) {
            return Err(format!(
                "uint16 byte length is not divisible by 2: {}",
                raw.len()
            ));
        }
        Ok(raw
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]) as f32)
            .collect())
    } else if dtype.contains("uint8") || dtype == "u1" {
        Ok(raw.iter().map(|byte| *byte as f32).collect())
    } else {
        Err(format!("unsupported SimSat array dtype '{dtype}'"))
    }
}

fn normalize_reflectance(values: Vec<f32>) -> Vec<f32> {
    let max = values.iter().copied().fold(0.0_f32, f32::max);
    let divisor = if max > 1000.0 {
        10000.0
    } else if max > 1.0 {
        255.0
    } else {
        1.0
    };
    values
        .into_iter()
        .map(|value| (value / divisor).clamp(0.0, 1.0))
        .collect()
}

fn normalize_cloud_cover(value: f32) -> f32 {
    if value > 1.0 {
        (value / 100.0).clamp(0.0, 1.0)
    } else {
        value.clamp(0.0, 1.0)
    }
}

fn footprint_geojson(metadata: &str) -> String {
    if let Ok(values) = extract_f32_array(metadata, "footprint")
        && values.len() == 4
    {
        let lon_min = values[0];
        let lat_min = values[1];
        let lon_max = values[2];
        let lat_max = values[3];
        return format!(
            "{{\"type\":\"Polygon\",\"coordinates\":[[[{lon_min},{lat_min}],[{lon_max},{lat_min}],[{lon_max},{lat_max}],[{lon_min},{lat_max}],[{lon_min},{lat_min}]] ]}}"
        );
    }
    "{}".to_string()
}

fn extract_string(body: &str, key: &str) -> Result<String, String> {
    let needle = format!("\"{key}\"");
    let pos = body
        .find(&needle)
        .ok_or_else(|| format!("missing key {key}"))?;
    let after_colon = body[pos + needle.len()..]
        .find(':')
        .map(|i| pos + needle.len() + i + 1)
        .ok_or_else(|| format!("missing colon for {key}"))?;
    let rest = body[after_colon..].trim_start();
    if !rest.starts_with('"') {
        return Err(format!("key {key} is not a string"));
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
    Err(format!("unterminated string for {key}"))
}

fn extract_usize(body: &str, key: &str) -> Result<usize, String> {
    extract_number_token(body, key)?
        .parse::<usize>()
        .map_err(|e| e.to_string())
}
fn extract_f32(body: &str, key: &str) -> Result<f32, String> {
    extract_number_token(body, key)?
        .parse::<f32>()
        .map_err(|e| e.to_string())
}

fn extract_number_token(body: &str, key: &str) -> Result<String, String> {
    let needle = format!("\"{key}\"");
    let pos = body
        .find(&needle)
        .ok_or_else(|| format!("missing key {key}"))?;
    let after_colon = body[pos + needle.len()..]
        .find(':')
        .map(|i| pos + needle.len() + i + 1)
        .ok_or_else(|| format!("missing colon for {key}"))?;
    let token: String = body[after_colon..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == 'e' || *c == 'E')
        .collect();
    if token.is_empty() {
        Err(format!("missing numeric value for {key}"))
    } else {
        Ok(token)
    }
}

fn extract_usize_array(body: &str, key: &str) -> Result<Vec<usize>, String> {
    let raw = extract_raw_value(body, key)?;
    let inner = raw
        .trim()
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| format!("key {key} is not an array"))?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|part| part.trim().parse::<usize>().map_err(|e| e.to_string()))
        .collect()
}

fn extract_f32_array(body: &str, key: &str) -> Result<Vec<f32>, String> {
    let raw = extract_raw_value(body, key)?;
    let inner = raw
        .trim()
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| format!("key {key} is not an array"))?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|part| part.trim().parse::<f32>().map_err(|e| e.to_string()))
        .collect()
}

fn extract_raw_value(body: &str, key: &str) -> Result<String, String> {
    let needle = format!("\"{key}\"");
    let pos = body
        .find(&needle)
        .ok_or_else(|| format!("missing key {key}"))?;
    let after_colon = body[pos + needle.len()..]
        .find(':')
        .map(|i| pos + needle.len() + i + 1)
        .ok_or_else(|| format!("missing colon for {key}"))?;
    let rest = body[after_colon..].trim_start();
    if rest.starts_with('[') {
        balanced_value(rest, '[', ']')
    } else if rest.starts_with('{') {
        balanced_value(rest, '{', '}')
    } else if rest.starts_with('"') {
        extract_string(body, key).map(|s| format!("\"{s}\""))
    } else {
        Ok(rest
            .chars()
            .take_while(|c| *c != ',' && *c != '}')
            .collect::<String>())
    }
}

fn balanced_value(rest: &str, open: char, close: char) -> Result<String, String> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in rest.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
        } else if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Ok(rest[..=idx].to_string());
            }
        }
    }
    Err("unbalanced JSON value".to_string())
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut values = Vec::new();
    for ch in input.chars().filter(|c| !c.is_whitespace()) {
        let value = match ch {
            'A'..='Z' => ch as u8 - b'A',
            'a'..='z' => ch as u8 - b'a' + 26,
            '0'..='9' => ch as u8 - b'0' + 52,
            '+' => 62,
            '/' => 63,
            '=' => 64,
            _ => return Err(format!("invalid base64 character '{ch}'")),
        };
        values.push(value);
    }
    if values.len() % 4 != 0 {
        return Err("base64 length is not a multiple of 4".to_string());
    }
    let mut out = Vec::new();
    for chunk in values.chunks(4) {
        let c0 = chunk[0];
        let c1 = chunk[1];
        let c2 = chunk[2];
        let c3 = chunk[3];
        if c0 >= 64 || c1 >= 64 {
            return Err("invalid base64 padding position".to_string());
        }
        out.push((c0 << 2) | (c1 >> 4));
        if c2 != 64 {
            out.push(((c1 & 0b0000_1111) << 4) | (c2 >> 2));
        }
        if c3 != 64 {
            out.push(((c2 & 0b0000_0011) << 6) | c3);
        }
    }
    Ok(out)
}

fn stable_hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 1469598103934665603;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dphi_simsat_array_payload() {
        let mut raw = Vec::new();
        let pixels = 25usize;
        for _ in 0..pixels {
            for value in [0.08_f32, 0.12, 0.05, 0.72, 0.22, 0.18] {
                raw.extend_from_slice(&value.to_le_bytes());
            }
        }
        let payload = format!(
            "{{\"sentinel_metadata\":{{\"image_available\":true,\"source\":\"sentinel-2a\",\"cloud_cover\":5.0,\"datetime\":\"2026-03-16T13:53:37Z\",\"footprint\":[1.0,2.0,3.0,4.0]}},\"image\":{{\"data\":\"{}\",\"metadata\":{{\"shape\":[5,5,6],\"dtype\":\"float32\"}}}}}}",
            base64_encode(&raw)
        );
        let obs = parse_simsat_json(&payload).unwrap();
        assert_eq!(obs.width, 5);
        assert_eq!(obs.height, 5);
        assert_eq!(obs.band("nir").unwrap()[0], 0.72);
        assert!((obs.cloud_cover - 0.05).abs() < 0.001);
    }
}
