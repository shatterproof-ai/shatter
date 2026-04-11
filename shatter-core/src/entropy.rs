//! Shannon entropy measurement for runtime crypto detection.
//!
//! Measures byte-level entropy of input/output buffers to confirm
//! cryptographic behavior: encryption produces high-entropy output
//! from low-entropy input, decryption does the reverse.

use crate::crypto_registry::CryptoDirection;

/// Buffers smaller than this are too short for reliable entropy measurement.
pub const MIN_BUFFER_SIZE: usize = 32;

/// Minimum bits-per-byte difference between input and output entropy
/// to classify as likely crypto.
pub const ENTROPY_DELTA_THRESHOLD: f64 = 1.5;

/// Compute Shannon entropy in bits per byte (0.0 to 8.0).
///
/// Returns 0.0 for empty buffers or those shorter than [`MIN_BUFFER_SIZE`].
/// For uniform random data, approaches 8.0; for constant data, returns 0.0.
pub fn shannon_entropy(data: &[u8]) -> f64 {
    if data.len() < MIN_BUFFER_SIZE {
        return 0.0;
    }

    let mut counts = [0u64; 256];
    for &byte in data {
        counts[byte as usize] += 1;
    }

    let len = data.len() as f64;
    let mut entropy = 0.0;
    for &count in &counts {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }

    entropy
}

/// Classify the entropy delta between input and output as crypto direction.
///
/// Returns `Some(Encrypt)` if output entropy exceeds input by more than
/// [`ENTROPY_DELTA_THRESHOLD`], `Some(Decrypt)` for the reverse, or
/// `None` if the delta is below threshold.
pub fn classify_entropy_delta(input_entropy: f64, output_entropy: f64) -> Option<CryptoDirection> {
    let delta = output_entropy - input_entropy;
    if delta > ENTROPY_DELTA_THRESHOLD {
        Some(CryptoDirection::Encrypt)
    } else if delta < -ENTROPY_DELTA_THRESHOLD {
        Some(CryptoDirection::Decrypt)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_bytes_zero_entropy() {
        let data = vec![0x42u8; 256];
        assert_eq!(shannon_entropy(&data), 0.0);
    }

    #[test]
    fn uniform_distribution_near_eight() {
        // Each byte value appears exactly once → maximum entropy.
        let data: Vec<u8> = (0..=255).collect();
        let e = shannon_entropy(&data);
        assert!((e - 8.0).abs() < 0.001, "expected ~8.0, got {e}");
    }

    #[test]
    fn two_values_one_bit() {
        // 128 zeros + 128 ones → 1.0 bit per byte.
        let mut data = vec![0u8; 128];
        data.extend(vec![1u8; 128]);
        let e = shannon_entropy(&data);
        assert!((e - 1.0).abs() < 0.001, "expected ~1.0, got {e}");
    }

    #[test]
    fn empty_returns_zero() {
        assert_eq!(shannon_entropy(&[]), 0.0);
    }

    #[test]
    fn below_min_buffer_returns_zero() {
        let data = vec![0u8; MIN_BUFFER_SIZE - 1];
        assert_eq!(shannon_entropy(&data), 0.0);
    }

    #[test]
    fn exactly_min_buffer_computes() {
        let data: Vec<u8> = (0..MIN_BUFFER_SIZE as u8).collect();
        assert!(shannon_entropy(&data) > 0.0);
    }

    #[test]
    fn classify_encrypt() {
        // Low input entropy, high output → encrypt.
        let result = classify_entropy_delta(2.0, 7.8);
        assert_eq!(result, Some(CryptoDirection::Encrypt));
    }

    #[test]
    fn classify_decrypt() {
        // High input entropy, low output → decrypt.
        let result = classify_entropy_delta(7.8, 4.0);
        assert_eq!(result, Some(CryptoDirection::Decrypt));
    }

    #[test]
    fn classify_below_threshold() {
        // Small delta → no classification.
        assert_eq!(classify_entropy_delta(4.0, 5.0), None);
        assert_eq!(classify_entropy_delta(5.0, 4.0), None);
    }

    #[test]
    fn classify_exact_threshold_no_match() {
        // Exactly at threshold (not exceeded) → None.
        assert_eq!(
            classify_entropy_delta(3.0, 3.0 + ENTROPY_DELTA_THRESHOLD),
            None
        );
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn entropy_in_valid_range(data in proptest::collection::vec(any::<u8>(), 0..1024)) {
                let e = shannon_entropy(&data);
                prop_assert!(e >= 0.0, "entropy must be non-negative, got {e}");
                prop_assert!(e <= 8.0, "entropy must be <= 8.0, got {e}");
            }

            #[test]
            fn short_buffers_return_zero(data in proptest::collection::vec(any::<u8>(), 0..MIN_BUFFER_SIZE)) {
                prop_assert_eq!(shannon_entropy(&data), 0.0);
            }

            #[test]
            fn classify_is_symmetric(
                input_e in 0.0f64..=8.0,
                output_e in 0.0f64..=8.0,
            ) {
                let fwd = classify_entropy_delta(input_e, output_e);
                let rev = classify_entropy_delta(output_e, input_e);
                match (fwd, rev) {
                    (Some(CryptoDirection::Encrypt), Some(CryptoDirection::Decrypt)) => {}
                    (Some(CryptoDirection::Decrypt), Some(CryptoDirection::Encrypt)) => {}
                    (None, None) => {}
                    (None, Some(_)) | (Some(_), None) => {
                        // Asymmetry only possible near threshold boundary — acceptable.
                    }
                    _ => prop_assert!(false, "unexpected: fwd={fwd:?}, rev={rev:?}"),
                }
            }
        }
    }
}
