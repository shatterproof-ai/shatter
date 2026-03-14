/**
 * Shannon entropy measurement for runtime crypto detection.
 *
 * Measures byte-level entropy of buffers to confirm cryptographic behavior:
 * encryption produces high-entropy output from low-entropy input,
 * decryption does the reverse.
 */

/** Buffers smaller than this are too short for reliable entropy measurement. */
export const MIN_BUFFER_SIZE = 32;

/** Minimum bits/byte difference to classify as likely crypto. */
export const ENTROPY_DELTA_THRESHOLD = 1.5;

/**
 * Compute Shannon entropy in bits per byte (0.0 to 8.0).
 *
 * Returns 0.0 for empty buffers or those shorter than {@link MIN_BUFFER_SIZE}.
 * For uniform random data, approaches 8.0; for constant data, returns 0.0.
 */
export function shannonEntropy(data: Buffer): number {
  if (data.length < MIN_BUFFER_SIZE) {
    return 0.0;
  }

  const counts = new Float64Array(256);
  for (let i = 0; i < data.length; i++) {
    const byte = data[i] as number;
    counts[byte] = (counts[byte] as number) + 1;
  }

  const len = data.length;
  let entropy = 0.0;
  for (let i = 0; i < 256; i++) {
    const count = counts[i] as number;
    if (count > 0) {
      const p = count / len;
      entropy -= p * Math.log2(p);
    }
  }

  return entropy;
}
