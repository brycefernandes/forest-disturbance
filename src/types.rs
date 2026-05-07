use std::collections::BTreeMap;

pub const REQUIRED_BANDS: [&str; 6] = ["red", "green", "blue", "nir", "swir16", "swir22"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    pub lon: f64,
    pub lat: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub source: String,
    pub image_datetime: String,
    pub width: usize,
    pub height: usize,
    pub bands: BTreeMap<String, Vec<f32>>,
    pub cloud_cover: f32,
    pub footprint_geojson: String,
    pub rgb_png_base64: String,
    pub content_hash: String,
}

impl Observation {
    pub fn band(&self, name: &str) -> Result<&[f32], String> {
        self.bands
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| format!("SimSat response omitted required band '{name}'"))
    }
    pub fn validate(&self) -> Result<(), String> {
        let expected = self.width * self.height;
        for band in REQUIRED_BANDS {
            let values = self.band(band)?;
            if values.len() != expected {
                return Err(format!(
                    "band '{band}' has {} pixels; expected {expected}",
                    values.len()
                ));
            }
            if values
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0 || *v > 1.0)
            {
                return Err(format!("band '{band}' contains invalid reflectance"));
            }
        }
        if self.rgb_png_base64.trim().is_empty() {
            return Err(
                "SimSat response must include rgb_png_base64 for LFM VLM evidence review"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexMaps {
    pub ndvi: Vec<f32>,
    pub ndmi: Vec<f32>,
    pub nbr: Vec<f32>,
    pub bsi: Vec<f32>,
    pub ndwi: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    pub id: String,
    pub pixel_count: usize,
    pub area_ha: f32,
    pub mean_delta_ndvi: f32,
    pub mean_delta_nbr: f32,
    pub mean_delta_bsi: f32,
    pub forest_baseline_fraction: f32,
    pub bbox_pixels: [usize; 4],
    pub centroid_pixel: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertLevel {
    None,
    Watch,
    Investigate,
    EnforcementReview,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VlmRoleReport {
    pub role: String,
    pub model: String,
    pub prompt_hash: String,
    pub input_image_hashes: Vec<String>,
    pub response_json: String,
    pub latency_ms: u128,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub id: String,
    pub level: AlertLevel,
    pub disturbance_score: f32,
    pub quality_score: f32,
    pub loss_score: f32,
    pub area_ha: f32,
    pub possible_unauthorized_activity: bool,
    pub false_positive_risks: Vec<String>,
    pub primary_patch: Option<Patch>,
    pub vlm_reports: Vec<VlmRoleReport>,
    pub created_at: String,
}
