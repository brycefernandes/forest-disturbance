use crate::evidence::write_packet;
use crate::simsat::SimSatClient;
use crate::types::{Alert, AlertLevel, GeoPoint, IndexMaps, Observation, Patch, VlmRoleReport};
use crate::vlm::{LfmVlmClient, png_data_url};
use std::collections::VecDeque;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub simsat_url: String,
    pub vlm_url: String,
    pub vlm_model: String,
    pub point: GeoPoint,
    pub baseline_timestamp: String,
    pub current_timestamp: String,
    pub size_km: f64,
    pub window_seconds: u64,
    pub out_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub alert: Alert,
    pub packet_dir: PathBuf,
}

pub fn run_pipeline(cfg: PipelineConfig) -> Result<PipelineResult, String> {
    let simsat = SimSatClient::new(&cfg.simsat_url);
    let vlm = LfmVlmClient::new(&cfg.vlm_url, &cfg.vlm_model);
    let baseline = simsat
        .get_image_sentinel(
            cfg.point,
            &cfg.baseline_timestamp,
            cfg.size_km,
            cfg.window_seconds,
        )
        .map_err(|e| format!("loading baseline observation from SimSat: {e}"))?;
    let current = simsat
        .get_image_sentinel(
            cfg.point,
            &cfg.current_timestamp,
            cfg.size_km,
            cfg.window_seconds,
        )
        .map_err(|e| format!("loading current observation from SimSat: {e}"))?;
    let mut alert = analyze_observations(&baseline, &current)?;
    let image_urls = vec![
        png_data_url(&baseline.rgb_png_base64),
        png_data_url(&current.rgb_png_base64),
    ];
    let vlm_reports = run_vlm_roles(&vlm, &alert, &image_urls)?;
    apply_vlm_demotions(&mut alert, &vlm_reports);
    alert.vlm_reports = vlm_reports;
    let packet_dir = write_packet(&cfg.out_dir, &baseline, &current, &alert)?;
    Ok(PipelineResult { alert, packet_dir })
}

pub fn analyze_observations(
    baseline: &Observation,
    current: &Observation,
) -> Result<Alert, String> {
    baseline.validate()?;
    current.validate()?;
    if baseline.width != current.width || baseline.height != current.height {
        return Err("baseline/current dimensions differ".to_string());
    }
    let baseline_idx = compute_indices(baseline)?;
    let current_idx = compute_indices(current)?;
    let mask = candidate_loss_mask(&baseline_idx, &current_idx, current.cloud_cover);
    let patches = extract_patches(
        &mask,
        baseline.width,
        baseline.height,
        &baseline_idx,
        &current_idx,
    );
    let primary_patch = patches.into_iter().max_by_key(|patch| patch.pixel_count);
    let (loss_score, quality_score, disturbance_score, area_ha) =
        if let Some(patch) = &primary_patch {
            let loss = loss_score(patch);
            let quality = quality_score(current.cloud_cover, 0.0, 0.0);
            (loss, quality, loss * quality, patch.area_ha)
        } else {
            (0.0, quality_score(current.cloud_cover, 0.0, 0.0), 0.0, 0.0)
        };
    let mut level = level_for_score(disturbance_score);
    if current.cloud_cover > 0.20 && matches!(level, AlertLevel::EnforcementReview) {
        level = AlertLevel::Investigate;
    }
    Ok(Alert {
        id: new_id("alert"),
        level,
        disturbance_score,
        quality_score,
        loss_score,
        area_ha,
        possible_unauthorized_activity: matches!(level, AlertLevel::EnforcementReview),
        false_positive_risks: Vec::new(),
        primary_patch,
        vlm_reports: Vec::new(),
        created_at: now_string(),
    })
}

