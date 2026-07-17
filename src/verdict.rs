//! Threshold rules and verdict classification for isitme.
//!
//! Pure logic only. No I/O. Thresholds are hardcoded per the design plan.

use serde::{Deserialize, Serialize};

/// Three-level status for any single metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Bad,
}

impl Status {
    /// Worst of a non-empty slice. Bad > Warn > Ok. Empty => Ok.
    pub fn worst_of(items: &[Status]) -> Status {
        items
            .iter()
            .copied()
            .fold(Status::Ok, |acc, s| match (acc, s) {
                (Status::Bad, _) | (_, Status::Bad) => Status::Bad,
                (Status::Warn, _) | (_, Status::Warn) => Status::Warn,
                _ => Status::Ok,
            })
    }
}

/// Ping-derived stats for a single target, in milliseconds and percent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PingStats {
    pub target: String,
    pub samples_ms: Vec<f64>,
    pub packets_sent: u32,
    pub packets_received: u32,
}

impl PingStats {
    pub fn loss_pct(&self) -> f64 {
        if self.packets_sent == 0 {
            return 0.0;
        }
        let lost = self.packets_sent.saturating_sub(self.packets_received);
        (lost as f64 / self.packets_sent as f64) * 100.0
    }

    pub fn avg_ms(&self) -> Option<f64> {
        if self.samples_ms.is_empty() {
            return None;
        }
        Some(self.samples_ms.iter().sum::<f64>() / self.samples_ms.len() as f64)
    }

    pub fn min_ms(&self) -> Option<f64> {
        self.samples_ms.iter().copied().fold(None, |acc, v| match acc {
            None => Some(v),
            Some(cur) => Some(cur.min(v)),
        })
    }

    pub fn max_ms(&self) -> Option<f64> {
        self.samples_ms.iter().copied().fold(None, |acc, v| match acc {
            None => Some(v),
            Some(cur) => Some(cur.max(v)),
        })
    }

    /// Sample standard deviation of RTTs (jitter proxy).
    pub fn jitter_ms(&self) -> Option<f64> {
        let n = self.samples_ms.len();
        if n < 2 {
            return None;
        }
        let mean = self.samples_ms.iter().sum::<f64>() / n as f64;
        let var = self.samples_ms.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
        Some(var.sqrt())
    }

    pub fn avg_status(&self) -> Status {
        match self.avg_ms() {
            Some(avg) => classify_avg(avg),
            None => Status::Bad,
        }
    }

    pub fn jitter_status(&self) -> Status {
        match self.jitter_ms() {
            Some(j) => classify_jitter(j),
            None => Status::Warn,
        }
    }

    pub fn loss_status(&self) -> Status {
        classify_loss(self.loss_pct())
    }

    pub fn overall(&self) -> Status {
        Status::worst_of(&[self.avg_status(), self.jitter_status(), self.loss_status()])
    }
}

/// Speed test result, in Mbps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeedStats {
    pub download_mbps: Option<f64>,
    pub upload_mbps: Option<f64>,
}

impl SpeedStats {
    pub fn download_status(&self) -> Status {
        match self.download_mbps {
            Some(d) => classify_download(d),
            None => Status::Bad,
        }
    }

    pub fn upload_status(&self) -> Status {
        match self.upload_mbps {
            Some(u) => classify_upload(u),
            None => Status::Bad,
        }
    }

    pub fn overall(&self) -> Status {
        Status::worst_of(&[self.download_status(), self.upload_status()])
    }
}

// --- threshold functions ---

pub fn classify_avg(avg_ms: f64) -> Status {
    if avg_ms < 150.0 {
        Status::Ok
    } else if avg_ms <= 300.0 {
        Status::Warn
    } else {
        Status::Bad
    }
}

pub fn classify_jitter(jitter_ms: f64) -> Status {
    if jitter_ms < 30.0 {
        Status::Ok
    } else if jitter_ms <= 50.0 {
        Status::Warn
    } else {
        Status::Bad
    }
}

pub fn classify_loss(loss_pct: f64) -> Status {
    if loss_pct < 1.0 {
        Status::Ok
    } else if loss_pct < 5.0 {
        Status::Warn
    } else {
        Status::Bad
    }
}

pub fn classify_download(mbps: f64) -> Status {
    if mbps > 3.0 {
        Status::Ok
    } else if mbps >= 1.0 {
        Status::Warn
    } else {
        Status::Bad
    }
}

pub fn classify_upload(mbps: f64) -> Status {
    if mbps > 1.5 {
        Status::Ok
    } else if mbps >= 0.5 {
        Status::Warn
    } else {
        Status::Bad
    }
}

