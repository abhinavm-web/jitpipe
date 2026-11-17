#![deny(clippy::all)]

pub mod cli;
pub mod engine;
pub mod error;
pub mod filter;
pub mod sampler;
pub mod synth;

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};

pub use cli::{JitPipeArgs, JitPipeSubcommand, SawkArgs, SgrepArgs, SjqArgs};
pub use engine::{create_buffered_writer, create_stdout_writer, is_broken_pipe, StreamOptions, StreamSummary, StreamingEngine, DEFAULT_BUFFER_SIZE};
pub use error::JitPipeError;
pub use filter::{AwkCondition, AwkFilter, CompiledFilter, Filter, FilterKind, GrepFilter, JqFilter, JqPredicate, JqProjection};
pub use sampler::{StreamSampler, DEFAULT_SAMPLE_SIZE};
pub use synth::{load_filter, save_filter, CachedFilter, FilterSynthesizer, HeuristicSynthesizer, LlmSynthesizer, MockSynthesizer};

/// Internal multi-file reader combining one or more file paths into a single continuous stream.
struct MultiFileReader {
    files: Vec<String>,
    index: usize,
    current: Option<File>,
}

impl MultiFileReader {
    fn new(files: Vec<String>) -> Self {
        Self {
            files,
            index: 0,
            current: None,
        }
    }
}

impl Read for MultiFileReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if let Some(file) = &mut self.current {
                let n = file.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
                self.current = None;
            }
            if self.index >= self.files.len() {
                return Ok(0);
            }
            let path = &self.files[self.index];
            self.index += 1;
            let file = File::open(path)?;
            self.current = Some(file);
        }
    }
}

/// Creates a buffered reader from either standard input or a list of file paths.
pub fn open_input_reader(files: &[String]) -> Result<Box<dyn BufRead + Send>, JitPipeError> {
    if files.is_empty() || (files.len() == 1 && files[0] == "-") {
        Ok(Box::new(BufReader::new(io::stdin())))
    } else if files.len() == 1 {
        let file = File::open(&files[0]).map_err(JitPipeError::Io)?;
        Ok(Box::new(BufReader::new(file)))
    } else {
        let multi = MultiFileReader::new(files.to_vec());
        Ok(Box::new(BufReader::new(multi)))
    }
}

/// Executes sgrep (semantic grep) given the CLI arguments.
pub fn run_sgrep(args: SgrepArgs) -> Result<i32, JitPipeError> {
    let query = args.regexp.clone().or_else(|| args.query.clone()).unwrap_or_default();

    if query.is_empty() && args.load_filter.is_none() {
        return Err(JitPipeError::Usage("No search query or pattern specified".to_string()));
    }

    let mut reader = open_input_reader(&args.files)?;

    let (filter, buffered_lines, remaining_reader, eof_reached) = if let Some(path) = &args.load_filter {
        let filter = load_filter(path)?;
        (filter, Vec::new(), reader, false)
    } else {
        let mut sampler = StreamSampler::with_sample_size(&mut reader, args.sample_size);
        let synth_samples = sampler
            .collect_samples_with_lookahead(&query, 2000)
            .map_err(JitPipeError::Io)?;

        let synthesizer: Box<dyn FilterSynthesizer> = if args.mock {
            Box::new(MockSynthesizer::new())
        } else if args.heuristic {
            Box::new(HeuristicSynthesizer::new())
        } else {
            Box::new(LlmSynthesizer::new())
        };

        let mut filter = synthesizer.synthesize(FilterKind::Grep, &query, &synth_samples)?;

        // Apply CLI argument overrides
        if let Filter::Grep(g) = &mut filter {
            if args.ignore_case {
                g.ignore_case = true;
            }
            if args.invert_match {
                g.invert = !g.invert;
            }
            g.ensure_compiled()?;
        }

        if let Some(save_path) = &args.save_filter {
            save_filter(save_path, &filter, &query)?;
        }

        if args.explain {
            eprintln!("[jitpipe:sgrep] Query: \"{}\" -> Synthesized: {:?}", query, filter);
        }

        let (samples, _, eof) = sampler.into_parts();
        (filter, samples, reader, eof)
    };

    let stream_options = StreamOptions {
        line_numbers: args.line_number,
        count_only: args.count,
        max_count: args.max_count,
        quiet: args.quiet,
    };

    let writer = create_stdout_writer();
    let summary = StreamingEngine::stream(
        buffered_lines,
        remaining_reader,
        eof_reached,
        &filter,
        writer,
        &stream_options,
    )?;

    if summary.broken_pipe {
        return Ok(0);
    }

    if args.quiet {
        if summary.lines_matched > 0 {
            Ok(0)
        } else {
            Ok(1)
        }
    } else {
        Ok(0)
    }
}