fn run_vlm_roles(
    vlm: &LfmVlmClient,
    alert: &Alert,
    image_urls: &[String],
) -> Result<Vec<VlmRoleReport>, String> {
    let feature_summary = alert_json(alert);
    let triage = vlm.run_role(
        "role_01_triage",
        "You are LFM VLM acting as a strict forest-disturbance triage reviewer. Return only JSON with keys image_quality, visible_clouds_over_30_pct, visible_cloud_shadows, geometric_agriculture_pattern, open_water_or_flooding, recent_bare_ground_visible, concurrence, false_positive_risks, notes. You may identify false-positive risks but you may not promote alert severity.",
        &format!("Review these SimSat before/current RGB images and numeric alert features:\n{feature_summary}"),
        image_urls,
    )?;
    let narrative = vlm.run_role(
        "role_03_narrative",
        "You are LFM VLM preparing an evidence-packet narrative. Return only JSON with keys what_was_detected, what_visual_evidence_shows, legal_context_summary, recommended_action_explanation, evidence_citations, legal_disclaimer. The legal_disclaimer must say findings require authority review and ground verification.",
        &format!("Write a concise review narrative from this alert JSON:\n{feature_summary}"),
        image_urls,
    )?;
    Ok(vec![triage, narrative])
}

fn apply_vlm_demotions(alert: &mut Alert, reports: &[VlmRoleReport]) {
    for report in reports {
        if report.role != "role_01_triage" {
            continue;
        }
        let json = &report.response_json;
        let unusable = json.contains("\"image_quality\":\"unusable\"")
            || json.contains("\"image_quality\": \"unusable\"");
        let clouds = json.contains("\"visible_clouds_over_30_pct\":true")
            || json.contains("\"visible_clouds_over_30_pct\": true");
        let agriculture = json.contains("\"geometric_agriculture_pattern\":true")
            || json.contains("\"geometric_agriculture_pattern\": true");
        let water = json.contains("\"open_water_or_flooding\":true")
            || json.contains("\"open_water_or_flooding\": true");
        if unusable || clouds || agriculture || water {
            alert.level = demote(alert.level);
        }
        for (present, risk) in [
            (clouds, "cloud"),
            (agriculture, "seasonal_crop_harvest"),
            (water, "water"),
        ] {
            if present && !alert.false_positive_risks.iter().any(|item| item == risk) {
                alert.false_positive_risks.push(risk.to_string());
            }
        }
    }
}

fn compute_indices(obs: &Observation) -> Result<IndexMaps, String> {
    let red = obs.band("red")?;
    let green = obs.band("green")?;
    let blue = obs.band("blue")?;
    let nir = obs.band("nir")?;
    let swir1 = obs.band("swir16")?;
    let swir2 = obs.band("swir22")?;
    let len = red.len();
    let mut maps = IndexMaps {
        ndvi: Vec::with_capacity(len),
        ndmi: Vec::with_capacity(len),
        nbr: Vec::with_capacity(len),
        bsi: Vec::with_capacity(len),
        ndwi: Vec::with_capacity(len),
    };
    for i in 0..len {
        maps.ndvi.push(safe_index(nir[i] - red[i], nir[i] + red[i]));
        maps.ndmi
            .push(safe_index(nir[i] - swir1[i], nir[i] + swir1[i]));
        maps.nbr
            .push(safe_index(nir[i] - swir2[i], nir[i] + swir2[i]));
        maps.bsi.push(safe_index(
            (swir1[i] + red[i]) - (nir[i] + blue[i]),
            (swir1[i] + red[i]) + (nir[i] + blue[i]),
        ));
        maps.ndwi
            .push(safe_index(green[i] - nir[i], green[i] + nir[i]));
    }
    Ok(maps)
}

fn candidate_loss_mask(baseline: &IndexMaps, current: &IndexMaps, cloud_cover: f32) -> Vec<bool> {
    if cloud_cover > 0.40 {
        return vec![false; baseline.ndvi.len()];
    }
    (0..baseline.ndvi.len())
        .map(|i| {
            let forest = baseline.ndvi[i] > 0.55
                && baseline.ndmi[i] > 0.10
                && baseline.ndwi[i] < 0.30
                && baseline.bsi[i] < 0.20;
            let water = current.ndwi[i] > 0.35 && current.ndvi[i] < 0.30;
            forest
                && current.ndvi[i] - baseline.ndvi[i] < -0.25
                && current.nbr[i] - baseline.nbr[i] < -0.25
                && current.bsi[i] - baseline.bsi[i] > 0.15
                && current.ndvi[i] < 0.45
                && !water
        })
        .collect()
}

