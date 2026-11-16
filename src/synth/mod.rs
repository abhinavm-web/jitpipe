pub mod cache;
pub mod heuristic;
pub mod llm;
pub mod mock;

pub use cache::{load_filter, save_filter, CachedFilter};
pub use heuristic::HeuristicSynthesizer;
pub use llm::LlmSynthesizer;
pub use mock::MockSynthesizer;

use crate::error::JitPipeError;
use crate::filter::{Filter, FilterKind};

/// Core trait for generating deterministic compiled filters from natural language queries and samples.
pub trait FilterSynthesizer: Send + Sync {
    /// Synthesizes a filter matching the desired kind and query, guided by the sample rows.
    fn synthesize(&self, kind: FilterKind, query: &str, samples: &[String]) -> Result<Filter, JitPipeError>;
}
