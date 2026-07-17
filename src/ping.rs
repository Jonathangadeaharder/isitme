//! Ping runner. Spawns system `ping` and parses RTT replies.
//!
//! The I/O surface (spawn, collect stdout) is a thin wrapper around the
//! pure parser `parse_ping_output`. The parser is unit-tested with
//! fixture strings from macOS BSD ping and Linux iputils ping.

use std::process::Command;

use crate::verdict::PingStats;

/// Number of ICMP probes per target. Three keeps it fast (default 1s
/// interval => ~2s per target, run in parallel) while still giving
/// min/avg/max and a rough jitter reading.
pub const PING_COUNT: u32 = 3;

/// Spawn ping for `host`, return parsed stats.
pub fn run_ping(host: &str, count: u32) -> std::io::Result<PingStats> {
    let output = Command::new("ping")
        .arg("-c")
        .arg(count.to_string())
        .arg("-W")
        .arg("2000")
        .arg(host)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok(parse_ping_output(host, count, &stdout, &stderr))
}

/// Async wrapper that runs `run_ping` on a blocking thread so many
/// targets can be pinged concurrently via `tokio::join!` / `join_all`.
pub async fn run_ping_async(host: String, count: u32) -> PingStats {
    let outer_fallback = host.clone();
    let inner_fallback = host.clone();
    let inner = tokio::task::spawn_blocking(move || {
        run_ping(&host, count).unwrap_or_else(|_| PingStats {
            target: inner_fallback,
            samples_ms: Vec::new(),
            packets_sent: count,
            packets_received: 0,
        })
    });
    inner
        .await
        .unwrap_or_else(|_| PingStats {
            target: outer_fallback,
            samples_ms: Vec::new(),
            packets_sent: count,
            packets_received: 0,
        })
}

/// Parse ping stdout into PingStats.
///
/// Handles two output dialects:
/// - macOS BSD: `64 bytes from 8.8.8.8: icmp_seq=0 ttl=119 time=12.345 ms`
/// - Linux iputils: `64 bytes from 8.8.8.8: icmp_seq=1 ttl=119 time=12.3 ms`
///
/// Both share the `time=<float> ms` token, which is what we extract.
pub fn parse_ping_output(host: &str, sent: u32, stdout: &str, _stderr: &str) -> PingStats {
    let samples = extract_rtts(stdout);
    let received = samples.len() as u32;
    PingStats {
        target: host.to_string(),
        samples_ms: samples,
        packets_sent: sent,
        packets_received: received,
    }
}

/// Extract every `time=<float>` value from the output, in order.
pub fn extract_rtts(stdout: &str) -> Vec<f64> {
    stdout
        .lines()
        .filter_map(extract_time_from_line)
        .collect()
}

/// Pull the `time=X` value out of a single reply line.
fn extract_time_from_line(line: &str) -> Option<f64> {
    let idx = line.find("time=")?;
    let rest = &line[idx + "time=".len()..];
    let num_str: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    num_str.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const MACOS_OUTPUT: &str = "PING 8.8.8.8 (8.8.8.8): 56 data bytes\n\
64 bytes from 8.8.8.8: icmp_seq=0 ttl=119 time=12.345 ms\n\
64 bytes from 8.8.8.8: icmp_seq=1 ttl=119 time=13.456 ms\n\
64 bytes from 8.8.8.8: icmp_seq=2 ttl=119 time=11.234 ms\n\
64 bytes from 8.8.8.8: icmp_seq=3 ttl=119 time=14.567 ms\n\n\
--- 8.8.8.8 ping statistics ---\n\
4 packets transmitted, 4 packets received, 0.0% packet loss\n\
round-trip min/avg/max/stddev = 11.234/12.900/14.567/1.234 ms\n";

    const LINUX_OUTPUT: &str = "PING 8.8.8.8 (8.8.8.8) 56(84) bytes of data.\n\
64 bytes from 8.8.8.8: icmp_seq=1 ttl=119 time=12.3 ms\n\
64 bytes from 8.8.8.8: icmp_seq=2 ttl=119 time=13.4 ms\n\
64 bytes from 8.8.8.8: icmp_seq=3 ttl=119 time=11.2 ms\n\n\
--- 8.8.8.8 ping statistics ---\n\
3 packets transmitted, 3 received, 0% packet loss, time 2003ms\n\
rtt min/avg/max/mdev = 11.200/12.300/13.400/0.900 ms\n";

    #[test]
    fn parses_macos_rtts_in_order() {
        let rtts = extract_rtts(MACOS_OUTPUT);
        assert_eq!(rtts, vec![12.345, 13.456, 11.234, 14.567]);
    }

    #[test]
    fn parses_linux_rtts_in_order() {
        let rtts = extract_rtts(LINUX_OUTPUT);
        assert_eq!(rtts, vec![12.3, 13.4, 11.2]);
    }

    fn approx(a: Option<f64>, b: f64) -> bool {
        match a {
            Some(v) => (v - b).abs() < 1e-3,
            None => false,
        }
    }

    #[test]
    fn macos_stats_have_no_loss() {
        let s = parse_ping_output("8.8.8.8", 4, MACOS_OUTPUT, "");
        assert_eq!(s.target, "8.8.8.8");
        assert_eq!(s.packets_sent, 4);
        assert_eq!(s.packets_received, 4);
        assert_eq!(s.loss_pct(), 0.0);
        assert!(approx(s.avg_ms(), 12.9));
    }

    #[test]
    fn linux_stats_have_no_loss() {
        let s = parse_ping_output("8.8.8.8", 3, LINUX_OUTPUT, "");
        assert_eq!(s.packets_sent, 3);
        assert_eq!(s.packets_received, 3);
        assert_eq!(s.loss_pct(), 0.0);
        assert!(approx(s.avg_ms(), 12.3));
    }

    #[test]
    fn packet_loss_when_some_replies_missing() {
        let partial = "PING x (1.2.3.4): 56 data bytes\n\
64 bytes from 1.2.3.4: icmp_seq=0 ttl=64 time=10.000 ms\n\
64 bytes from 1.2.3.4: icmp_seq=2 ttl=64 time=11.000 ms\n";
        let s = parse_ping_output("x", 4, partial, "");
        assert_eq!(s.packets_sent, 4);
        assert_eq!(s.packets_received, 2);
        assert_eq!(s.loss_pct(), 50.0);
    }

    #[test]
    fn empty_output_yields_no_samples() {
        let s = parse_ping_output("x", 4, "", "");
        assert!(s.samples_ms.is_empty());
        assert_eq!(s.packets_sent, 4);
        assert_eq!(s.packets_received, 0);
        assert_eq!(s.loss_pct(), 100.0);
    }

    #[test]
    fn ignores_summary_line_time_field() {
        // The summary line "time 2003ms" must NOT be parsed as an RTT,
        // because it lacks the "time=" token form.
        let summary_only = "--- 8.8.8.8 ping statistics ---\n\
3 packets transmitted, 3 received, 0% packet loss, time 2003ms\n";
        let rtts = extract_rtts(summary_only);
        assert!(rtts.is_empty(), "got {:?}", rtts);
    }

    #[test]
    fn handles_decimal_with_one_digit() {
        let line = "64 bytes from 1.2.3.4: icmp_seq=0 ttl=64 time=5.1 ms";
        assert_eq!(extract_time_from_line(line), Some(5.1));
    }

    #[test]
    fn handles_integer_time() {
        let line = "64 bytes from 1.2.3.4: icmp_seq=0 ttl=64 time=7 ms";
        assert_eq!(extract_time_from_line(line), Some(7.0));
    }
}