/// Executes sawk (semantic awk) given the CLI arguments.
pub fn run_sawk(args: SawkArgs) -> Result<i32, JitPipeError> {
    let expression = args.expression.clone().unwrap_or_default();

    if expression.is_empty() && args.load_filter.is_none() {
        return Err(JitPipeError::Usage("No expression or template specified".to_string()));
    }

    let mut reader = open_input_reader(&args.files)?;

    let (filter, buffered_lines, remaining_reader, eof_reached) = if let Some(path) = &args.load_filter {
        let filter = load_filter(path)?;
        (filter, Vec::new(), reader, false)
    } else {
        let mut sampler = StreamSampler::with_sample_size(&mut reader, args.sample_size);
        let synth_samples = sampler
            .collect_samples_with_lookahead(&expression, 2000)
            .map_err(JitPipeError::Io)?;

        let synthesizer: Box<dyn FilterSynthesizer> = if args.mock {
            Box::new(MockSynthesizer::new())
        } else if args.heuristic {
            Box::new(HeuristicSynthesizer::new())
        } else {
            Box::new(LlmSynthesizer::new())
        };

        let mut filter = synthesizer.synthesize(FilterKind::Awk, &expression, &synth_samples)?;

        // Apply CLI overrides
        if let Filter::Awk(a) = &mut filter {
            if let Some(fs) = &args.field_separator {
                a.delimiter = Some(fs.clone());
            }
            a.output_delimiter = args.output_delimiter.clone();
            a.ensure_compiled()?;
        }

        if let Some(save_path) = &args.save_filter {
            save_filter(save_path, &filter, &expression)?;
        }

        if args.explain {
            eprintln!("[jitpipe:sawk] Expression: \"{}\" -> Synthesized: {:?}", expression, filter);
        }

        let (samples, _, eof) = sampler.into_parts();
        (filter, samples, reader, eof)
    };

    let stream_options = StreamOptions::default();
    let writer = create_stdout_writer();
    let summary = StreamingEngine::stream(
        buffered_lines,
        remaining_reader,
        eof_reached,
        &filter,
        writer,
        &stream_options,
    )?;

    if summary.broken_pipe {
        return Ok(0);
    }

    Ok(0)
}

/// Executes sjq (semantic jq) given the CLI arguments.
pub fn run_sjq(args: SjqArgs) -> Result<i32, JitPipeError> {
    let filter_query = args.filter.clone().unwrap_or_default();

    if filter_query.is_empty() && args.load_filter.is_none() {
        return Err(JitPipeError::Usage("No filter or JSON query specified".to_string()));
    }

    let mut reader = open_input_reader(&args.files)?;

    let (filter, buffered_lines, remaining_reader, eof_reached) = if let Some(path) = &args.load_filter {
        let filter = load_filter(path)?;
        (filter, Vec::new(), reader, false)
    } else {
        let mut sampler = StreamSampler::with_sample_size(&mut reader, args.sample_size);
        let synth_samples = sampler
            .collect_samples_with_lookahead(&filter_query, 2000)
            .map_err(JitPipeError::Io)?;

        let synthesizer: Box<dyn FilterSynthesizer> = if args.mock {
            Box::new(MockSynthesizer::new())
        } else if args.heuristic {
            Box::new(HeuristicSynthesizer::new())
        } else {
            Box::new(LlmSynthesizer::new())
        };

        let mut filter = synthesizer.synthesize(FilterKind::Jq, &filter_query, &synth_samples)?;

        // Apply CLI overrides
        if let Filter::Jq(j) = &mut filter {
            if args.raw_output {
                j.raw_output = true;
            }
            if args.compact_output {
                j.compact = true;
            }
            if args.ignore_malformed {
                j.ignore_malformed = true;
            }
        }

        if let Some(save_path) = &args.save_filter {
            save_filter(save_path, &filter, &filter_query)?;
        }

        if args.explain {
            eprintln!("[jitpipe:sjq] Query: \"{}\" -> Synthesized: {:?}", filter_query, filter);
        }

        let (samples, _, eof) = sampler.into_parts();
        (filter, samples, reader, eof)
    };

    let stream_options = StreamOptions::default();
    let writer = create_stdout_writer();
    let summary = StreamingEngine::stream(
        buffered_lines,
        remaining_reader,
        eof_reached,
        &filter,
        writer,
        &stream_options,
    )?;

    if summary.broken_pipe {
        return Ok(0);
    }

    Ok(0)
}

/// Executes the unified jitpipe CLI dispatcher.
pub fn run_jitpipe(args: JitPipeArgs) -> Result<i32, JitPipeError> {
    match args.command {
        Some(JitPipeSubcommand::Sgrep(sub_args)) => run_sgrep(sub_args),
        Some(JitPipeSubcommand::Sawk(sub_args)) => run_sawk(sub_args),
        Some(JitPipeSubcommand::Sjq(sub_args)) => run_sjq(sub_args),
        None => {
            if args.default_sgrep.query.is_some() || args.default_sgrep.regexp.is_some() || args.default_sgrep.load_filter.is_some() {
                run_sgrep(args.default_sgrep)
            } else {
                eprintln!("jitpipe: No query or subcommand specified. Run with --help for usage.");
                Ok(2)
            }
        }
    }
}
