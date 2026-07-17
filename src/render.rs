//! Rendering: progress bars, final table, colored verdict.
//!
//! Tables are hand-rolled to avoid the `tabled` proc-macro chain, which
//! transitively pulls in `proc-macro-error2` (archived upstream, emits a
//! future-compat warning). Our schema is fixed, so a tiny formatter
//! suffices.

use owo_colors::OwoColorize;

use crate::verdict::{PingStats, SpeedStats, Status, Verdict, VerdictLabel};

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

/// Render a fixed-schema table with rounded Unicode box borders.
///
/// `headers` and every row must have the same length. Cells are
/// left-aligned and single-space-padded. No inter-row separators.
fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let cols = headers.len();
    let widths: Vec<usize> = (0..cols)
        .map(|c| {
            let header_w = headers[c].chars().count();
            let row_w = rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0);
            header_w.max(row_w)
        })
        .collect();

    let pad = |c: &str, w: usize| -> String {
        let len = c.chars().count();
        let mut s = String::with_capacity(w + 2);
        s.push(' ');
        s.push_str(c);
        s.push_str(&" ".repeat(w.saturating_sub(len)));
        s.push(' ');
        s
    };

    let mut out = String::new();

    // Top border: ╭─...─┬─...─╮
    out.push('╭');
    out.push_str(
        &widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┬"),
    );
    out.push('╮');
    out.push('\n');

    // Header row.
    out.push('│');
    out.push_str(
        &headers
            .iter()
            .zip(widths.iter())
            .map(|(h, w)| pad(h, *w))
            .collect::<Vec<_>>()
            .join("│"),
    );
    out.push('│');
    out.push('\n');

    // Header separator: ├─...─┼─...─┤
    out.push('├');
    out.push_str(
        &widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┼"),
    );
    out.push('┤');
    out.push('\n');

    // Data rows.
    for row in rows {
        out.push('│');
        out.push_str(
            &row.iter()
                .zip(widths.iter())
                .map(|(cell, w)| pad(cell, *w))
                .collect::<Vec<_>>()
                .join("│"),
        );
        out.push('│');
        out.push('\n');
    }

    // Bottom border: ╰─...─┴─...─╯
    out.push('╰');
    out.push_str(
        &widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┴"),
    );
    out.push('╯');

    out
}

/// Build the ping table string.
pub fn ping_table(pings: &[PingStats]) -> String {
    let headers = ["target", "min", "avg", "max", "jitter", "loss", "status"];
    let rows: Vec<Vec<String>> = pings
        .iter()
        .map(|p| {
            vec![
                format!("{} ({})", p.target, crate::targets::label_for(&p.target)),
                fmt_ms(p.min_ms()),
                fmt_ms(p.avg_ms()),
                fmt_ms(p.max_ms()),
                fmt_ms(p.jitter_ms()),
                format!("{:.1}%", p.loss_pct()),
                status_str(p.overall()),
            ]
        })
        .collect();
    render_table(&headers, &rows)
}

/// Build the speed table string.
pub fn speed_table(speed: &SpeedStats) -> String {
    let headers = ["direction", "mbps", "status"];
    let rows = vec![
        vec![
            "download".to_string(),
            fmt_mbps(speed.download_mbps),
            status_str(speed.download_status()),
        ],
        vec![
            "upload".to_string(),
            fmt_mbps(speed.upload_mbps),
            status_str(speed.upload_status()),
        ],
    ];
    render_table(&headers, &rows)
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
    use pretty_assertions::assert_eq;
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
    fn render_table_empty_rows_has_header_only() {
        let t = render_table(&["a", "b"], &[]);
        assert!(t.contains('╭'));
        assert!(t.contains('╯'));
        assert!(t.contains("│ a │ b │"));
    }

    #[test]
    fn render_table_aligns_to_widest_cell() {
        let t = render_table(&["h", "name"], &[vec!["x".to_string(), "longvalue".to_string()]]);
        // Second column width = max("name", "longvalue") = "longvalue" (9).
        assert!(t.contains("│ longvalue │"));
        // Header padded to same width.
        assert!(t.contains("│ name      │"));
    }

    #[test]
    fn render_table_handles_unicode_width() {
        // Multibyte char counts as 1 char, consistent with our counting.
        let t = render_table(&["h"], &[vec!["é".to_string()]]);
        assert!(t.contains("│ é │"));
    }

    #[test]
    fn ping_table_contains_target_and_loss() {
        let pings = vec![stats("8.8.8.8", &[10.0, 20.0], 2, 2)];
        let t = ping_table(&pings);
        assert!(t.contains("8.8.8.8"));
        assert!(t.contains("0.0%"));
        assert!(t.contains("OK"));
    }

    #[test]
    fn ping_table_has_all_seven_headers() {
        let pings = vec![stats("8.8.8.8", &[10.0, 20.0], 2, 2)];
        let t = ping_table(&pings);
        for h in ["target", "min", "avg", "max", "jitter", "loss", "status"] {
            assert!(t.contains(h), "missing header {}", h);
        }
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
