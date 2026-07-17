//! isitme: is it me, or is it them? Diagnose network quality for calls.

mod targets;
mod verdict;
mod ping;
mod speed;
mod render;

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;

use ping::{run_ping, PING_COUNT};
use render::{ping_table, speed_table, verdict_line};
use speed::run_speed_test;
use targets::{hosts, is_baseline, label_for, BASELINE_HOST};
use verdict::{build_verdict, exit_code, PingStats, SpeedStats, Verdict, VerdictLabel};

#[derive(Parser, Debug)]
#[command(
    name = "isitme",
    version,
    about = "Is it me? Diagnose whether your network is the problem in a call.",
    long_about = "Pings call vendor endpoints (Zoom, Teams, Meet) plus DNS baselines, \
runs a Cloudflare-backed speed test, and prints a verdict."
)]
struct Cli {
    /// Skip the speed test (ping only).
    #[arg(long)]
    ping_only: bool,

    /// Skip the ping phase (speed test only).
    #[arg(long)]
    speed_only: bool,

    /// Disable colored output.
    #[arg(long)]
    no_color: bool,

    /// Emit machine-readable JSON instead of the TUI table.
    #[arg(long)]
    json: bool,
}

#[derive(Serialize)]
struct Report {
    pings: Vec<PingStats>,
    speed: Option<SpeedStats>,
    verdict: Verdict,
}

fn verdict_plain_label(verdict: &Verdict) -> &'static str {
    match verdict.label {
        VerdictLabel::You => "It's you.",
        VerdictLabel::Vendor => "It's them.",
        VerdictLabel::Clear => "All clear.",
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    owo_colors::set_override(!cli.no_color);

    let pings = if cli.speed_only {
        Vec::new()
    } else {
        run_all_pings()
    };

    let speed = if cli.ping_only {
        None
    } else {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;
        Some(run_speed_test(&client).await)
    };

    let verdict = build_verdict(BASELINE_HOST, &pings, speed.as_ref());

    if cli.json {
        let report = Report {
            pings,
            speed,
            verdict: verdict.clone(),
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        if !pings.is_empty() {
            println!("\n{}", ping_table(&pings));
        }
        if let Some(s) = &speed {
            println!("\n{}", speed_table(s));
        }
        if !cli.no_color {
            println!("\n{}", verdict_line(&verdict));
        } else {
            println!("\n{} {}", verdict_plain_label(&verdict), verdict.reason);
        }
    }

    std::process::exit(exit_code(verdict.label));
}

fn run_all_pings() -> Vec<PingStats> {
    let targets = hosts();
    let bar = ProgressBar::new(targets.len() as u64);
    bar.set_style(
        ProgressStyle::with_template("{spinner} pinging {msg} ({pos}/{len})")
            .unwrap(),
    );

    let mut results = Vec::with_capacity(targets.len());
    for host in targets {
        let tag = if is_baseline(host) { " [baseline]" } else { "" };
        bar.set_message(format!("{}{}", label_for(host), tag));
        let stats = match run_ping(host, PING_COUNT) {
            Ok(s) => s,
            Err(_) => PingStats {
                target: host.to_string(),
                samples_ms: Vec::new(),
                packets_sent: PING_COUNT,
                packets_received: 0,
            },
        };
        results.push(stats);
        bar.inc(1);
    }
    bar.finish_and_clear();
    results
}
