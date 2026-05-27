use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use raft_log::Callback;
use raft_log::Config;
use raft_log::FlushMetrics;
use raft_log::RaftLog;
use raft_log::Types;
use raft_log::api::raft_log_writer::RaftLogWriter;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct BenchTypes;

impl Types for BenchTypes {
    type LogId = (u64, u64);
    type LogPayload = Vec<u8>;
    type Vote = (u64, u64);
    type Callback = BenchCallback;
    type UserData = String;

    fn log_index(log_id: &Self::LogId) -> u64 {
        log_id.1
    }

    fn payload_size(payload: &Self::LogPayload) -> u64 {
        payload.len() as u64
    }
}

struct BenchCallback {
    id: usize,
    submitted_at: Instant,
    tx: Sender<CallbackEvent>,
}

impl Callback for BenchCallback {
    fn send(self, res: Result<(), io::Error>) {
        let event = CallbackEvent {
            id: self.id,
            latency: self.submitted_at.elapsed(),
            error: res.err().map(|e| e.to_string()),
        };
        let _ = self.tx.send(event);
    }
}

struct CallbackEvent {
    id: usize,
    latency: Duration,
    error: Option<String>,
}

struct Options {
    operations: usize,
    payload_bytes: usize,
    wait_us: Vec<u64>,
    batch_sizes: Vec<usize>,
    output: PathBuf,
}

struct CaseResult {
    name: String,
    wait: Duration,
    batch_size: usize,
    operations: usize,
    payload_bytes: usize,
    elapsed: Duration,
    latencies: Vec<Duration>,
    metrics: FlushMetrics,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::parse()?;

    let mut results = Vec::new();
    for batch_size in &options.batch_sizes {
        for wait_us in &options.wait_us {
            let wait = Duration::from_micros(*wait_us);
            let name = format!("wait_{}us_batch_{}", wait_us, batch_size);
            println!("running {name}");
            results.push(run_case(
                name,
                wait,
                *batch_size,
                options.operations,
                options.payload_bytes,
            )?);
        }
    }

    let report = render_report(&options, &results);

    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&options.output, report.as_bytes())?;

    println!("{report}");
    println!("report: {}", options.output.display());

    Ok(())
}

impl Options {
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut operations = 2048;
        let mut payload_bytes = 256;
        let mut wait_us = vec![0, 100, 1000, 2000, 5000];
        let mut batch_sizes = vec![1024];
        let mut output = PathBuf::from("target/flush_batch_bench_report.md");

        let args = env::args().skip(1).collect::<Vec<_>>();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--operations" => {
                    i += 1;
                    operations = parse_arg(&args, i, "--operations")?;
                }
                "--payload-bytes" => {
                    i += 1;
                    payload_bytes = parse_arg(&args, i, "--payload-bytes")?;
                }
                "--wait-us" => {
                    i += 1;
                    let raw =
                        args.get(i).ok_or_else(|| missing_arg("--wait-us"))?;
                    wait_us = raw
                        .split(',')
                        .map(str::parse::<u64>)
                        .collect::<Result<Vec<_>, _>>()?;
                }
                "--batch-size" | "--batch-sizes" => {
                    i += 1;
                    let raw = args
                        .get(i)
                        .ok_or_else(|| missing_arg("--batch-size"))?;
                    batch_sizes = raw
                        .split(',')
                        .map(str::parse::<usize>)
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .map(|v| v.max(1))
                        .collect();
                }
                "--output" => {
                    i += 1;
                    output = args
                        .get(i)
                        .ok_or_else(|| missing_arg("--output"))?
                        .into();
                }
                "--help" | "-h" => {
                    println!(
                        "usage: cargo run --release --example flush_batch_bench -- \
                         [--operations N] [--payload-bytes N] \
                         [--wait-us 0,100,1000,2000,5000] \
                         [--batch-size 512,1024,2048] [--output PATH]"
                    );
                    std::process::exit(0);
                }
                other => {
                    return Err(format!("unknown argument: {other}").into());
                }
            }
            i += 1;
        }

        Ok(Self {
            operations,
            payload_bytes,
            wait_us,
            batch_sizes,
            output,
        })
    }
}

