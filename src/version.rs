//! Startup version check against crates.io.
//!
//! Runs concurrently with the diagnostic so it never adds latency.
//! A 24h cache (in the user's cache dir) prevents hitting the API on
//! every invocation. All failures are silent: a version check must
//! never break the actual diagnostic.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The version compiled into this binary.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// True if `latest` is a strictly newer semver than `current`.
/// Returns false on any parse failure (defensive: never misreport).
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (semver::Version::parse(current), semver::Version::parse(latest)) {
        (Ok(c), Ok(l)) => l > c,
        _ => false,
    }
}

/// Run the update check. Returns the latest version string only when a
/// newer release exists; returns None when up to date, when the check
/// fails, or when the cache is fresh and there is no update.
///
/// `client` must have a User-Agent set or crates.io returns 403.
pub async fn check_for_update(client: &reqwest::Client) -> Option<String> {
    let current = current_version();

    // Cache short-circuit only when it already shows a newer version.
    // Otherwise still hit the network: a release may have published
    // after the cache was written, and we'd miss it for up to 24h.
    if let Some(cache) = read_cache()
        && cache.is_fresh()
        && is_newer(current, &cache.latest)
    {
        return Some(cache.latest);
    }

    // Network check, hard-capped at 2s so it never blocks the
    // diagnostic longer than the ping phase.
    let latest = match tokio::time::timeout(
        Duration::from_secs(2),
        fetch_latest_version(client),
    )
    .await
    {
        Ok(Some(v)) => v,
        _ => return None,
    };

    let _ = write_cache(&latest);
    if is_newer(current, &latest) {
        Some(latest)
    } else {
        None
    }
}

/// Hit the crates.io API for the latest stable version. Returns None
/// on any network or parse failure.
async fn fetch_latest_version(client: &reqwest::Client) -> Option<String> {
    let resp = client
        .get("https://crates.io/api/v1/crates/isitme")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: CratesResponse = resp.json().await.ok()?;
    Some(body.crate_.max_stable_version)
}

#[derive(Deserialize)]
struct CratesResponse {
    #[serde(rename = "crate")]
    crate_: CrateInfo,
}

#[derive(Deserialize)]
struct CrateInfo {
    max_stable_version: String,
}

// --- cache ---

const CACHE_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Serialize, Deserialize)]
struct Cache {
    /// Unix timestamp of the last successful network check.
    checked_at: u64,
    latest: String,
}

impl Cache {
    fn is_fresh(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(self.checked_at) < CACHE_TTL_SECS
    }
}

fn cache_path() -> Option<PathBuf> {
    let base = std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("isitme").join("version.json"))
}

fn read_cache() -> Option<Cache> {
    let path = cache_path()?;
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_cache(latest: &str) -> Option<()> {
    let path = cache_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cache = Cache {
        checked_at: now,
        latest: latest.to_string(),
    };
    let data = serde_json::to_string(&cache).ok()?;
    std::fs::write(&path, data).ok()?;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_basic() {
        assert!(is_newer("0.4.0", "0.5.0"));
        assert!(is_newer("0.4.0", "0.4.1"));
    }

    #[test]
    fn is_newer_false_when_same_or_older() {
        assert!(!is_newer("0.4.0", "0.4.0"));
        assert!(!is_newer("0.4.0", "0.3.0"));
        assert!(!is_newer("0.4.0", "0.4.0"));
    }

    #[test]
    fn is_newer_handles_double_digits() {
        // String compare would say "0.10.0" < "0.4.0"; semver must not.
        assert!(is_newer("0.4.0", "0.10.0"));
        assert!(is_newer("0.9.9", "0.10.0"));
        assert!(!is_newer("0.10.0", "0.9.9"));
    }

    #[test]
    fn is_newer_handles_major_bumps() {
        assert!(is_newer("0.4.0", "1.0.0"));
        assert!(is_newer("1.9.0", "2.0.0"));
    }

    #[test]
    fn is_newer_false_on_invalid_input() {
        assert!(!is_newer("not-a-version", "0.5.0"));
        assert!(!is_newer("0.4.0", "garbage"));
        assert!(!is_newer("", ""));
    }

    #[test]
    fn cache_fresh_within_24h() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let c = Cache {
            checked_at: now - 100,
            latest: "0.5.0".to_string(),
        };
        assert!(c.is_fresh());
    }

    #[test]
    fn cache_stale_after_24h() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let c = Cache {
            checked_at: now - CACHE_TTL_SECS - 1,
            latest: "0.5.0".to_string(),
        };
        assert!(!c.is_fresh());
    }

    #[test]
    fn cache_fresh_at_boundary() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let c = Cache {
            checked_at: now - CACHE_TTL_SECS + 1,
            latest: "0.5.0".to_string(),
        };
        assert!(c.is_fresh());
    }
}
