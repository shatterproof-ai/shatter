//! Stratum selection: explore functions at specific call graph depths.
//!
//! Supports single layers, ranges (inclusive), open-ended ranges, and negative
//! indices (where -0 = max depth, -1 = second from top, etc.).
//!
//! Syntax examples:
//! - `"0"` — layer 0 only (leaf functions)
//! - `"0..3"` — layers 0 through 3 inclusive
//! - `"3.."` — layer 3 and above
//! - `"..3"` — layers 0 through 3
//! - `"-0"` — top layer (entry points)
//! - `"-2..-0"` — top 3 layers

use std::ops::RangeInclusive;

use crate::scan_orchestrator::ScanError;

/// A parsed stratum specification.
#[derive(Debug, Clone)]
pub struct StratumSpec {
    kind: SpecKind,
}

#[derive(Debug, Clone)]
enum SpecKind {
    /// A single layer index.
    Single(LayerIndex),
    /// An inclusive range of layers.
    Range {
        start: Option<LayerIndex>,
        end: Option<LayerIndex>,
    },
}

/// A layer index, possibly negative (counted from the top).
#[derive(Debug, Clone, Copy)]
struct LayerIndex {
    /// The numeric value (always non-negative).
    value: usize,
    /// Whether this is a negative index (counted from max depth).
    negative: bool,
}

impl LayerIndex {
    /// Resolve this index to an absolute layer number given the max layer.
    fn resolve(self, max_layer: usize) -> usize {
        if self.negative {
            // -0 = max_layer, -1 = max_layer - 1, etc.
            max_layer.saturating_sub(self.value)
        } else {
            self.value
        }
    }
}

/// Parse a stratum specification string.
///
/// Accepted formats:
/// - `"N"` or `"-N"` — single layer
/// - `"A..B"` — inclusive range (A and B may be negative)
/// - `"A.."` — open-ended range from A
/// - `"..B"` — open-ended range to B
pub fn parse_stratum_spec(s: &str) -> Result<StratumSpec, String> {
    let s = s.trim();

    if let Some((left, right)) = s.split_once("..") {
        let start = if left.is_empty() {
            None
        } else {
            Some(parse_layer_index(left)?)
        };
        let end = if right.is_empty() {
            None
        } else {
            Some(parse_layer_index(right)?)
        };
        if start.is_none() && end.is_none() {
            return Err("stratum range must have at least one bound: \"..\" is not valid".into());
        }
        Ok(StratumSpec {
            kind: SpecKind::Range { start, end },
        })
    } else {
        let idx = parse_layer_index(s)?;
        Ok(StratumSpec {
            kind: SpecKind::Single(idx),
        })
    }
}

fn parse_layer_index(s: &str) -> Result<LayerIndex, String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('-') {
        let value: usize = rest
            .parse()
            .map_err(|_| format!("invalid layer index: {s}"))?;
        Ok(LayerIndex {
            value,
            negative: true,
        })
    } else {
        let value: usize = s
            .parse()
            .map_err(|_| format!("invalid layer index: {s}"))?;
        Ok(LayerIndex {
            value,
            negative: false,
        })
    }
}

/// Resolve a stratum spec into an inclusive range of absolute layer indices.
///
/// Returns `Err(ScanError)` if the resolved range is empty or out of bounds.
pub fn resolve_range(
    spec: &StratumSpec,
    max_layer: usize,
) -> Result<RangeInclusive<usize>, ScanError> {
    let (start, end) = match &spec.kind {
        SpecKind::Single(idx) => {
            let resolved = idx.resolve(max_layer);
            (resolved, resolved)
        }
        SpecKind::Range { start, end } => {
            let s = start.map_or(0, |i| i.resolve(max_layer));
            let e = end.map_or(max_layer, |i| i.resolve(max_layer));
            (s, e)
        }
    };

    // Clamp to valid range.
    let start = start.min(max_layer);
    let end = end.min(max_layer);

    if start > end {
        return Err(ScanError::Stratum(format!(
            "resolved range {start}..={end} is empty (start > end after resolving negative indices)"
        )));
    }

    Ok(start..=end)
}

