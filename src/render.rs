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

/// One compact diagnostic table: per-target latency (avg, jitter, loss)
/// with speed folded in as a final row. Min/max are dropped; avg is the
/// only latency number that matters for a quick gut check during a call.
pub fn diagnostic_table(pings: &[PingStats], speed: Option<&SpeedStats>) -> String {
    let headers = ["target", "latency", "jitter", "loss", "speed"];
    let mut rows: Vec<Vec<String>> = pings
        .iter()
        .map(|p| {
            vec![
                crate::targets::label_for(&p.target).to_string(),
                fmt_ms(p.avg_ms()),
                fmt_ms(p.jitter_ms()),
                format!("{:.0}%", p.loss_pct()),
                "-".to_string(),
            ]
        })
        .collect();
    if let Some(s) = speed {
        rows.push(vec![
            "Internet (CF)".to_string(),
            "-".to_string(),
            "-".to_string(),
            "-".to_string(),
            format!(
                "{} ↓ / {} ↑",
                fmt_mbps(s.download_mbps),
                fmt_mbps(s.upload_mbps),
            ),
        ]);
    }
    render_table(&headers, &rows)
}

/// Big prominent banner: giant ASCII-art verdict headline, the funny
/// one-liner reason below it. `worst` shades the Clear case green (all
/// OK) vs yellow (usable with warnings).
///
/// The art is hand-authored (not figlet) so there is no font-file dep
/// and the output stays pipe-safe. Each glyph is 5 rows tall.
pub fn verdict_banner(verdict: &Verdict, worst: Status) -> String {
    use VerdictLabel::*;
    let (art, color): (&[&str], fn(&str) -> String) = match verdict.label {
        You => (ART_IT_IS_YOU, |s: &str| s.red().bold().to_string()),
        Vendor => (ART_IT_IS_THEM, |s: &str| s.yellow().bold().to_string()),
        Clear => match worst {
            Status::Ok => (ART_IT_IS_NOT_YOU, |s: &str| s.green().bold().to_string()),
            _ => (ART_IT_IS_NOT_YOU, |s: &str| s.yellow().bold().to_string()),
        },
    };
    let colored: String = art
        .iter()
        .map(|line| format!("{}\n", color(line)))
        .collect();
    format!("{}\n  {}\n", colored, verdict.reason)
}

// 5-row block font, glyph width 4 (space-separated). Compact enough
// for a 60-column terminal. Hand-authored to avoid a figlet dep.
const ART_IT_IS_YOU: &[&str] = &[
    "██╗ ██████╗ ██╗  ██╗",
    "██║██╔═══██╗██║  ██║",
    "██║██║   ██║███████║",
    "██║██║   ██║╚██╔══██║",
    "╚═╝╚██████╔╝ ╚═╝ ╚═╝",
    "     ╚═════╝        ",
];

const ART_IT_IS_NOT_YOU: &[&str] = &[
    "██╗ ██████╗ ██╗  ██╗ ███╗   ██╗ ██████╗ ██╗  ██╗",
    "██║██╔═══██╗██║  ██║ ████╗  ██║██╔═══██╗██║  ██║",
    "██║██║   ██║███████║ ██╔██╗ ██║██║   ██║███████║",
    "██║██║   ██║╚██╔══██║ ██║╚██╗██║██║   ██║╚██╔══██║",
    "╚═╝╚██████╔╝ ╚═╝ ╚═╝ ██║ ╚████║╚██████╔╝ ╚═╝ ╚═╝",
    "     ╚═════╝         ╚═╝  ╚═══╝ ╚═════╝        ",
];

const ART_IT_IS_THEM: &[&str] = &[
    "████████╗██╗  ██╗███████╗███╗   ███╗",
    "╚══██╔══╝██║  ██║██╔════╝████╗ ████║",
    "   ██║   ███████║█████╗  ██╔████╔██║",
    "   ██║   ██╔══██║██╔══╝  ██║╚██╔╝██║",
    "   ╚═╝   ╚═╝  ╚═╝███████╗╚═╝ ╚═╝╚═╝",
    "                 ╚══════╝          ",
];

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
    fn diagnostic_table_shows_label_and_loss() {
        let pings = vec![stats("8.8.8.8", &[10.0, 20.0], 2, 2)];
        let t = diagnostic_table(&pings, None);
        // Label resolves to the human form, not the raw host.
        assert!(t.contains("Google DNS (baseline)"));
        assert!(t.contains("0%"));
    }

    #[test]
    fn diagnostic_table_has_five_headers() {
        let pings = vec![stats("8.8.8.8", &[10.0, 20.0], 2, 2)];
        let t = diagnostic_table(&pings, None);
        for h in ["target", "latency", "jitter", "loss", "speed"] {
            assert!(t.contains(h), "missing header {}", h);
        }
    }

    #[test]
    fn diagnostic_table_appends_speed_row_when_given() {
        let pings = vec![stats("zoom.us", &[10.0, 20.0], 2, 2)];
        let s = SpeedStats {
            download_mbps: Some(50.0),
            upload_mbps: Some(10.0),
        };
        let t = diagnostic_table(&pings, Some(&s));
        assert!(t.contains("Internet (CF)"));
        assert!(t.contains("50.0"));
        assert!(t.contains("10.0"));
        assert!(t.contains("↓"));
        assert!(t.contains("↑"));
    }

    #[test]
    fn diagnostic_table_speed_dash_when_no_speed() {
        let pings = vec![stats("8.8.8.8", &[10.0, 20.0], 2, 2)];
        let t = diagnostic_table(&pings, None);
        // No speed row appended.
        assert!(!t.contains("Internet (CF)"));
    }

    #[test]
    fn verdict_banner_shows_headline_words_and_reason() {
        let v = Verdict {
            label: VerdictLabel::Clear,
            reason: "test reason here".to_string(),
        };
        let banner = verdict_banner(&v, Status::Ok);
        let stripped = strip_ansi(&banner);
        // Banner has block-art (not literal letters) plus the reason.
        assert!(stripped.contains("█"));
        assert!(stripped.contains("test reason here"));
    }

    #[test]
    fn verdict_banner_you_when_label_you() {
        let v = Verdict {
            label: VerdictLabel::You,
            reason: "your network".to_string(),
        };
        let banner = verdict_banner(&v, Status::Bad);
        let stripped = strip_ansi(&banner);
        assert!(stripped.contains("█"));
        assert!(stripped.contains("your network"));
    }

    #[test]
    fn verdict_banner_vendor_when_label_vendor() {
        let v = Verdict {
            label: VerdictLabel::Vendor,
            reason: "zoom down".to_string(),
        };
        let banner = verdict_banner(&v, Status::Bad);
        let stripped = strip_ansi(&banner);
        assert!(stripped.contains("█"));
        assert!(stripped.contains("zoom down"));
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
