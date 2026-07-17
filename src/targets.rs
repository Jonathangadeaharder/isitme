//! Hardcoded ping targets. The first baseline target is used by the
//! verdict logic to distinguish "your network" from "vendor side".

#[derive(Debug, Clone, Copy)]
pub struct Target {
    pub host: &'static str,
    pub label: &'static str,
    pub is_baseline: bool,
}

/// Name of the target treated as the baseline. Must match one of the
/// `host` fields below.
pub const BASELINE_HOST: &str = "8.8.8.8";

pub const TARGETS: &[Target] = &[
    Target { host: "8.8.8.8", label: "Google DNS (baseline)", is_baseline: true },
    Target { host: "1.1.1.1", label: "Cloudflare DNS", is_baseline: false },
    Target { host: "zoom.us", label: "Zoom", is_baseline: false },
    Target { host: "teams.microsoft.com", label: "MS Teams", is_baseline: false },
    Target { host: "meet.google.com", label: "Google Meet", is_baseline: false },
];

pub fn hosts() -> Vec<&'static str> {
    TARGETS.iter().map(|t| t.host).collect()
}

/// Human label for a host, falling back to the host itself.
pub fn label_for(host: &str) -> &'static str {
    TARGETS
        .iter()
        .find(|t| t.host == host)
        .map(|t| t.label)
        .unwrap_or("unknown")
}

/// True if `host` is the configured baseline target.
pub fn is_baseline(host: &str) -> bool {
    TARGETS.iter().any(|t| t.host == host && t.is_baseline)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_host_is_present_in_targets() {
        assert!(TARGETS.iter().any(|t| t.host == BASELINE_HOST));
        let baseline = TARGETS.iter().find(|t| t.is_baseline).unwrap();
        assert_eq!(baseline.host, BASELINE_HOST);
    }

    #[test]
    fn exactly_one_baseline() {
        let n = TARGETS.iter().filter(|t| t.is_baseline).count();
        assert_eq!(n, 1);
    }

    #[test]
    fn hosts_non_empty_and_unique() {
        let h = hosts();
        assert!(!h.is_empty());
        let mut sorted = h.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), h.len());
    }

    #[test]
    fn label_for_known_and_unknown() {
        assert_eq!(label_for("zoom.us"), "Zoom");
        assert_eq!(label_for("8.8.8.8"), "Google DNS (baseline)");
        assert_eq!(label_for("nope.example"), "unknown");
    }

    #[test]
    fn is_baseline_only_for_baseline_host() {
        assert!(is_baseline(BASELINE_HOST));
        assert!(!is_baseline("zoom.us"));
    }
}