fn parse_arg<T>(
    args: &[String],
    index: usize,
    name: &str,
) -> Result<T, Box<dyn std::error::Error>>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + 'static,
{
    args.get(index)
        .ok_or_else(|| missing_arg(name))?
        .parse::<T>()
        .map_err(Into::into)
}

fn missing_arg(name: &str) -> Box<dyn std::error::Error> {
    format!("missing value for {name}").into()
}

fn run_case(
    name: String,
    wait: Duration,
    batch_size: usize,
    operations: usize,
    payload_bytes: usize,
) -> Result<CaseResult, Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let config = Arc::new(Config {
        wal: chunked_wal::Config {
            dir: temp.path().to_string_lossy().to_string(),
            chunk_max_records: Some(1024 * 1024),
            chunk_max_size: Some(1024 * 1024 * 1024),
            flush_batch_wait: Some(wait),
            flush_batch_max_items: Some(batch_size),
            ..Default::default()
        },
        log_cache_max_items: Some(1024 * 1024),
        log_cache_capacity: Some(1024 * 1024 * 1024),
    });

    let mut log = RaftLog::<BenchTypes>::open(config)?;
    let (tx, rx) = std::sync::mpsc::channel::<CallbackEvent>();
    let payload = vec![b'x'; payload_bytes];
    let start = Instant::now();

    for i in 0..operations {
        let index = i as u64;
        log.append([((1, index), payload.clone())])?;
        let cb = BenchCallback {
            id: i,
            submitted_at: Instant::now(),
            tx: tx.clone(),
        };
        log.flush(true, Some(cb))?;
    }
    drop(tx);

    let mut latencies = Vec::with_capacity(operations);
    let mut seen = vec![false; operations];
    for _ in 0..operations {
        let event = rx.recv()?;
        if let Some(error) = event.error {
            return Err(io::Error::other(error).into());
        }
        if event.id >= operations {
            return Err(
                format!("callback id out of range: {}", event.id).into()
            );
        }
        if seen[event.id] {
            return Err(format!("duplicate callback id: {}", event.id).into());
        }
        seen[event.id] = true;
        latencies.push(event.latency);
    }

    log.wait_worker_idle()?;
    let elapsed = start.elapsed();
    let metrics = log.stat().flush_metrics;

    Ok(CaseResult {
        name,
        wait,
        batch_size,
        operations,
        payload_bytes,
        elapsed,
        latencies,
        metrics,
    })
}

