use crate::error::JitPipeError;
use crate::filter::awk::{AwkCondition, AwkFilter};
use crate::filter::grep::GrepFilter;
use crate::filter::jq::{JqFilter, JqPredicate, JqProjection};
use crate::filter::{Filter, FilterKind};
use crate::synth::FilterSynthesizer;

/// Deterministic mock synthesizer providing offline pattern compilation for tests.
#[derive(Debug, Default, Clone)]
pub struct MockSynthesizer {
    preset: Option<Filter>,
}

impl MockSynthesizer {
    pub fn new() -> Self {
        Self { preset: None }
    }

    /// Sets a fixed preset filter to return regardless of query.
    pub fn with_preset(preset: Filter) -> Self {
        Self {
            preset: Some(preset),
        }
    }
}

impl FilterSynthesizer for MockSynthesizer {
    fn synthesize(&self, kind: FilterKind, query: &str, _samples: &[String]) -> Result<Filter, JitPipeError> {
        if let Some(preset) = &self.preset {
            return Ok(preset.clone());
        }

        match kind {
            FilterKind::Grep => {
                let pattern = if query.starts_with('/') && query.ends_with('/') && query.len() >= 2 {
                    query[1..query.len() - 1].to_string()
                } else {
                    let q_lower = query.to_lowercase();
                    if q_lower.contains("5xx") {
                        r"\b5\d{2}\b".to_string()
                    } else if q_lower.contains("4xx") {
                        r"\b4\d{2}\b".to_string()
                    } else if q_lower.contains("critical or error") {
                        r"(?i)\b(critical|error)\b".to_string()
                    } else if q_lower.contains("error") {
                        r"(?i)\berror\b".to_string()
                    } else if q_lower.contains("fail") || q_lower.contains("auth") {
                        r"(?i)(fail|login|auth)".to_string()
                    } else if q_lower.contains("ip") || q_lower.contains("address") {
                        r"\b(?:\d{1,3}\.){3}\d{1,3}\b".to_string()
                    } else if regex::Regex::new(query).is_ok() {
                        query.to_string()
                    } else {
                        regex::escape(query)
                    }
                };

                let filter = GrepFilter::new(pattern, false, false)?;
                Ok(Filter::Grep(filter))
            }

            FilterKind::Awk => {
                let mut delimiter = None;
                let mut clean_query = query.to_string();
                if query.contains("csv") || query.contains("comma") {
                    delimiter = Some(",".to_string());
                    clean_query = clean_query.replace("csv", "").replace("comma", "").trim().to_string();
                } else if query.contains("tsv") || query.contains("tab") {
                    delimiter = Some("\t".to_string());
                    clean_query = clean_query.replace("tsv", "").replace("tab", "").trim().to_string();
                }

                if clean_query.contains('$') {
                    // Extract template or columns directly
                    let filter = AwkFilter::new_template(delimiter, clean_query);
                    Ok(Filter::Awk(filter))
                } else {
                    let mut cols = Vec::new();
                    for token in clean_query.split_whitespace() {
                        if let Ok(c) = token.parse::<usize>() {
                            cols.push(c);
                        }
                    }
                    if cols.is_empty() {
                        cols = vec![1, 2];
                    }
                    let mut filter = AwkFilter::new_columns(delimiter, Some("\t".to_string()), cols);

                    if clean_query.contains("> 500") || clean_query.contains(">500") {
                        filter = filter.with_condition(AwkCondition::FieldGreaterThan { col: 3, threshold: 500.0 })?;
                    }

                    Ok(Filter::Awk(filter))
                }
            }

            FilterKind::Jq => {
                let trimmed = query.trim();
                let mut filter = JqFilter::default();

                if trimmed.contains('>') {
                    let path = if trimmed.contains("latency") || _samples.iter().any(|s| s.contains("\"latency\"")) {
                        "latency".to_string()
                    } else if let Some(idx) = trimmed.find('>') {
                        let left = trimmed[..idx].trim();
                        left.split_whitespace().last().unwrap_or("latency").trim_start_matches('.').to_string()
                    } else {
                        "latency".to_string()
                    };

                    if let Some(idx) = trimmed.find('>') {
                        let after = trimmed[idx + 1..].trim();
                        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                        if let Ok(threshold) = num_str.parse::<f64>() {
                            filter.predicate = JqPredicate::GreaterThan {
                                path,
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

                if clean_proj.starts_with('{') && clean_proj.ends_with('}') {
                    let inner = &clean_proj[1..clean_proj.len() - 1];
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
                } else if !clean_proj.is_empty() && !clean_proj.contains(' ') {
                    filter.projection = JqProjection::Path(format!(".{}", clean_proj));
                }

                Ok(Filter::Jq(filter))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::CompiledFilter;

    #[test]
    fn test_mock_synthesize_grep() {
        let synth = MockSynthesizer::new();
        let samples = vec!["2026-11-16 500 Internal Error".to_string()];
        let filter = synth.synthesize(FilterKind::Grep, "5xx errors", &samples).expect("synth");
        assert!(filter.matches("server returned 503"));
        assert!(!filter.matches("server returned 200"));
    }

    #[test]
    fn test_mock_synthesize_awk() {
        let synth = MockSynthesizer::new();
        let samples = vec!["a,b,c".to_string()];
        let filter = synth.synthesize(FilterKind::Awk, "csv $1 - $3", &samples).expect("synth");
        assert_eq!(filter.transform("foo,bar,baz"), Some("foo - baz".to_string()));
    }

    #[test]
    fn test_mock_synthesize_jq() {
        let synth = MockSynthesizer::new();
        let samples = vec![r#"{"user":"admin","latency":600}"#.to_string()];
        let filter = synth.synthesize(FilterKind::Jq, "{user} where latency > 500", &samples).expect("synth");
        let res = filter.transform(r#"{"user":"admin","latency":600}"#).expect("transform");
        assert!(res.contains("admin"));
    }
}
