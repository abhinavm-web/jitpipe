#![deny(clippy::all)]

use jitpipe::{
     FilterKind, FilterSynthesizer, HeuristicSynthesizer, StreamOptions, StreamSampler,
    StreamingEngine,
};
use std::io::Cursor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== jitpipe: JIT-Compiled Semantic Unix Pipe Showcase ===");

    let mut raw_logs = String::new();
    for i in 1..=1_000 {
        let status = if i % 250 == 0 { 503 } else { 200 };
        let latency = if status == 503 { 1420 } else { 18 };
        raw_logs.push_str(&format!(
            "2026-11-16T12:00:00Z req_id=req_{i:04} status={status} latency_ms={latency}\n"
        ));
    }

    let mut reader = Cursor::new(raw_logs.into_bytes());
    let mut sampler = StreamSampler::with_sample_size(&mut reader, 25);
    sampler.collect_samples()?;

    let synth = HeuristicSynthesizer::new();
    let query = "status=503";
    let filter = synth.synthesize(FilterKind::Grep, query, sampler.samples())?;
    println!("Sampled {} rows -> compiled native filter: {:?}", sampler.samples().len(), filter);

    let (buffered, _, eof) = sampler.into_parts();
    let mut out = Vec::new();
    let summary = StreamingEngine::stream(
        buffered,
        reader,
        eof,
        &filter,
        &mut out,
        &StreamOptions {
            line_numbers: true,
            ..Default::default()
        },
    )?;

    println!(
        "Streamed {} total lines ({} matched) with zero per-row allocations:",
        summary.lines_read, summary.lines_matched
    );
    print!("{}", String::from_utf8_lossy(&out));

    Ok(())
}
