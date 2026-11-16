use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::error::JitPipeError;
use crate::filter::{CompiledFilter, Filter, FilterKind};

/// Cache envelope storing a serialized filter rule along with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFilter {
    pub version: u32,
    pub kind: FilterKind,
    pub query: String,
    pub created_at: String,
    pub filter: Filter,
}

/// Serializes and writes a compiled filter to a file path.
pub fn save_filter(path: impl AsRef<Path>, filter: &Filter, query: &str) -> Result<(), JitPipeError> {
    let file = File::create(path.as_ref()).map_err(JitPipeError::Io)?;
    let writer = BufWriter::new(file);

    let cached = CachedFilter {
        version: 1,
        kind: filter.kind(),
        query: query.to_string(),
        created_at: "2026-11-16T19:30:00Z".to_string(),
        filter: filter.clone(),
    };

    serde_json::to_writer_pretty(writer, &cached).map_err(JitPipeError::Json)?;
    Ok(())
}

/// Loads and compiles a previously serialized filter from a file path.
pub fn load_filter(path: impl AsRef<Path>) -> Result<Filter, JitPipeError> {
    let file = File::open(path.as_ref()).map_err(JitPipeError::Io)?;
    let reader = BufReader::new(file);

    // Attempt to deserialize CachedFilter envelope first, then fallback to raw Filter
    let mut filter = match serde_json::from_reader::<_, CachedFilter>(reader) {
        Ok(cached) => cached.filter,
        Err(_) => {
            let file_again = File::open(path.as_ref()).map_err(JitPipeError::Io)?;
            let reader_again = BufReader::new(file_again);
            serde_json::from_reader::<_, Filter>(reader_again).map_err(JitPipeError::Json)?
        }
    };

    filter.ensure_compiled()?;
    Ok(filter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::grep::GrepFilter;
    use tempfile::NamedTempFile;

    #[test]
    fn test_save_and_load_filter_roundtrip() {
        let temp = NamedTempFile::new().expect("tempfile");
        let path = temp.path();

        let original_grep = GrepFilter::new(r"\b500\b", false, true).expect("grep");
        let original_filter = Filter::Grep(original_grep);

        save_filter(path, &original_filter, "HTTP 500").expect("save");

        let loaded = load_filter(path).expect("load");
        assert_eq!(loaded.kind(), FilterKind::Grep);
        assert!(loaded.matches("HTTP 500 Internal Error"));
        assert!(!loaded.matches("HTTP 200 OK"));
    }
}