/// Final verdict against the "isitme?" question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub label: VerdictLabel,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictLabel {
    You,
    Vendor,
    Clear,
}

/// Build the final verdict from per-target ping stats (with one baseline
/// target identified by name), plus speed stats.
///
/// - If baseline is BAD, or speed is BAD: It's you.
/// - Else if any vendor target is BAD: It's <vendor>.
/// - Else: All clear.
pub fn build_verdict(
    baseline_name: &str,
    pings: &[PingStats],
    speed: Option<&SpeedStats>,
) -> Verdict {
    let baseline = pings.iter().find(|p| p.target == baseline_name);
    let baseline_bad = baseline.map(|b| b.overall()) == Some(Status::Bad);

    let speed_bad = match speed {
        Some(s) => s.overall() == Status::Bad,
        None => false,
    };

    if baseline_bad || speed_bad {
        return Verdict {
            label: VerdictLabel::You,
            reason: "Your network is the problem. Try restarting your router, or your reputation."
                .to_string(),
        };
    }

    let bad_vendor = pings
        .iter()
        .find(|p| p.target != baseline_name && p.overall() == Status::Bad);

    if let Some(v) = bad_vendor {
        return Verdict {
            label: VerdictLabel::Vendor,
            reason: format!(
                "{} is having a bad day. Not your fault. Send a passive-aggressive support ticket.",
                v.target
            ),
        };
    }

    let any_warn = pings.iter().any(|p| p.overall() == Status::Warn)
        || speed.map(|s| s.overall() == Status::Warn).unwrap_or(false);

    if any_warn {
        return Verdict {
            label: VerdictLabel::Clear,
            reason: "It works, barely. Like your Monday motivation."
                .to_string(),
        };
    }

    Verdict {
        label: VerdictLabel::Clear,
        reason: "Your network is minty fresh. Go take the call.".to_string(),
    }
}

