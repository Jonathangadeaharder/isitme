//! Download/upload throughput test against Cloudflare's speed endpoint.
//!
//! Pure throughput math (`mbps_from_bytes_seconds`) is unit-tested. The
//! network I/O is a thin wrapper around that pure function.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

use crate::verdict::SpeedStats;

const DOWN_URL: &str = "https://speed.cloudflare.com/__down?bytes=25000000";
const UP_URL: &str = "https://speed.cloudflare.com/__up";
const UPLOAD_BYTES: usize = 25_000_000;

/// Convert byte count and elapsed seconds into Mbps.
pub fn mbps_from_bytes_seconds(bytes: u64, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    let bits = bytes as f64 * 8.0;
    bits / seconds / 1_000_000.0
}

/// Run both download and upload tests. Failures degrade gracefully to None.
pub async fn run_speed_test(client: &reqwest::Client) -> SpeedStats {
    let download_mbps = download(client).await.ok();
    let upload_mbps = upload(client).await.ok();
    SpeedStats {
        download_mbps,
        upload_mbps,
    }
}

async fn download(client: &reqwest::Client) -> Result<f64> {
    let bar = ProgressBar::new(25_000_000);
    bar.set_style(
        ProgressStyle::with_template("{spinner} {msg} {wide_bar} {bytes}/{total_bytes}")
            .unwrap(),
    );
    bar.set_message("download");

    let start = std::time::Instant::now();
    let mut resp = client.get(DOWN_URL).send().await?.error_for_status()?;
    let mut total: u64 = 0;
    while let Some(chunk) = resp.chunk().await? {
        total += chunk.len() as u64;
        bar.inc(chunk.len() as u64);
    }
    let elapsed = start.elapsed().as_secs_f64();
    bar.finish_and_clear();
    Ok(mbps_from_bytes_seconds(total, elapsed))
}

async fn upload(client: &reqwest::Client) -> Result<f64> {
    let bar = ProgressBar::new(UPLOAD_BYTES as u64);
    bar.set_style(
        ProgressStyle::with_template("{spinner} {msg} {wide_bar} {bytes}/{total_bytes}")
            .unwrap(),
    );
    bar.set_message("upload");

    // Load body into memory; 25MB is fine for a CLI diagnostic tool.
    let body = vec![0u8; UPLOAD_BYTES];
    let start = std::time::Instant::now();
    let resp = client
        .post(UP_URL)
        .header("Content-Length", UPLOAD_BYTES.to_string())
        .body(body)
        .send()
        .await?
        .error_for_status()?;
    let _ = resp.bytes().await?;
    let elapsed = start.elapsed().as_secs_f64();
    bar.finish_and_clear();
    Ok(mbps_from_bytes_seconds(UPLOAD_BYTES as u64, elapsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn mbps_basic_25mb_in_one_second() {
        // 25 MB in 1s = 200 Mbps.
        let m = mbps_from_bytes_seconds(25_000_000, 1.0);
        assert!((m - 200.0).abs() < 0.01, "got {}", m);
    }

    #[test]
    fn mbps_zero_seconds_yields_zero() {
        assert_eq!(mbps_from_bytes_seconds(25_000_000, 0.0), 0.0);
    }

    #[test]
    fn mbps_negative_seconds_yields_zero() {
        assert_eq!(mbps_from_bytes_seconds(25_000_000, -1.0), 0.0);
    }

    #[test]
    fn mbps_scales_linearly() {
        let fast = mbps_from_bytes_seconds(25_000_000, 1.0);
        let slow = mbps_from_bytes_seconds(25_000_000, 2.0);
        assert!((fast / slow - 2.0).abs() < 0.01);
    }

    #[test]
    fn mbps_ten_mb_in_half_second() {
        // 10 MB in 0.5s = 160 Mbps.
        let m = mbps_from_bytes_seconds(10_000_000, 0.5);
        assert!((m - 160.0).abs() < 0.01, "got {}", m);
    }
}
