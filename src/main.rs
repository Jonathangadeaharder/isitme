//! isitme: is it me, or is it them? Diagnose network quality for calls.

mod ping;
mod render;
mod speed;
mod targets;
mod verdict;
mod version;

use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
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
    #[command(subcommand)]
    command: Option<Command>,

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

#[derive(Subcommand, Debug)]
enum Command {
    /// Install the latest published version from crates.io.
    Update,
}

#[derive(Serialize)]
struct Report {
    pings: Vec<PingStats>,
    speed: Option<SpeedStats>,
    verdict: Verdict,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_version: Option<String>,
}

fn worst_status(pings: &[PingStats], speed: Option<&SpeedStats>) -> Status {
    let mut worst = speed.map(|s| s.overall()).unwrap_or(Status::Ok);
    for p in pings {
        worst = Status::worst_of(&[worst, p.overall()]);
    }
    worst
}

/// Shared HTTP client. User-Agent is required by crates.io or it 403s.
fn build_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(format!(
            "isitme/{} (https://github.com/Jonathangadeaharder/isitme)",
            version::current_version()
        ))
        .build()?)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // `isitme update` short-circuits: no diagnostic, just reinstall.
    if let Some(Command::Update) = cli.command {
        return run_update().await;
    }

    owo_colors::set_override(!cli.no_color);

    let client = build_client()?;

    // Run pings, speed test, and version check concurrently. The
    // version check is 24h-cached and 2s-timeout-capped so it never
    // adds latency or blocks on a flaky network.
    let pings_fut = run_all_pings_parallel();
    let speed_fut = async {
        if cli.ping_only {
            None
        } else {
            Some(run_speed_test(&client).await)
        }
    };
    let version_fut = version::check_for_update(&client);

    let (pings, speed, latest_version) = if cli.speed_only {
        let s = speed_fut.await;
        let v = version_fut.await;
        (Vec::new(), s, v)
    } else {
        tokio::join!(pings_fut, speed_fut, version_fut)
    };

    let verdict = build_verdict(BASELINE_HOST, &pings, speed.as_ref());

    if cli.json {
        let report = Report {
            pings,
            speed,
            verdict: verdict.clone(),
            latest_version,
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

        // Update notice: one muted line, last, so it never steals
        // focus from the verdict.
        if let Some(latest) = &latest_version {
            use owo_colors::OwoColorize;
            println!(
                "\n  {} {} {} {}",
                "isitme".dimmed(),
                latest.dimmed().bold(),
                format!("(you're on {})", version::current_version()).dimmed(),
                "Run: isitme update".cyan(),
            );
        }
    }

    std::process::exit(exit_code(verdict.label));
}

/// `isitme update`: reinstall from crates.io via cargo. cargo itself
/// decides whether a rebuild is needed (no-op if already latest).
async fn run_update() -> Result<()> {
    println!("Updating isitme via cargo...");
    let status = tokio::process::Command::new("cargo")
        .args(["install", "isitme"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Ping all targets concurrently. Each target spawns its own ping
/// process on a blocking thread; results are reassembled in declared
/// target order so the table output is stable across runs. A spinner
/// ticks during the wait so the user sees live progress.
async fn run_all_pings_parallel() -> Vec<PingStats> {
    let targets = hosts();
    let bar = ProgressBar::new(targets.len() as u64);
    bar.set_style(
        ProgressStyle::with_template("{spinner} pinging {pos}/{len} targets ({msg})").unwrap(),
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
    results
        .into_iter()
        .map(|o| {
            o.unwrap_or(PingStats {
                target: String::new(),
                samples_ms: Vec::new(),
                packets_sent: PING_COUNT,
                packets_received: 0,
            })
        })
        .collect()
}