/// Exit code for the CLI process, derived from verdict label.
pub fn exit_code(label: VerdictLabel) -> i32 {
    match label {
        VerdictLabel::Clear => 0,
        VerdictLabel::Vendor => 1,
        VerdictLabel::You => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn stats(target: &str, samples: &[f64], sent: u32, recv: u32) -> PingStats {
        PingStats {
            target: target.to_string(),
            samples_ms: samples.to_vec(),
            packets_sent: sent,
            packets_received: recv,
        }
    }

    #[test]
    fn classify_avg_thresholds() {
        assert_eq!(classify_avg(10.0), Status::Ok);
        assert_eq!(classify_avg(149.9), Status::Ok);
        assert_eq!(classify_avg(150.0), Status::Warn);
        assert_eq!(classify_avg(300.0), Status::Warn);
        assert_eq!(classify_avg(300.1), Status::Bad);
    }

    #[test]
    fn classify_jitter_thresholds() {
        assert_eq!(classify_jitter(5.0), Status::Ok);
        assert_eq!(classify_jitter(29.9), Status::Ok);
        assert_eq!(classify_jitter(30.0), Status::Warn);
        assert_eq!(classify_jitter(50.0), Status::Warn);
        assert_eq!(classify_jitter(50.1), Status::Bad);
    }

    #[test]
    fn classify_loss_thresholds() {
        assert_eq!(classify_loss(0.0), Status::Ok);
        assert_eq!(classify_loss(0.9), Status::Ok);
        assert_eq!(classify_loss(1.0), Status::Warn);
        assert_eq!(classify_loss(4.9), Status::Warn);
        assert_eq!(classify_loss(5.0), Status::Bad);
    }

    #[test]
    fn classify_download_thresholds() {
        assert_eq!(classify_download(5.0), Status::Ok);
        assert_eq!(classify_download(3.1), Status::Ok);
        assert_eq!(classify_download(3.0), Status::Warn);
        assert_eq!(classify_download(1.0), Status::Warn);
        assert_eq!(classify_download(0.9), Status::Bad);
    }

    #[test]
    fn classify_upload_thresholds() {
        assert_eq!(classify_upload(2.0), Status::Ok);
        assert_eq!(classify_upload(1.6), Status::Ok);
        assert_eq!(classify_upload(1.5), Status::Warn);
        assert_eq!(classify_upload(0.5), Status::Warn);
        assert_eq!(classify_upload(0.4), Status::Bad);
    }

    #[test]
    fn loss_pct_handles_zero_sent() {
        let s = stats("x", &[], 0, 0);
        assert_eq!(s.loss_pct(), 0.0);
    }

    #[test]
    fn loss_pct_computes_percent() {
        let s = stats("x", &[], 10, 8);
        assert_eq!(s.loss_pct(), 20.0);
    }

    #[test]
    fn avg_and_jitter_basic() {
        let s = stats("x", &[10.0, 20.0, 30.0], 3, 3);
        assert_eq!(s.avg_ms(), Some(20.0));
        assert_eq!(s.min_ms(), Some(10.0));
        assert_eq!(s.max_ms(), Some(30.0));
        assert!(s.jitter_ms().unwrap() > 9.0 && s.jitter_ms().unwrap() < 11.0);
    }

    #[test]
    fn jitter_none_for_under_two_samples() {
        let s = stats("x", &[10.0], 1, 1);
        assert_eq!(s.jitter_ms(), None);
        let s0 = stats("x", &[], 0, 0);
        assert_eq!(s0.jitter_ms(), None);
    }

    #[test]
    fn avg_none_when_no_samples() {
        let s = stats("x", &[], 0, 0);
        assert_eq!(s.avg_ms(), None);
        assert_eq!(s.avg_status(), Status::Bad);
    }

    #[test]
    fn worst_of_combinations() {
        assert_eq!(Status::worst_of(&[Status::Ok, Status::Ok]), Status::Ok);
        assert_eq!(Status::worst_of(&[Status::Ok, Status::Warn]), Status::Warn);
        assert_eq!(Status::worst_of(&[Status::Warn, Status::Bad]), Status::Bad);
        assert_eq!(Status::worst_of(&[Status::Ok, Status::Bad]), Status::Bad);
        assert_eq!(Status::worst_of(&[]), Status::Ok);
    }

    #[test]
    fn verdict_you_when_baseline_bad() {
        let pings = vec![
            stats("8.8.8.8", &[400.0, 410.0], 2, 2),
            stats("zoom.us", &[10.0, 11.0], 2, 2),
        ];
        let speed = SpeedStats {
            download_mbps: Some(50.0),
            upload_mbps: Some(10.0),
        };
        let v = build_verdict("8.8.8.8", &pings, Some(&speed));
        assert_eq!(v.label, VerdictLabel::You);
        assert_eq!(exit_code(v.label), 2);
    }

    #[test]
    fn verdict_you_when_speed_bad() {
        let pings = vec![
            stats("8.8.8.8", &[10.0, 11.0], 2, 2),
            stats("zoom.us", &[12.0, 13.0], 2, 2),
        ];
        let speed = SpeedStats {
            download_mbps: Some(0.2),
            upload_mbps: Some(0.1),
        };
        let v = build_verdict("8.8.8.8", &pings, Some(&speed));
        assert_eq!(v.label, VerdictLabel::You);
    }

    #[test]
    fn verdict_vendor_when_one_vendor_bad() {
        let pings = vec![
            stats("8.8.8.8", &[10.0, 11.0], 2, 2),
            stats("zoom.us", &[10.0, 11.0], 2, 2),
            stats("teams.microsoft.com", &[400.0, 420.0], 2, 2),
        ];
        let speed = SpeedStats {
            download_mbps: Some(50.0),
            upload_mbps: Some(10.0),
        };
        let v = build_verdict("8.8.8.8", &pings, Some(&speed));
        assert_eq!(v.label, VerdictLabel::Vendor);
        assert!(v.reason.contains("teams.microsoft.com"));
        assert_eq!(exit_code(v.label), 1);
    }

    #[test]
    fn verdict_clear_when_all_ok() {
        let pings = vec![
            stats("8.8.8.8", &[10.0, 11.0], 2, 2),
            stats("zoom.us", &[12.0, 13.0], 2, 2),
        ];
        let speed = SpeedStats {
            download_mbps: Some(50.0),
            upload_mbps: Some(10.0),
        };
        let v = build_verdict("8.8.8.8", &pings, Some(&speed));
        assert_eq!(v.label, VerdictLabel::Clear);
        assert_eq!(exit_code(v.label), 0);
    }

    #[test]
    fn verdict_clear_with_warnings() {
        let pings = vec![
            stats("8.8.8.8", &[10.0, 11.0], 2, 2),
            stats("zoom.us", &[200.0, 210.0], 2, 2),
        ];
        let speed = SpeedStats {
            download_mbps: Some(50.0),
            upload_mbps: Some(10.0),
        };
        let v = build_verdict("8.8.8.8", &pings, Some(&speed));
        assert_eq!(v.label, VerdictLabel::Clear);
        assert!(v.reason.contains("barely"));
    }

    #[test]
    fn verdict_clear_when_speed_skipped() {
        let pings = vec![
            stats("8.8.8.8", &[10.0, 11.0], 2, 2),
            stats("zoom.us", &[12.0, 13.0], 2, 2),
        ];
        let v = build_verdict("8.8.8.8", &pings, None);
        assert_eq!(v.label, VerdictLabel::Clear);
    }
}
