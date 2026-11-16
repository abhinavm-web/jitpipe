use clap::{Parser, Subcommand};

/// Command-line arguments for sgrep (semantic grep).
#[derive(Parser, Debug, Clone)]
#[command(
    name = "sgrep",
    about = "JIT-compiled semantic grep: samples ~25 rows, synthesizes a regex, and streams at native speed.",
    version
)]
pub struct SgrepArgs {
    /// Natural language query or pattern to search for.
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,

    /// Input file(s) to process. If omitted or '-', reads from stdin.
    #[arg(value_name = "FILES")]
    pub files: Vec<String>,

    /// Case-insensitive search.
    #[arg(short = 'i', long = "ignore-case")]
    pub ignore_case: bool,

    /// Invert matching: select non-matching lines.
    #[arg(short = 'v', long = "invert-match")]
    pub invert_match: bool,

    /// Prefix each matching line with its 1-based line number.
    #[arg(short = 'n', long = "line-number")]
    pub line_number: bool,

    /// Suppress normal output; print total count of matching lines.
    #[arg(short = 'c', long = "count")]
    pub count: bool,

    /// Stop reading after NUM matching lines.
    #[arg(short = 'm', long = "max-count", value_name = "NUM")]
    pub max_count: Option<usize>,

    /// Quiet mode: emit nothing to stdout, exit 0 if match found, exit 1 if no match.
    #[arg(short = 'q', long = "quiet", alias = "silent")]
    pub quiet: bool,

    /// Explicit regular expression pattern (allows patterns starting with -).
    #[arg(short = 'e', long = "regexp", value_name = "PATTERN")]
    pub regexp: Option<String>,

    /// Number of sample lines to buffer for pattern synthesis.
    #[arg(long = "sample-size", default_value_t = 25)]
    pub sample_size: usize,

    /// Force offline heuristic filter synthesis.
    #[arg(long = "heuristic")]
    pub heuristic: bool,

    /// Force deterministic mock synthesizer for offline testing.
    #[arg(long = "mock")]
    pub mock: bool,

    /// Emit synthesis diagnostics and execution statistics to stderr.
    #[arg(long = "explain")]
    pub explain: bool,

    /// Save the compiled filter representation to a file path.
    #[arg(long = "save-filter", value_name = "PATH")]
    pub save_filter: Option<String>,

    /// Load a previously saved filter from a file path, bypassing synthesis.
    #[arg(long = "load-filter", value_name = "PATH")]
    pub load_filter: Option<String>,
}

/// Command-line arguments for sawk (semantic awk).
#[derive(Parser, Debug, Clone)]
#[command(
    name = "sawk",
    about = "JIT-compiled semantic awk: samples ~25 rows, synthesizes column extraction, and streams at native speed.",
    version
)]
pub struct SawkArgs {
    /// Extraction template, field expression, or natural language query.
    #[arg(value_name = "EXPRESSION")]
    pub expression: Option<String>,

    /// Input file(s) to process. If omitted or '-', reads from stdin.
    #[arg(value_name = "FILES")]
    pub files: Vec<String>,

    /// Field delimiter (character, string, or regex). Default: whitespace.
    #[arg(short = 'F', long = "field-separator", value_name = "FS")]
    pub field_separator: Option<String>,

    /// Output field delimiter string (default: tab).
    #[arg(short = 'O', long = "output-delimiter", default_value = "\t")]
    pub output_delimiter: String,

    /// Variable assignment (e.g. VAR=VAL).
    #[arg(short = 'v', long = "assign", value_name = "VAR=VAL")]
    pub assign: Vec<String>,

    /// Number of sample lines to buffer for pattern synthesis.
    #[arg(long = "sample-size", default_value_t = 25)]
    pub sample_size: usize,

    /// Force offline heuristic filter synthesis.
    #[arg(long = "heuristic")]
    pub heuristic: bool,

    /// Force deterministic mock synthesizer for offline testing.
    #[arg(long = "mock")]
    pub mock: bool,

    /// Emit synthesis diagnostics to stderr.
    #[arg(long = "explain")]
    pub explain: bool,

    /// Save the compiled filter representation to a file path.
    #[arg(long = "save-filter", value_name = "PATH")]
    pub save_filter: Option<String>,

    /// Load a previously saved filter from a file path.
    #[arg(long = "load-filter", value_name = "PATH")]
    pub load_filter: Option<String>,
}

/// Command-line arguments for sjq (semantic jq).
#[derive(Parser, Debug, Clone)]
#[command(
    name = "sjq",
    about = "JIT-compiled semantic jq: samples ~25 rows, synthesizes JSON projection, and streams at native speed.",
    version
)]
pub struct SjqArgs {
    /// Jq path expression, object projection, or natural language query.
    #[arg(value_name = "FILTER")]
    pub filter: Option<String>,

    /// Input file(s) to process. If omitted or '-', reads from stdin.
    #[arg(value_name = "FILES")]
    pub files: Vec<String>,

    /// Compact output: output each JSON object on a single line.
    #[arg(short = 'c', long = "compact-output")]
    pub compact_output: bool,

    /// Raw output: output raw strings without enclosing quotes.
    #[arg(short = 'r', long = "raw-output")]
    pub raw_output: bool,

    /// Slurp mode: read entire input into a single array before processing.
    #[arg(short = 's', long = "slurp")]
    pub slurp: bool,

    /// Silently skip lines that fail JSON parsing.
    #[arg(long = "ignore-malformed")]
    pub ignore_malformed: bool,

    /// Number of sample lines to buffer for pattern synthesis.
    #[arg(long = "sample-size", default_value_t = 25)]
    pub sample_size: usize,

    /// Force offline heuristic filter synthesis.
    #[arg(long = "heuristic")]
    pub heuristic: bool,

    /// Force deterministic mock synthesizer for offline testing.
    #[arg(long = "mock")]
    pub mock: bool,

    /// Emit synthesis diagnostics to stderr.
    #[arg(long = "explain")]
    pub explain: bool,

    /// Save the compiled filter representation to a file path.
    #[arg(long = "save-filter", value_name = "PATH")]
    pub save_filter: Option<String>,

    /// Load a previously saved filter from a file path.
    #[arg(long = "load-filter", value_name = "PATH")]
    pub load_filter: Option<String>,
}

/// Command-line arguments for the unified jitpipe CLI.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "jitpipe",
    about = "JIT-compiled semantic Unix pipe utilities (sgrep, sawk, sjq).",
    version
)]
pub struct JitPipeArgs {
    #[command(subcommand)]
    pub command: Option<JitPipeSubcommand>,

    #[command(flatten)]
    pub default_sgrep: SgrepArgs,
}

#[derive(Subcommand, Debug, Clone)]
pub enum JitPipeSubcommand {
    /// Semantic grep: regex/boolean predicate matching.
    Sgrep(SgrepArgs),
    /// Semantic awk: column extraction and format projection.
    Sawk(SawkArgs),
    /// Semantic jq: JSON projection and predicate filtering.
    Sjq(SjqArgs),
}