/// Filter layers to only those within the given range.
///
/// Returns `(original_layer_index, &functions)` pairs for layers in the range.
pub fn filter_layers<'a>(
    layers: &'a [Vec<String>],
    range: &RangeInclusive<usize>,
) -> Vec<(usize, &'a Vec<String>)> {
    layers
        .iter()
        .enumerate()
        .filter(|(idx, _)| range.contains(idx))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_positive() {
        let spec = parse_stratum_spec("3").unwrap();
        assert!(matches!(spec.kind, SpecKind::Single(LayerIndex { value: 3, negative: false })));
    }

    #[test]
    fn parse_single_negative() {
        let spec = parse_stratum_spec("-0").unwrap();
        assert!(matches!(spec.kind, SpecKind::Single(LayerIndex { value: 0, negative: true })));
    }

    #[test]
    fn parse_range_both_bounds() {
        let spec = parse_stratum_spec("1..3").unwrap();
        match spec.kind {
            SpecKind::Range { start: Some(s), end: Some(e) } => {
                assert_eq!(s.value, 1);
                assert!(!s.negative);
                assert_eq!(e.value, 3);
                assert!(!e.negative);
            }
            _ => panic!("expected range with both bounds"),
        }
    }

    #[test]
    fn parse_range_open_start() {
        let spec = parse_stratum_spec("..3").unwrap();
        match spec.kind {
            SpecKind::Range { start: None, end: Some(e) } => {
                assert_eq!(e.value, 3);
            }
            _ => panic!("expected range with open start"),
        }
    }

    #[test]
    fn parse_range_open_end() {
        let spec = parse_stratum_spec("3..").unwrap();
        match spec.kind {
            SpecKind::Range { start: Some(s), end: None } => {
                assert_eq!(s.value, 3);
            }
            _ => panic!("expected range with open end"),
        }
    }

    #[test]
    fn parse_range_negative_indices() {
        let spec = parse_stratum_spec("-2..-0").unwrap();
        match spec.kind {
            SpecKind::Range { start: Some(s), end: Some(e) } => {
                assert!(s.negative);
                assert_eq!(s.value, 2);
                assert!(e.negative);
                assert_eq!(e.value, 0);
            }
            _ => panic!("expected range with negative indices"),
        }
    }

    #[test]
    fn parse_empty_range_is_error() {
        assert!(parse_stratum_spec("..").is_err());
    }

    #[test]
    fn parse_invalid_is_error() {
        assert!(parse_stratum_spec("abc").is_err());
    }

    #[test]
    fn resolve_single_positive() {
        let spec = parse_stratum_spec("2").unwrap();
        let range = resolve_range(&spec, 5).unwrap();
        assert_eq!(range, 2..=2);
    }

    #[test]
    fn resolve_single_negative_zero() {
        let spec = parse_stratum_spec("-0").unwrap();
        let range = resolve_range(&spec, 5).unwrap();
        assert_eq!(range, 5..=5);
    }

    #[test]
    fn resolve_single_negative_one() {
        let spec = parse_stratum_spec("-1").unwrap();
        let range = resolve_range(&spec, 5).unwrap();
        assert_eq!(range, 4..=4);
    }

    #[test]
    fn resolve_range_positive() {
        let spec = parse_stratum_spec("1..3").unwrap();
        let range = resolve_range(&spec, 5).unwrap();
        assert_eq!(range, 1..=3);
    }

    #[test]
    fn resolve_range_open_end() {
        let spec = parse_stratum_spec("2..").unwrap();
        let range = resolve_range(&spec, 5).unwrap();
        assert_eq!(range, 2..=5);
    }

    #[test]
    fn resolve_range_open_start() {
        let spec = parse_stratum_spec("..3").unwrap();
        let range = resolve_range(&spec, 5).unwrap();
        assert_eq!(range, 0..=3);
    }

    #[test]
    fn resolve_range_negative() {
        let spec = parse_stratum_spec("-2..-0").unwrap();
        let range = resolve_range(&spec, 5).unwrap();
        assert_eq!(range, 3..=5);
    }

    #[test]
    fn resolve_clamps_beyond_max() {
        let spec = parse_stratum_spec("10").unwrap();
        let range = resolve_range(&spec, 3).unwrap();
        assert_eq!(range, 3..=3);
    }

    #[test]
    fn resolve_negative_beyond_max_clamps_to_zero() {
        let spec = parse_stratum_spec("-10").unwrap();
        let range = resolve_range(&spec, 3).unwrap();
        assert_eq!(range, 0..=0);
    }

    #[test]
    fn filter_layers_selects_correct_range() {
        let layers = vec![
            vec!["a".into()],
            vec!["b".into()],
            vec!["c".into()],
            vec!["d".into()],
        ];
        let result = filter_layers(&layers, &(1..=2));
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, 1);
        assert_eq!(result[1].0, 2);
    }

    #[test]
    fn filter_layers_empty_range() {
        let layers = vec![vec!["a".into()], vec!["b".into()]];
        let result = filter_layers(&layers, &(5..=7));
        assert!(result.is_empty());
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        /// Generate a valid stratum spec string.
        fn arb_stratum_spec() -> impl Strategy<Value = String> {
            prop_oneof![
                // Single positive index
                (0..100usize).prop_map(|v| v.to_string()),
                // Single negative index
                (0..100usize).prop_map(|v| format!("-{v}")),
                // Range with both bounds (positive)
                (0..50usize, 0..50usize).prop_map(|(a, b)| format!("{a}..{b}")),
                // Range with both bounds (negative)
                (0..50usize, 0..50usize).prop_map(|(a, b)| format!("-{a}..-{b}")),
                // Range with open start
                (0..100usize).prop_map(|v| format!("..{v}")),
                // Range with open start (negative end)
                (0..100usize).prop_map(|v| format!("..-{v}")),
                // Range with open end
                (0..100usize).prop_map(|v| format!("{v}..")),
                // Range with open end (negative start)
                (0..100usize).prop_map(|v| format!("-{v}..")),
                // Mixed: positive start, negative end
                (0..50usize, 0..50usize).prop_map(|(a, b)| format!("{a}..-{b}")),
                // Mixed: negative start, positive end
                (0..50usize, 0..50usize).prop_map(|(a, b)| format!("-{a}..{b}")),
            ]
        }

        proptest! {
            /// Any valid spec string parses successfully.
            #[test]
            fn valid_specs_always_parse(spec_str in arb_stratum_spec()) {
                prop_assert!(parse_stratum_spec(&spec_str).is_ok(),
                    "failed to parse valid spec: {}", spec_str);
            }

            /// Resolved range bounds are always within 0..=max_layer.
            #[test]
            fn resolved_range_within_bounds(
                spec_str in arb_stratum_spec(),
                max_layer in 0..50usize,
            ) {
                let spec = parse_stratum_spec(&spec_str).unwrap();
                if let Ok(range) = resolve_range(&spec, max_layer) {
                    prop_assert!(*range.start() <= max_layer,
                        "start {} > max_layer {}", range.start(), max_layer);
                    prop_assert!(*range.end() <= max_layer,
                        "end {} > max_layer {}", range.end(), max_layer);
                    prop_assert!(range.start() <= range.end(),
                        "start {} > end {}", range.start(), range.end());
                }
            }

            /// Negative-zero always resolves to max_layer.
            #[test]
            fn negative_zero_is_max_layer(max_layer in 0..100usize) {
                let spec = parse_stratum_spec("-0").unwrap();
                let range = resolve_range(&spec, max_layer).unwrap();
                prop_assert_eq!(*range.start(), max_layer);
                prop_assert_eq!(*range.end(), max_layer);
            }

            /// Open-ended ranges span the full extent in their open direction.
            #[test]
            fn open_start_begins_at_zero(end_val in 0..50usize, max_layer in 0..50usize) {
                let spec = parse_stratum_spec(&format!("..{end_val}")).unwrap();
                if let Ok(range) = resolve_range(&spec, max_layer) {
                    prop_assert_eq!(*range.start(), 0);
                }
            }

            #[test]
            fn open_end_reaches_max_layer(start_val in 0..50usize, max_layer in 0..50usize) {
                let spec = parse_stratum_spec(&format!("{start_val}..")).unwrap();
                if let Ok(range) = resolve_range(&spec, max_layer) {
                    prop_assert_eq!(*range.end(), max_layer);
                }
            }

            /// filter_layers returns only indices within the given range.
            #[test]
            fn filter_layers_indices_in_range(
                num_layers in 1..20usize,
                range_start in 0..20usize,
                range_end in 0..20usize,
            ) {
                let layers: Vec<Vec<String>> = (0..num_layers)
                    .map(|i| vec![format!("fn_{i}")])
                    .collect();
                let start = range_start.min(range_end);
                let end = range_start.max(range_end);
                let range = start..=end;
                let result = filter_layers(&layers, &range);
                for (idx, _) in &result {
                    prop_assert!(range.contains(idx),
                        "filtered index {} not in range {:?}", idx, range);
                    prop_assert!(*idx < num_layers,
                        "filtered index {} >= num_layers {}", idx, num_layers);
                }
                // Count should match the overlap between range and 0..num_layers.
                let expected = (start..=end).filter(|i| *i < num_layers).count();
                prop_assert_eq!(result.len(), expected);
            }

            /// Arbitrary strings never cause panics in parse (may return Err).
            #[test]
            fn arbitrary_input_never_panics(s in ".*") {
                let _ = parse_stratum_spec(&s);
            }
        }
    }
}
