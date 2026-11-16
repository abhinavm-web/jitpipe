use crate::error::JitPipeError;
use crate::filter::awk::{AwkCondition, AwkFilter};
use crate::filter::grep::GrepFilter;
use crate::filter::jq::{JqFilter, JqPredicate, JqProjection};
use crate::filter::{Filter, FilterKind};
use crate::synth::FilterSynthesizer;

/// Heuristic rule-based synthesizer that inspects query tokens and sample rows offline.
#[derive(Debug, Default, Clone)]
pub struct HeuristicSynthesizer;

impl HeuristicSynthesizer {
    pub fn new() -> Self {
        Self
    }

    /// Detects the most probable delimiter from sample rows.
    pub fn detect_delimiter(samples: &[String]) -> Option<String> {
        if samples.is_empty() {
            return None;
        }

        let non_empty: Vec<&str> = samples.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if non_empty.is_empty() {
            return None;
        }

        // Check comma (CSV)
        let comma_counts: Vec<usize> = non_empty.iter().map(|s| s.matches(',').count()).collect();
        if comma_counts.iter().all(|&c| c > 0 && c == comma_counts[0]) {
            return Some(",".to_string());
        }

        // Check tab (TSV)
        let tab_counts: Vec<usize> = non_empty.iter().map(|s| s.matches('\t').count()).collect();
        if tab_counts.iter().all(|&c| c > 0 && c == tab_counts[0]) {
            return Some("\t".to_string());
        }

        // Check pipe
        let pipe_counts: Vec<usize> = non_empty.iter().map(|s| s.matches('|').count()).collect();
        if pipe_counts.iter().all(|&c| c > 0 && c == pipe_counts[0]) {
            return Some("|".to_string());
        }

        None
    }

    /// Synthesizes a regex pattern for sgrep based on query keywords and sample rows.
    fn synthesize_grep(&self, query: &str, samples: &[String]) -> Result<GrepFilter, JitPipeError> {
        let trimmed = query.trim();

        // 1. Direct explicit regex with slashes e.g. /pattern/
        if trimmed.starts_with('/') && trimmed.ends_with('/') && trimmed.len() >= 2 {
            let pat = &trimmed[1..trimmed.len() - 1];
            if let Ok(filter) = GrepFilter::new(pat, false, false) {
                return Ok(filter);
            }
        }

        // 2. Keyword heuristic mapping
        let q_lower = trimmed.to_lowercase();
        let mut candidates = Vec::new();

        if q_lower.contains("5xx") {
            candidates.push(r"\b5\d{2}\b");
        }
        if q_lower.contains("4xx") {
            candidates.push(r"\b4\d{2}\b");
        }
        if q_lower.contains("status 500") || q_lower.contains("http 500") {
            candidates.push(r"(?i)\b500\b");
        }
        if q_lower.contains("404") {
            candidates.push(r"\b404\b");
        }
        if q_lower.contains("critical or error") || q_lower.contains("error or critical") {
            candidates.push(r"(?i)\b(fatal|critical|error)\b");
        } else if q_lower.contains("error") {
            candidates.push(r"(?i)\berror\b");
        } else if q_lower.contains("warn") {
            candidates.push(r"(?i)\bwarn(ing)?\b");
        }
        if q_lower.contains("failed") || q_lower.contains("failure") || q_lower.contains("auth") {
            candidates.push(r"(?i)(auth.*fail|fail.*auth|failed|failure)");
        }
        if q_lower.contains("ip") || q_lower.contains("address") {
            candidates.push(r"\b(?:\d{1,3}\.){3}\d{1,3}\b");
        }

        // Try candidates against sample lines
        for pat in &candidates {
            if let Ok(re) = regex::Regex::new(pat) {
                if samples.iter().any(|s| re.is_match(s)) {
                    return GrepFilter::new(*pat, false, false);
                }
            }
        }

        // If candidates found but none matched samples, use first candidate
        if let Some(first) = candidates.first() {
            return GrepFilter::new(*first, false, false);
        }

        // 3. Fallback: single word with word boundary, or direct regex if valid, or escaped query
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if words.len() == 1 {
            let pat = format!(r"(?i)\b{}\b", regex::escape(words[0]));
            if let Ok(filter) = GrepFilter::new(pat, false, false) {
                return Ok(filter);
            }
        }

        if let Ok(filter) = GrepFilter::new(trimmed, false, false) {
            return Ok(filter);
        }

        let escaped = format!(r"(?i){}", regex::escape(trimmed));
        GrepFilter::new(escaped, false, false)
    }