fn extract_patches(
    mask: &[bool],
    width: usize,
    height: usize,
    baseline: &IndexMaps,
    current: &IndexMaps,
) -> Vec<Patch> {
    let mut visited = vec![false; mask.len()];
    let mut patches = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || visited[start] {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        visited[start] = true;
        let mut pixels = Vec::new();
        while let Some(idx) = queue.pop_front() {
            pixels.push(idx);
            let x = idx % width;
            let y = idx / width;
            let neighbors = [
                (x.wrapping_sub(1), y, x > 0),
                (x + 1, y, x + 1 < width),
                (x, y.wrapping_sub(1), y > 0),
                (x, y + 1, y + 1 < height),
            ];
            for (nx, ny, valid) in neighbors {
                if !valid {
                    continue;
                }
                let next = ny * width + nx;
                if mask[next] && !visited[next] {
                    visited[next] = true;
                    queue.push_back(next);
                }
            }
        }
        if pixels.len() >= 25 {
            patches.push(patch_from_pixels(&pixels, width, baseline, current));
        }
    }
    patches
}

fn patch_from_pixels(
    pixels: &[usize],
    width: usize,
    baseline: &IndexMaps,
    current: &IndexMaps,
) -> Patch {
    let mut min_x = usize::MAX;
    let mut min_y = usize::MAX;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut d_ndvi = 0.0;
    let mut d_nbr = 0.0;
    let mut d_bsi = 0.0;
    let mut forest = 0usize;
    for idx in pixels {
        let x = idx % width;
        let y = idx / width;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        sum_x += x as f32;
        sum_y += y as f32;
        d_ndvi += current.ndvi[*idx] - baseline.ndvi[*idx];
        d_nbr += current.nbr[*idx] - baseline.nbr[*idx];
        d_bsi += current.bsi[*idx] - baseline.bsi[*idx];
        if baseline.ndvi[*idx] > 0.55 {
            forest += 1;
        }
    }
    let n = pixels.len() as f32;
    Patch {
        id: new_id("patch"),
        pixel_count: pixels.len(),
        area_ha: n * 0.01,
        mean_delta_ndvi: d_ndvi / n,
        mean_delta_nbr: d_nbr / n,
        mean_delta_bsi: d_bsi / n,
        forest_baseline_fraction: forest as f32 / n,
        bbox_pixels: [min_x, min_y, max_x, max_y],
        centroid_pixel: [sum_x / n, sum_y / n],
    }
}

fn loss_score(patch: &Patch) -> f32 {
    clamp01(
        0.25 * norm_neg(patch.mean_delta_ndvi, -0.25)
            + 0.25 * norm_neg(patch.mean_delta_nbr, -0.25)
            + 0.15 * norm_pos(patch.mean_delta_bsi, 0.15)
            + 0.10 * patch.forest_baseline_fraction
            + 0.10 * patch_area_score(patch.area_ha)
            + 0.10
            + 0.05,
    )
}

fn quality_score(cloud_cover: f32, no_data_fraction: f32, misregistration_risk: f32) -> f32 {
    clamp01(1.0 - 0.5 * cloud_cover - 0.3 * no_data_fraction - 0.2 * misregistration_risk)
}

fn level_for_score(score: f32) -> AlertLevel {
    if score >= 0.75 {
        AlertLevel::EnforcementReview
    } else if score >= 0.55 {
        AlertLevel::Investigate
    } else if score >= 0.30 {
        AlertLevel::Watch
    } else {
        AlertLevel::None
    }
}

fn demote(level: AlertLevel) -> AlertLevel {
    match level {
        AlertLevel::EnforcementReview => AlertLevel::Investigate,
        AlertLevel::Investigate => AlertLevel::Watch,
        AlertLevel::Watch => AlertLevel::None,
        AlertLevel::None => AlertLevel::None,
    }
}

