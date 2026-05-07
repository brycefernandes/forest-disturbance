use forest_guardian::pipeline::{PipelineConfig, run_pipeline};
use forest_guardian::types::GeoPoint;
use std::path::PathBuf;

fn main() {
    if let Err(err) = real_main() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return Ok(());
    }
    let command = args.remove(0);
    match command.as_str() {
        "run" => {
            let cfg = PipelineConfig {
                simsat_url: value(&args, "--simsat-url")
                    .or_else(|| std::env::var("FG_SIMSAT_URL").ok())
                    .unwrap_or_else(|| "http://localhost:9005".to_string()),
                vlm_url: value(&args, "--vlm-url")
                    .or_else(|| std::env::var("FG_VLM_URL").ok())
                    .unwrap_or_else(|| "http://localhost:8080".to_string()),
                vlm_model: value(&args, "--vlm-model")
                    .or_else(|| std::env::var("FG_VLM_MODEL").ok())
                    .unwrap_or_else(|| "lfm2-vl".to_string()),
                point: GeoPoint {
                    lon: required(&args, "--lon")?
                        .parse::<f64>()
                        .map_err(|e| e.to_string())?,
                    lat: required(&args, "--lat")?
                        .parse::<f64>()
                        .map_err(|e| e.to_string())?,
                },
                baseline_timestamp: required(&args, "--baseline-timestamp")?,
                current_timestamp: required(&args, "--current-timestamp")?,
                size_km: value(&args, "--size-km")
                    .unwrap_or_else(|| "5.0".to_string())
                    .parse::<f64>()
                    .map_err(|e| e.to_string())?,
                window_seconds: value(&args, "--window-seconds")
                    .unwrap_or_else(|| "864000".to_string())
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?,
                out_dir: PathBuf::from(
                    value(&args, "--out-dir")
                        .unwrap_or_else(|| "data/alert_packets/latest".to_string()),
                ),
            };
            let result = run_pipeline(cfg)?;
            println!("alert_id={}", result.alert.id);
            println!("level={:?}", result.alert.level);
            println!("disturbance_score={:.3}", result.alert.disturbance_score);
            println!("packet_dir={}", result.packet_dir.display());
        }
        "verify" => {
            let packet_dir = required(&args, "--packet-dir")?;
            forest_guardian::evidence::verify_packet(PathBuf::from(&packet_dir).as_path())?;
            println!("packet verified: {packet_dir}");
        }
        _ => return Err(format!("unknown command '{command}'")),
    }
    Ok(())
}

fn value(args: &[String], key: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == key)
        .map(|pair| pair[1].clone())
}
fn required(args: &[String], key: &str) -> Result<String, String> {
    value(args, key).ok_or_else(|| format!("missing required argument {key}"))
}
fn print_help() {
    println!(
        "forest-guardian\n\nCommands:\n  run --lon <f64> --lat <f64> --baseline-timestamp <ISO> --current-timestamp <ISO> [--simsat-url http://localhost:9005] [--vlm-url http://localhost:8080] [--vlm-model lfm2-vl] [--out-dir data/alert_packets/latest]\n  verify --packet-dir <path>\n\nThe run command uses real SimSat and LFM VLM HTTP services and exits non-zero if either is unavailable."
    );
}
