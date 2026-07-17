//! Rendering: progress bars, final table, colored verdict.

use owo_colors::OwoColorize;
use tabled::{Table, Tabled};
use tabled::settings::Style;

use crate::verdict::{PingStats, SpeedStats, Status, Verdict, VerdictLabel};

#[derive(Tabled)]
struct PingRow {
    target: String,
    min: String,
    avg: String,
    max: String,
    jitter: String,
    loss: String,
    status: String,
}

#[derive(Tabled)]
struct SpeedRow {
    direction: String,
    mbps: String,
    status: String,
}

fn fmt_ms(opt: Option<f64>) -> String {
    match opt {
        Some(v) => format!("{:.1}", v),
        None => "-".to_string(),
    }
}

fn fmt_mbps(opt: Option<f64>) -> String {
    match opt {
        Some(v) => format!("{:.1}", v),
        None => "-".to_string(),
    }
}

fn status_str(s: Status) -> String {
    match s {
        Status::Ok => "OK".to_string(),
        Status::Warn => "WARN".to_string(),
        Status::Bad => "BAD".to_string(),
    }
}

/// Build the ping table string.
pub fn ping_table(pings: &[PingStats]) -> String {
    let rows: Vec<PingRow> = pings
        .iter()
        .map(|p| PingRow {
            target: format!("{} ({})", p.target, crate::targets::label_for(&p.target)),
            min: fmt_ms(p.min_ms()),
            avg: fmt_ms(p.avg_ms()),
            max: fmt_ms(p.max_ms()),
            jitter: fmt_ms(p.jitter_ms()),
            loss: format!("{:.1}%", p.loss_pct()),
            status: status_str(p.overall()),
        })
        .collect();
    let mut t = Table::new(rows);
    t.with(Style::rounded());
    format!("{}", t)
}

/// Build the speed table string.
pub fn speed_table(speed: &SpeedStats) -> String {
    let rows = vec![
        SpeedRow {
            direction: "download".to_string(),
            mbps: fmt_mbps(speed.download_mbps),
            status: status_str(speed.download_status()),
        },
        SpeedRow {
            direction: "upload".to_string(),
            mbps: fmt_mbps(speed.upload_mbps),
            status: status_str(speed.upload_status()),
        },
    ];
    let mut t = Table::new(rows);
    t.with(Style::rounded());
    format!("{}", t)
}

/// Render the colored verdict line. Color is best-effort; the caller
/// decides whether to suppress via `--no-color` by not calling this.
pub fn verdict_line(verdict: &Verdict) -> String {
    let label = match verdict.label {
        VerdictLabel::You => "It's you.".red().bold().to_string(),
        VerdictLabel::Vendor => "It's them.".yellow().bold().to_string(),
        VerdictLabel::Clear => "All clear.".green().bold().to_string(),
    };
    format!("{} {}", label, verdict.reason.white())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verdict::PingStats;

    fn stats(target: &str, samples: &[f64], sent: u32, recv: u32) -> PingStats {
        PingStats {
            target: target.to_string(),
            samples_ms: samples.to_vec(),
            packets_sent: sent,
            packets_received: recv,
        }
    }

    #[test]
    fn fmt_ms_handles_some_and_none() {
        assert_eq!(fmt_ms(Some(12.34)), "12.3");
        assert_eq!(fmt_ms(None), "-");
    }

    #[test]
    fn fmt_mbps_handles_some_and_none() {
        assert_eq!(fmt_mbps(Some(50.67)), "50.7");
        assert_eq!(fmt_mbps(None), "-");
    }

    #[test]
    fn status_str_round_trips() {
        assert_eq!(status_str(Status::Ok), "OK");
        assert_eq!(status_str(Status::Warn), "WARN");
        assert_eq!(status_str(Status::Bad), "BAD");
    }

    #[test]
    fn ping_table_contains_target_and_loss() {
        let pings = vec![stats("8.8.8.8", &[10.0, 20.0], 2, 2)];
        let t = ping_table(&pings);
        assert!(t.contains("8.8.8.8"));
        assert!(t.contains("0.0%"));
    }

    #[test]
    fn speed_table_shows_down_and_up() {
        let s = SpeedStats {
            download_mbps: Some(50.0),
            upload_mbps: Some(10.0),
        };
        let t = speed_table(&s);
        assert!(t.contains("download"));
        assert!(t.contains("upload"));
        assert!(t.contains("50.0"));
        assert!(t.contains("10.0"));
    }

    #[test]
    fn verdict_line_contains_reason() {
        let v = Verdict {
            label: VerdictLabel::Clear,
            reason: "test reason here".to_string(),
        };
        let line = verdict_line(&v);
        // Strip ANSI for the assertion.
        let stripped = strip_ansi(&line);
        assert!(stripped.contains("All clear."));
        assert!(stripped.contains("test reason here"));
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut in_esc = false;
        for c in s.chars() {
            if c == '\x1b' {
                in_esc = true;
            } else if in_esc && c == 'm' {
                in_esc = false;
            } else if !in_esc {
                out.push(c);
            }
        }
        out
    }
}