    /// Synthesizes an Awk filter based on query and detected sample delimiter.
    fn synthesize_awk(&self, query: &str, samples: &[String]) -> Result<AwkFilter, JitPipeError> {
        let delimiter = Self::detect_delimiter(samples);

        // Check if query is an explicit template containing $ tokens
        if query.contains('$') {
            return Ok(AwkFilter::new_template(delimiter, query));
        }

        // Parse column indices
        let mut cols = Vec::new();
        for token in query.split_whitespace() {
            let clean = token.trim_matches(|c: char| !c.is_ascii_digit());
            if let Ok(c) = clean.parse::<usize>() {
                if c > 0 && !cols.contains(&c) {
                    cols.push(c);
                }
            }
        }

        if cols.is_empty() {
            // Default first two columns
            cols = vec![1, 2];
        }

        let mut filter = AwkFilter::new_columns(delimiter, Some("\t".to_string()), cols);

        // Check for comparison conditions in query
        if query.contains('>') {
            if let Some(idx) = query.find('>') {
                let after = query[idx + 1..].trim();
                let num_str: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                if let Ok(threshold) = num_str.parse::<f64>() {
                    filter = filter.with_condition(AwkCondition::FieldGreaterThan { col: 3, threshold })?;
                }
            }
        } else if query.contains('<') {
            if let Some(idx) = query.find('<') {
                let after = query[idx + 1..].trim();
                let num_str: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                if let Ok(threshold) = num_str.parse::<f64>() {
                    filter = filter.with_condition(AwkCondition::FieldLessThan { col: 3, threshold })?;
                }
            }
        }

        Ok(filter)
    }

    /// Synthesizes a Jq filter based on query tokens and sample JSON structures.
    fn synthesize_jq(&self, query: &str, _samples: &[String]) -> Result<JqFilter, JitPipeError> {
        let trimmed = query.trim();
        let mut filter = JqFilter::default();

        // Predicate detection: e.g. latency > 500 or status == 200
        if trimmed.contains('>') {
            if let Some(idx) = trimmed.find('>') {
                let left = trimmed[..idx].trim();
                let after = trimmed[idx + 1..].trim();
                let num_str: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                if let Ok(threshold) = num_str.parse::<f64>() {
                    let field = if trimmed.contains("latency") || _samples.iter().any(|s| s.contains("\"latency\"")) {
                        "latency".to_string()
                    } else {
                        left.split_whitespace().last().unwrap_or("latency").trim_start_matches('.').to_string()
                    };
                    filter.predicate = JqPredicate::GreaterThan {
                        path: field,
                        threshold,
                    };
                }
            }
        }

        let clean_proj = if let Some(idx) = trimmed.find('>') {
            trimmed[..idx].trim()
        } else if let Some(idx) = trimmed.find("where") {
            trimmed[..idx].trim()
        } else {
            trimmed
        };
        let clean_proj = clean_proj.trim_end_matches("where").trim();

        // Projection detection
        if clean_proj.starts_with('{') && clean_proj.contains('}') {
            let start = clean_proj.find('{').unwrap_or(0);
            let end = clean_proj.find('}').unwrap_or(clean_proj.len());
            let inner = &clean_proj[start + 1..end];
            let fields: Vec<String> = inner
                .split(',')
                .map(|s| s.trim().trim_start_matches('.').to_string())
                .filter(|s| !s.is_empty())
                .collect();
            filter.projection = JqProjection::Fields(fields);
        } else if clean_proj.ends_with("[]") {
            let path = clean_proj.trim_end_matches("[]");
            filter.projection = JqProjection::ArrayUnroll(path.to_string());
        } else if clean_proj.starts_with('.') {
            filter.projection = JqProjection::Path(clean_proj.to_string());
        } else {
            let token = clean_proj.split_whitespace().next().unwrap_or("");
            if !token.is_empty() {
                filter.projection = JqProjection::Path(format!(".{}", token.trim_start_matches('.')));
            }
        }

        Ok(filter)
    }
}

impl FilterSynthesizer for HeuristicSynthesizer {
    fn synthesize(&self, kind: FilterKind, query: &str, samples: &[String]) -> Result<Filter, JitPipeError> {
        match kind {
            FilterKind::Grep => {
                let f = self.synthesize_grep(query, samples)?;
                Ok(Filter::Grep(f))
            }
            FilterKind::Awk => {
                let f = self.synthesize_awk(query, samples)?;
                Ok(Filter::Awk(f))
            }
            FilterKind::Jq => {
                let f = self.synthesize_jq(query, samples)?;
                Ok(Filter::Jq(f))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::CompiledFilter;

    #[test]
    fn test_heuristic_detect_csv_delimiter() {
        let samples = vec![
            "col1,col2,col3\n".to_string(),
            "val1,val2,val3\n".to_string(),
        ];
        assert_eq!(HeuristicSynthesizer::detect_delimiter(&samples), Some(",".to_string()));
    }

    #[test]
    fn test_heuristic_detect_tab_delimiter() {
        let samples = vec![
            "col1\tcol2\tcol3\n".to_string(),
            "val1\tval2\tval3\n".to_string(),
        ];
        assert_eq!(HeuristicSynthesizer::detect_delimiter(&samples), Some("\t".to_string()));
    }

    #[test]
    fn test_heuristic_grep_status_code() {
        let synth = HeuristicSynthesizer::new();
        let samples = vec!["2026-11-16 GET /api/v1 500 45ms\n".to_string()];
        let filter = synth.synthesize(FilterKind::Grep, "5xx errors", &samples).expect("synth");
        assert!(filter.matches("2026-11-16 GET /api/v1 503 12ms"));
        assert!(!filter.matches("2026-11-16 GET /api/v1 200 12ms"));
    }
}