fn norm_neg(x: f32, threshold: f32) -> f32 {
    if x <= 0.0 {
        clamp01(-x / (-threshold).max(1e-6))
    } else {
        0.0
    }
}

fn norm_pos(x: f32, threshold: f32) -> f32 {
    if x >= 0.0 {
        clamp01(x / threshold.max(1e-6))
    } else {
        0.0
    }
}
fn patch_area_score(area_ha: f32) -> f32 {
    clamp01(((area_ha.max(0.01) + 1.0).log10()) / 2.0)
}
fn safe_index(num: f32, den: f32) -> f32 {
    if den.abs() <= 1e-6 {
        0.0
    } else {
        (num / den).clamp(-1.0, 1.0)
    }
}
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

pub fn alert_json(alert: &Alert) -> String {
    format!(
        "{{\"id\":\"{}\",\"level\":\"{:?}\",\"disturbance_score\":{:.6},\"quality_score\":{:.6},\"loss_score\":{:.6},\"area_ha\":{:.4},\"possible_unauthorized_activity\":{}}}",
        alert.id,
        alert.level,
        alert.disturbance_score,
        alert.quality_score,
        alert.loss_score,
        alert.area_ha,
        alert.possible_unauthorized_activity
    )
}

fn new_id(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{nanos:x}")
}
fn now_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn detects_clear_cut_patch_from_multispectral_change() {
        let baseline = observation(true, 0.05);
        let current = observation(false, 0.05);
        let alert = analyze_observations(&baseline, &current).unwrap();
        assert!(matches!(alert.level, AlertLevel::EnforcementReview));
        let patch = alert.primary_patch.unwrap();
        assert_eq!(patch.pixel_count, 36);
        assert!(patch.mean_delta_ndvi < -0.25);
        assert!(patch.mean_delta_bsi > 0.15);
    }

    #[test]
    fn blocks_cloudy_scene_before_alerting() {
        let baseline = observation(true, 0.05);
        let current = observation(false, 0.60);
        let alert = analyze_observations(&baseline, &current).unwrap();
        assert!(matches!(alert.level, AlertLevel::None));
        assert!(alert.primary_patch.is_none());
    }

    pub fn observation(forest_patch: bool, cloud_cover: f32) -> Observation {
        let width = 8;
        let height = 8;
        let mut bands = BTreeMap::new();
        for name in ["red", "green", "blue", "nir", "swir16", "swir22"] {
            bands.insert(name.to_string(), vec![0.1; width * height]);
        }
        for y in 1..7 {
            for x in 1..7 {
                let idx = y * width + x;
                if forest_patch {
                    bands.get_mut("red").unwrap()[idx] = 0.08;
                    bands.get_mut("green").unwrap()[idx] = 0.12;
                    bands.get_mut("blue").unwrap()[idx] = 0.05;
                    bands.get_mut("nir").unwrap()[idx] = 0.72;
                    bands.get_mut("swir16").unwrap()[idx] = 0.22;
                    bands.get_mut("swir22").unwrap()[idx] = 0.18;
                } else {
                    bands.get_mut("red").unwrap()[idx] = 0.28;
                    bands.get_mut("green").unwrap()[idx] = 0.18;
                    bands.get_mut("blue").unwrap()[idx] = 0.12;
                    bands.get_mut("nir").unwrap()[idx] = 0.22;
                    bands.get_mut("swir16").unwrap()[idx] = 0.42;
                    bands.get_mut("swir22").unwrap()[idx] = 0.48;
                }
            }
        }
        Observation {
            source: "unit-test-simsat-capture".to_string(),
            image_datetime: "2026-01-01T00:00:00Z".to_string(),
            width,
            height,
            bands,
            cloud_cover,
            footprint_geojson: "{\"type\":\"Polygon\",\"coordinates\":[]}".to_string(),
            rgb_png_base64: "iVBORw0KGgo=".to_string(),
            content_hash: "test".to_string(),
        }
    }
}
