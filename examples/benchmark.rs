#![deny(clippy::all)]

use jitpipe::{
    CompiledFilter, FilterKind, FilterSynthesizer, HeuristicSynthesizer, StreamOptions,
    StreamSampler, StreamingEngine,
};
use std::io::Cursor;
use std::time::Instant;

struct BenchResult {
    name: &'static str,
    rows: usize,
    bytes: usize,
    elapsed_secs: f64,
    tp: usize,
    fp: usize,
    fn_count: usize,
}

impl BenchResult {
    fn print(&self) {
        let lines_per_sec = (self.rows as f64 / self.elapsed_secs) / 1_000.0;
        let mb_per_sec = (self.bytes as f64 / (1024.0 * 1024.0)) / self.elapsed_secs;
        let precision = if self.tp + self.fp == 0 {
            100.0
        } else {
            (self.tp as f64 / (self.tp + self.fp) as f64) * 100.0
        };
        let recall = if self.tp + self.fn_count == 0 {
            100.0
        } else {
            (self.tp as f64 / (self.tp + self.fn_count) as f64) * 100.0
        };
        println!(
            "{:<34} | {:>7} rows | {:>6.1}k lines/s | {:>6.1} MB/s | Prec: {:>5.1}% | Rec: {:>5.1}%",
            self.name, self.rows, lines_per_sec, mb_per_sec, precision, recall
        );
    }
}

fn bench_access_log_100k() -> Result<BenchResult, Box<dyn std::error::Error>> {
    let rows = 100_000;
    let mut input = String::with_capacity(rows * 72);
    let mut expected_matches = 0usize;
    for i in 0..rows {
        if i % 97 == 0 {
            input.push_str(&format!(
                "2026-11-16T12:00:00Z req={i} status=503 route=/checkout latency=940ms\n"
            ));
            expected_matches += 1;
        } else {
            input.push_str(&format!(
                "2026-11-16T12:00:00Z req={i} status=200 route=/health latency=8ms\n"
            ));
        }
    }
    let bytes = input.len();
    let start = Instant::now();
    let mut reader = Cursor::new(input.into_bytes());
    let mut sampler = StreamSampler::new(&mut reader);
    let samples = sampler.collect_samples_with_lookahead("5xx errors", 2000)?;
    let filter = HeuristicSynthesizer::new().synthesize(FilterKind::Grep, "5xx errors", &samples)?;
    let (buf, rem, eof) = sampler.into_parts();
    let mut out = Vec::with_capacity(bytes / 20);
    let summary = StreamingEngine::stream(buf, rem, eof, &filter, &mut out, &StreamOptions::default())?;
    let elapsed_secs = start.elapsed().as_secs_f64().max(1e-6);

    let tp = summary.lines_matched.min(expected_matches);
    let fp = summary.lines_matched.saturating_sub(expected_matches);
    let fn_count = expected_matches.saturating_sub(summary.lines_matched);

    Ok(BenchResult {
        name: "sgrep: 100k mixed access log",
        rows,
        bytes,
        elapsed_secs,
        tp,
        fp,
        fn_count,
    })
}

fn bench_late_rare_event_50k() -> Result<BenchResult, Box<dyn std::error::Error>> {
    let rows = 50_000;
    let mut input = String::with_capacity(rows * 70);
    let mut expected_matches = 0usize;
    for i in 0..rows {
        // First 1,250 lines contain zero rare events; event begins well past row 25
        if i >= 1250 && i % 500 == 0 {
            input.push_str(&format!(
                "2026-11-16T12:05:00Z req={i} ERROR gateway_timeout upstream=stripe latency=5000ms\n"
            ));
            expected_matches += 1;
        } else {
            input.push_str(&format!(
                "2026-11-16T12:05:00Z req={i} INFO ok upstream=cache latency=4ms\n"
            ));
        }
    }
    let bytes = input.len();
    let start = Instant::now();
    let mut reader = Cursor::new(input.into_bytes());
    let mut sampler = StreamSampler::new(&mut reader);
    let samples = sampler.collect_samples_with_lookahead("gateway_timeout", 2000)?;
    let filter = HeuristicSynthesizer::new().synthesize(FilterKind::Grep, "gateway_timeout", &samples)?;
    let (buf, rem, eof) = sampler.into_parts();
    let mut out = Vec::with_capacity(8192);
    let summary = StreamingEngine::stream(buf, rem, eof, &filter, &mut out, &StreamOptions::default())?;
    let elapsed_secs = start.elapsed().as_secs_f64().max(1e-6);

    let out_str = String::from_utf8_lossy(&out);
    let verified = out_str.lines().filter(|l| l.contains("gateway_timeout")).count();

    Ok(BenchResult {
        name: "sgrep: late rare event (row 1250)",
        rows,
        bytes,
        elapsed_secs,
        tp: verified,
        fp: summary.lines_matched.saturating_sub(verified),
        fn_count: expected_matches.saturating_sub(verified),
    })
}

fn bench_sjq_50k() -> Result<BenchResult, Box<dyn std::error::Error>> {
    let rows = 50_000;
    let mut input = String::with_capacity(rows * 68);
    let mut expected_matches = 0usize;
    for i in 0..rows {
        let latency = if i % 40 == 0 { 850 } else { 22 };
        if latency > 500 {
            expected_matches += 1;
        }
        input.push_str(&format!(
            "{{\"user_id\":\"u_{i}\",\"service\":\"api\",\"latency_ms\":{latency}}}\n"
        ));
    }
    let bytes = input.len();
    let start = Instant::now();
    let mut reader = Cursor::new(input.into_bytes());
    let mut sampler = StreamSampler::new(&mut reader);
    let query = "{user_id, service} where latency_ms > 500";
    let samples = sampler.collect_samples_with_lookahead(query, 2000)?;
    let filter = HeuristicSynthesizer::new().synthesize(FilterKind::Jq, query, &samples)?;
    let (buf, rem, eof) = sampler.into_parts();
    let mut out = Vec::with_capacity(bytes / 20);
    let summary = StreamingEngine::stream(buf, rem, eof, &filter, &mut out, &StreamOptions::default())?;
    let elapsed_secs = start.elapsed().as_secs_f64().max(1e-6);

    assert!(filter.matches("{\"user_id\":\"u_0\",\"service\":\"api\",\"latency_ms\":850}"));
    let tp = summary.lines_matched.min(expected_matches);
    let fp = summary.lines_matched.saturating_sub(expected_matches);
    let fn_count = expected_matches.saturating_sub(summary.lines_matched);

    Ok(BenchResult {
        name: "sjq: 50k JSONL projection+pred",
        rows,
        bytes,
        elapsed_secs,
        tp,
        fp,
        fn_count,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== jitpipe Reproducible Accuracy & Throughput Benchmark ===");
    bench_access_log_100k()?.print();
    bench_late_rare_event_50k()?.print();
    bench_sjq_50k()?.print();
    Ok(())
}
