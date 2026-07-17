# isitme

Is it me? Diagnose whether your network is the problem when a call
(Zoom, MS Teams, Google Meet) is acting up.

`isitme` pings call vendor endpoints plus DNS baselines, runs a
Cloudflare-backed speed test, and prints a verdict:

- `All clear.` => your network is fine
- `It's them.` => a specific vendor endpoint is degraded
- `It's you.` => your baseline or speed test is bad

## Install

```sh
cargo install --path .
```

Requires the system `ping` binary (present on macOS and Linux).

## Usage

```sh
isitme                 # full run: ping all targets + speed test + verdict
isitme --ping-only     # skip the speed test
isitme --speed-only    # skip the ping phase
isitme --no-color      # disable colored output
isitme --json          # machine-readable JSON report
isitme --help          # full options
```

## Targets

Pinged in order:

| Host                  | Label                  | Role     |
|-----------------------|------------------------|----------|
| `8.8.8.8`             | Google DNS (baseline)  | baseline |
| `1.1.1.1`             | Cloudflare DNS         | probe    |
| `zoom.us`             | Zoom                   | vendor   |
| `teams.microsoft.com` | MS Teams               | vendor   |
| `meet.google.com`     | Google Meet            | vendor   |

Speed test runs against `https://speed.cloudflare.com/__down` and
`/__up` with 25 MB payloads in each direction.

## Thresholds

Hardcoded rules. Three-level status per metric: OK / WARN / BAD.

| Metric      | OK          | WARN        | BAD          |
|-------------|-------------|-------------|--------------|
| ping avg    | < 150 ms    | 150-300 ms  | > 300 ms     |
| jitter      | < 30 ms     | 30-50 ms    | > 50 ms      |
| packet loss | < 1%        | 1-4%        | >= 5%        |
| download    | > 3 Mbps    | 1-3 Mbps    | < 1 Mbps     |
| upload      | > 1.5 Mbps  | 0.5-1.5     | < 0.5 Mbps   |

## Verdict logic

1. If the baseline (`8.8.8.8`) is BAD, or the speed test is BAD:
   `It's you.` Your network is the likely cause.
2. Else if any vendor target is BAD while the baseline is fine:
   `It's them.` The degraded vendor's side.
3. Else: `All clear.`

## Exit codes

For scripting:

| Code | Meaning                                  |
|------|------------------------------------------|
| 0    | all clear (no WARN, no BAD)              |
| 1    | warnings present, or a vendor is BAD     |
| 2    | BAD present on your side (you)           |

## JSON output

`isitme --json` emits a pretty-printed report:

```json
{
  "pings": [
    {
      "target": "8.8.8.8",
      "samples_ms": [12.3, 13.4, 11.2],
      "packets_sent": 8,
      "packets_received": 8
    }
  ],
  "speed": {
    "download_mbps": 50.3,
    "upload_mbps": 10.7
  },
  "verdict": {
    "label": "clear",
    "reason": "All metrics within OK thresholds. Not you."
  }
}
```

`verdict.label` is one of `you`, `vendor`, `clear`.

## Development

```sh
cargo test                              # 42 unit tests, pure logic
cargo clippy --all-targets -- -D warnings
cargo build --release
```

Pure logic (thresholds, ping output parsing, throughput math) is
unit-tested with fixture strings. Network and process-spawn code is a
thin wrapper around the tested pure functions.

## License

MIT