fn render_report(options: &Options, results: &[CaseResult]) -> String {
    let mut report = String::new();
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    report.push_str("# WAL Flush Batch Benchmark Report\n\n");
    report.push_str(&format!("- Generated at Unix time: {generated_at}\n"));
    report.push_str(&format!("- Build profile: {profile}\n"));
    report
        .push_str(&format!("- Operations per case: {}\n", options.operations));
    report.push_str(&format!("- Payload bytes: {}\n", options.payload_bytes));
    report.push_str(&format!("- Wait windows: {:?} us\n", options.wait_us));
    report.push_str(&format!("- Batch sizes: {:?}\n\n", options.batch_sizes));

    report.push_str("## Methodology\n\n");
    report.push_str("- Each case opens a fresh temporary RaftLog directory.\n");
    report.push_str(
        "- The benchmark submits all append + sync flush operations first, then waits for all callbacks.\n",
    );
    report.push_str(
        "- Callback latency is measured from flush submission to callback completion, so it includes flush-worker queueing, file write, and fsync time.\n",
    );
    report.push_str(
        "- QPS is computed from total operations divided by wall-clock time for submitting and completing all callbacks.\n\n",
    );

    report.push_str("## Summary\n\n");
    report.push_str(
        "| case | wait | configured batch | elapsed ms | qps | avg us | p50 us | p90 us | p99 us | max us | batches | sync batches | writes/batch | max batch | sync avg us | sync max us | group avg us | group max us |\n",
    );
    report.push_str(
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
    );

    for result in results {
        let mut latencies = result.latencies.clone();
        latencies.sort_unstable();
        let avg_us = average_duration_us(&latencies);
        let qps = result.operations as f64 / result.elapsed.as_secs_f64();
        let metrics = &result.metrics;
        let writes_per_batch =
            divide(metrics.write_request_count, metrics.batch_count);
        let sync_avg_us = divide(metrics.sync_us, metrics.sync_batch_count);
        let group_avg_us =
            divide(metrics.group_wait_us, metrics.group_wait_count);

        report.push_str(&format!(
            "| {} | {} | {} | {:.2} | {:.0} | {:.0} | {} | {} | {} | {} | {} | {} | {:.2} | {} | {:.2} | {} | {:.2} | {} |\n",
            result.name,
            format_duration(result.wait),
            result.batch_size,
            duration_ms(result.elapsed),
            qps,
            avg_us,
            percentile_us(&latencies, 0.50),
            percentile_us(&latencies, 0.90),
            percentile_us(&latencies, 0.99),
            percentile_us(&latencies, 1.00),
            metrics.batch_count,
            metrics.sync_batch_count,
            writes_per_batch,
            metrics.batch_size_max,
            sync_avg_us,
            metrics.sync_max_us,
            group_avg_us,
            metrics.group_wait_max_us,
        ));
    }

    for result in results {
        report.push_str(&format!("\n## {}\n\n", result.name));
        report.push_str(&format!("- wait: {}\n", format_duration(result.wait)));
        report
            .push_str(&format!("- configured batch: {}\n", result.batch_size));
        report.push_str(&format!("- operations: {}\n", result.operations));
        report
            .push_str(&format!("- payload bytes: {}\n", result.payload_bytes));
        report.push_str(&format!(
            "- elapsed: {:.2} ms\n",
            duration_ms(result.elapsed)
        ));
        report.push_str(&format!(
            "- write bytes: {}\n",
            result.metrics.write_bytes
        ));
        report.push_str(&format!(
            "- queued wait max: {} us\n",
            result.metrics.queued_wait_max_us
        ));
        report.push_str(&format!(
            "- write max: {} us\n",
            result.metrics.write_max_us
        ));
        report.push_str(&format!(
            "- batch max: {} us\n\n",
            result.metrics.batch_max_us
        ));

        report.push_str("### Callback Latency Histogram\n\n");
        report.push_str("| bucket | count | percent |\n");
        report.push_str("|---:|---:|---:|\n");
        for (label, count) in histogram(&result.latencies) {
            let percent = count as f64 * 100.0 / result.latencies.len() as f64;
            report
                .push_str(&format!("| {label} | {count} | {percent:.2}% |\n"));
        }
    }

    report
}

fn histogram(latencies: &[Duration]) -> Vec<(String, usize)> {
    let bounds = [
        100_u64, 250, 500, 1_000, 2_000, 5_000, 10_000, 20_000, 50_000,
        100_000, 250_000, 500_000, 1_000_000,
    ];
    let mut counts = vec![0_usize; bounds.len() + 1];

    for latency in latencies {
        let us = duration_us(*latency);
        let index = bounds
            .iter()
            .position(|bound| us <= *bound)
            .unwrap_or(bounds.len());
        counts[index] += 1;
    }

    let mut rows = Vec::new();
    let mut lower = 0;
    for (i, upper) in bounds.iter().enumerate() {
        rows.push((format!("{}..{} us", lower, upper), counts[i]));
        lower = *upper + 1;
    }
    rows.push((
        format!(">{} us", bounds[bounds.len() - 1]),
        counts[bounds.len()],
    ));
    rows
}

fn average_duration_us(latencies: &[Duration]) -> f64 {
    if latencies.is_empty() {
        return 0.0;
    }

    let total = latencies.iter().map(|d| duration_us(*d)).sum::<u64>();
    total as f64 / latencies.len() as f64
}

fn percentile_us(sorted_latencies: &[Duration], percentile: f64) -> u64 {
    if sorted_latencies.is_empty() {
        return 0;
    }

    let index =
        ((sorted_latencies.len() - 1) as f64 * percentile).round() as usize;
    duration_us(sorted_latencies[index])
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn duration_us(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}

fn divide(total: u64, count: u64) -> f64 {
    if count == 0 {
        0.0
    } else {
        total as f64 / count as f64
    }
}

fn format_duration(duration: Duration) -> String {
    if duration.as_secs() > 0 {
        format!("{:.3}s", duration.as_secs_f64())
    } else if duration.as_millis() > 0 {
        format!("{}ms", duration.as_millis())
    } else {
        format!("{}us", duration.as_micros())
    }
}
