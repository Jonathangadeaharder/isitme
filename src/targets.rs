//! Hardcoded ping targets. The baseline target (matched by host against
//! `BASELINE_HOST`) is used by the verdict logic to distinguish "your
//! network" from "vendor side".

#[derive(Debug, Clone, Copy)]
pub struct Target {
    pub host: &'static str,
    pub label: &'static str,
}

/// Host treated as the baseline. Must match one of the `host` fields.
pub const BASELINE_HOST: &str = "8.8.8.8";

pub const TARGETS: &[Target] = &[
    Target { host: "8.8.8.8", label: "Google DNS (baseline)" },
    Target { host: "1.1.1.1", label: "Cloudflare DNS" },
    Target { host: "zoom.us", label: "Zoom" },
    Target { host: "teams.microsoft.com", label: "MS Teams" },
    Target { host: "meet.google.com", label: "Google Meet" },
];

pub fn hosts() -> Vec<&'static str> {
    TARGETS.iter().map(|t| t.host).collect()
}

/// Human label for a host, falling back to "unknown".
pub fn label_for(host: &str) -> &'static str {
    TARGETS
        .iter()
        .find(|t| t.host == host)
        .map(|t| t.label)
        .unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_host_is_present_in_targets() {
        assert!(TARGETS.iter().any(|t| t.host == BASELINE_HOST));
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
}
