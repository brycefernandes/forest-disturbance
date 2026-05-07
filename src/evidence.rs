use crate::pipeline::alert_json;
use crate::types::{Alert, Observation, VlmRoleReport};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn write_packet(
    out_dir: &Path,
    baseline: &Observation,
    current: &Observation,
    alert: &Alert,
) -> Result<PathBuf, String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("creating {}: {e}", out_dir.display()))?;
    fs::write(
        out_dir.join("baseline_observation_metadata.json"),
        observation_json(baseline),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        out_dir.join("current_observation_metadata.json"),
        observation_json(current),
    )
    .map_err(|e| e.to_string())?;
    fs::write(out_dir.join("alert.json"), alert_json(alert)).map_err(|e| e.to_string())?;
    fs::write(
        out_dir.join("feature_summary.json"),
        feature_summary_json(alert),
    )
    .map_err(|e| e.to_string())?;
    fs::create_dir_all(out_dir.join("vlm_reports")).map_err(|e| e.to_string())?;
    for report in &alert.vlm_reports {
        fs::write(
            out_dir
                .join("vlm_reports")
                .join(format!("{}.json", report.role)),
            vlm_report_json(report),
        )
        .map_err(|e| e.to_string())?;
    }
    fs::write(out_dir.join("report.md"), report_markdown(alert)).map_err(|e| e.to_string())?;

    let mut files = BTreeMap::new();
    for path in packet_files(out_dir)? {
        let relative = path
            .strip_prefix(out_dir)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "manifest.json" || relative == "hashes.txt" {
            continue;
        }
        files.insert(relative, hash_file(&path)?);
    }
    let hashes_txt = files
        .iter()
        .map(|(path, hash)| format!("{hash}  {path}\n"))
        .collect::<String>();
    fs::write(out_dir.join("hashes.txt"), hashes_txt).map_err(|e| e.to_string())?;
    files.insert(
        "hashes.txt".to_string(),
        hash_file(&out_dir.join("hashes.txt"))?,
    );
    let manifest_without_hash = manifest_json(alert, &files, "");
    let manifest_hash = stable_hash(manifest_without_hash.as_bytes());
    fs::write(
        out_dir.join("manifest.json"),
        manifest_json(alert, &files, &manifest_hash),
    )
    .map_err(|e| e.to_string())?;
    Ok(out_dir.to_path_buf())
}

pub fn verify_packet(packet_dir: &Path) -> Result<(), String> {
    let manifest =
        fs::read_to_string(packet_dir.join("manifest.json")).map_err(|e| e.to_string())?;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('"') || !trimmed.contains("sha256:") {
            continue;
        }
        let mut parts = trimmed.split('"');
        parts.next();
        let relative = parts.next().unwrap_or_default();
        parts.next();
        let hash_value = parts.next().unwrap_or_default();
        let expected = hash_value.trim_start_matches("sha256:");
        let actual = hash_file(&packet_dir.join(relative))?;
        if actual != expected {
            return Err(format!(
                "hash mismatch for {relative}: expected {expected}, got {actual}"
            ));
        }
    }
    Ok(())
}

fn observation_json(obs: &Observation) -> String {
    format!(
        "{{\n  \"source\": \"{}\",\n  \"image_datetime\": \"{}\",\n  \"width\": {},\n  \"height\": {},\n  \"cloud_cover\": {:.4},\n  \"content_hash\": \"{}\",\n  \"footprint_geojson\": {}\n}}\n",
        escape(&obs.source),
        escape(&obs.image_datetime),
        obs.width,
        obs.height,
        obs.cloud_cover,
        escape(&obs.content_hash),
        obs.footprint_geojson
    )
}

fn feature_summary_json(alert: &Alert) -> String {
    let patch_json = match &alert.primary_patch {
        Some(p) => format!(
            "{{\"pixel_count\":{},\"area_ha\":{:.4},\"mean_delta_ndvi\":{:.6},\"mean_delta_nbr\":{:.6},\"mean_delta_bsi\":{:.6}}}",
            p.pixel_count, p.area_ha, p.mean_delta_ndvi, p.mean_delta_nbr, p.mean_delta_bsi
        ),
        None => "null".to_string(),
    };
    format!(
        "{{\n  \"alert\": {},\n  \"primary_patch\": {}\n}}\n",
        alert_json(alert),
        patch_json
    )
}

fn vlm_report_json(report: &VlmRoleReport) -> String {
    format!(
        "{{\n  \"role\": \"{}\",\n  \"model\": \"{}\",\n  \"prompt_hash\": \"{}\",\n  \"latency_ms\": {},\n  \"response_json\": {}\n}}\n",
        escape(&report.role),
        escape(&report.model),
        escape(&report.prompt_hash),
        report.latency_ms,
        report.response_json
    )
}

fn report_markdown(alert: &Alert) -> String {
    let narrative = alert
        .vlm_reports
        .iter()
        .find(|report| report.role == "role_03_narrative")
        .map(|report| report.response_json.as_str())
        .unwrap_or("{}");
    format!(
        "# Forest Guardian Evidence Packet\n\n- Alert ID: `{}`\n- Level: `{:?}`\n- Disturbance score: `{:.3}`\n- Area estimate: `{:.2} ha`\n\n## LFM VLM narrative JSON\n\n```json\n{}\n```\n",
        alert.id, alert.level, alert.disturbance_score, alert.area_ha, narrative
    )
}

fn manifest_json(alert: &Alert, files: &BTreeMap<String, String>, manifest_hash: &str) -> String {
    let files_json = files
        .iter()
        .map(|(path, hash)| format!("    \"{}\": \"sha256:{}\"", escape(path), hash))
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{{\n  \"packet_version\": \"forest-guardian-evidence-v1\",\n  \"alert_id\": \"{}\",\n  \"files\": {{\n{}\n  }},\n  \"manifest_hash\": \"{}\"\n}}\n",
        escape(&alert.id),
        files_json,
        manifest_hash
    )
}

fn packet_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    visit_files(dir, &mut out)?;
    out.sort();
    Ok(out)
}
fn visit_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            visit_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}
fn hash_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    Ok(stable_hash(&bytes))
}
fn stable_hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 1469598103934665603;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}
fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Alert, AlertLevel};

    #[test]
    fn packet_hashes_verify() {
        let dir = std::env::temp_dir().join(format!("fg-packet-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let alert = Alert {
            id: "alert-test".to_string(),
            level: AlertLevel::Investigate,
            disturbance_score: 0.7,
            quality_score: 0.9,
            loss_score: 0.8,
            area_ha: 1.2,
            possible_unauthorized_activity: true,
            false_positive_risks: vec![],
            primary_patch: None,
            vlm_reports: vec![VlmRoleReport {
                role: "role_03_narrative".to_string(),
                model: "lfm2-vl".to_string(),
                prompt_hash: "abc".to_string(),
                input_image_hashes: vec!["def".to_string()],
                response_json: "{\"what_was_detected\":\"disturbance\"}".to_string(),
                latency_ms: 10,
            }],
            created_at: "unix:0".to_string(),
        };
        let obs = Observation {
            source: "simsat-capture".to_string(),
            image_datetime: "2026-01-01T00:00:00Z".to_string(),
            width: 1,
            height: 1,
            bands: Default::default(),
            cloud_cover: 0.0,
            footprint_geojson: "{}".to_string(),
            rgb_png_base64: "iVBORw0KGgo=".to_string(),
            content_hash: "hash".to_string(),
        };
        write_packet(&dir, &obs, &obs, &alert).unwrap();
        verify_packet(&dir).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }
}
