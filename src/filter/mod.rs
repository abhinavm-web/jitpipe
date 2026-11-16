pub mod awk;
pub mod grep;
pub mod jq;

use std::borrow::Cow;
use std::fmt;
use serde::{Deserialize, Serialize};

pub use awk::{AwkCondition, AwkFilter};
pub use grep::GrepFilter;
pub use jq::{JqFilter, JqPredicate, JqProjection};
use crate::error::JitPipeError;

/// The target filter variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterKind {
    Grep,
    Awk,
    Jq,
}

impl fmt::Display for FilterKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Grep => write!(f, "grep"),
            Self::Awk => write!(f, "awk"),
            Self::Jq => write!(f, "jq"),
        }
    }
}

/// Core interface for deterministic compiled filters operating on streamed lines.
pub trait CompiledFilter: Send + Sync {
    /// Applies the filter to an input line.
    /// Returns Some(Cow::Borrowed(line)) for zero-allocation matches,
    /// Some(Cow::Owned(transformed)) for projected lines,
    /// or None if the line does not match.
    fn apply<'a>(&self, line: &'a str) -> Option<Cow<'a, str>>;

    /// Returns true if the line satisfies the filter predicate.
    fn matches(&self, line: &str) -> bool;

    /// Transforms a matching line, allocating a new String.
    fn transform(&self, line: &str) -> Option<String>;

    /// Returns the kind of filter.
    fn kind(&self) -> FilterKind;
}

/// Unified polymorphic filter representation that can be executed and serialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Filter {
    Grep(GrepFilter),
    Awk(AwkFilter),
    Jq(JqFilter),
}

impl Filter {
    /// Recompiles internal regexes if necessary after deserialization.
    pub fn ensure_compiled(&mut self) -> Result<(), JitPipeError> {
        match self {
            Self::Grep(g) => g.ensure_compiled(),
            Self::Awk(a) => a.ensure_compiled(),
            Self::Jq(_) => Ok(()),
        }
    }
}

impl CompiledFilter for Filter {
    fn apply<'a>(&self, line: &'a str) -> Option<Cow<'a, str>> {
        match self {
            Self::Grep(g) => g.apply(line),
            Self::Awk(a) => a.apply(line),
            Self::Jq(j) => j.apply(line),
        }
    }

    fn matches(&self, line: &str) -> bool {
        match self {
            Self::Grep(g) => g.matches(line),
            Self::Awk(a) => a.matches(line),
            Self::Jq(j) => j.matches(line),
        }
    }

    fn transform(&self, line: &str) -> Option<String> {
        match self {
            Self::Grep(g) => g.transform(line),
            Self::Awk(a) => a.transform(line),
            Self::Jq(j) => j.transform(line),
        }
    }

    fn kind(&self) -> FilterKind {
        match self {
            Self::Grep(_) => FilterKind::Grep,
            Self::Awk(_) => FilterKind::Awk,
            Self::Jq(_) => FilterKind::Jq,
        }
    }
}
