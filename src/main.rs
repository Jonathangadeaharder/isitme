//! isitme: is it me, or is it them? Diagnose network quality for calls.

mod ping;
mod render;
mod speed;
mod targets;
mod verdict;

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use tokio::task::JoinSet;

use ping::{run_ping_async, PING_COUNT};
use render::{diagnostic_table, verdict_banner};
use speed::run_speed_test;
use targets::{hosts, BASELINE_HOST};
use verdict::{build_verdict, exit_code, PingStats, SpeedStats, Status, Verdict};

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

fn worst_status(pings: &[PingStats], speed: Option<&SpeedStats>) -> Status {
    let mut worst = speed
        .map(|s| s.overall())
        .unwrap_or(Status::Ok);
    for p in pings {
        worst = Status::worst_of(&[worst, p.overall()]);
    }
    worst
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    owo_colors::set_override(!cli.no_color);

    // Run pings and speed test concurrently. Pings fan out across all
    // targets in parallel; the speed test runs in the same window.
    let pings_fut = run_all_pings_parallel();
    let speed_fut = async {
        if cli.ping_only {
            None
        } else {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .ok()?;
            Some(run_speed_test(&client).await)
        }
    };

    let (pings, speed) = if cli.speed_only {
        (Vec::new(), speed_fut.await)
    } else {
        tokio::join!(pings_fut, speed_fut)
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
        // Banner first: the answer to "is it me?" is the headline.
        let worst = worst_status(&pings, speed.as_ref());
        println!("\n{}", verdict_banner(&verdict, worst));

        // One compact table below.
        if !pings.is_empty() {
            println!("{}", diagnostic_table(&pings, speed.as_ref()));
        }
    }

    std::process::exit(exit_code(verdict.label));
}

/// Ping all targets concurrently. Each target spawns its own ping
/// process on a blocking thread; results are reassembled in declared
/// target order so the table output is stable across runs. A spinner
/// ticks during the wait so the user sees live progress.
async fn run_all_pings_parallel() -> Vec<PingStats> {
    let targets = hosts();
    let bar = ProgressBar::new(targets.len() as u64);
    bar.set_style(
        ProgressStyle::with_template("{spinner} pinging {pos}/{len} targets ({msg})")
            .unwrap(),
    );
    bar.enable_steady_tick(Duration::from_millis(80));

    let mut set: JoinSet<(usize, PingStats)> = JoinSet::new();
    for (i, host) in targets.iter().enumerate() {
        let host = host.to_string();
        set.spawn(async move {
            let stats = run_ping_async(host, PING_COUNT).await;
            (i, stats)
        });
    }
    let mut results: Vec<Option<PingStats>> = (0..targets.len()).map(|_| None).collect();
    while let Some(res) = set.join_next().await {
        if let Ok((i, stats)) = res {
            let label = crate::targets::label_for(&stats.target).to_string();
            results[i] = Some(stats);
            bar.inc(1);
            bar.set_message(label);
        }
    }
    bar.finish_and_clear();
    results.into_iter().map(|o| o.unwrap_or(PingStats {
        target: String::new(),
        samples_ms: Vec::new(),
        packets_sent: PING_COUNT,
        packets_received: 0,
    })).collect()
}
