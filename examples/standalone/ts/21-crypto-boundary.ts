// Example 21: crypto boundary — AES-256-CBC decrypt-then-branch
//
// Demonstrates the crypto boundary feature: a function that decrypts its input
// before branching on the plaintext.  Z3 cannot invert AES, but the shatter
// runtime intercepts the decrypt call and records the (ciphertext → plaintext)
// mapping so the orchestrator can use it as a witness oracle.
//
// EXPECTED BRANCHES for classifySecret (3):
//   1. plaintext is exactly "admin"          -> role "superuser"
//   2. plaintext starts with "user:"        -> role "user" + username slice
//   3. anything else                         -> role "unknown"
//
// TRIGGERING INPUTS (pre-encrypted ciphertexts):
//   - encrypt("admin")    with the hardcoded KEY/IV -> branch 1
//   - encrypt("user:bob") with the hardcoded KEY/IV -> branch 2
//   - encrypt("guest")    with the hardcoded KEY/IV -> branch 3
//
// DIFFICULTY: Requires crypto oracle; Z3 alone cannot reach branches 1 or 2.

import * as crypto from "crypto";

// Fixed key/IV for deterministic test vectors (32-byte key, 16-byte IV).
const KEY = Buffer.from("0123456789abcdef0123456789abcdef", "utf8"); // 32 bytes
const IV = Buffer.from("abcdef0123456789", "utf8"); // 16 bytes

/** Encrypt plaintext using AES-256-CBC (used only to produce test vectors). */
export function encrypt(plaintext: string): Buffer {
  const cipher = crypto.createCipheriv("aes-256-cbc", KEY, IV);
  return Buffer.concat([cipher.update(plaintext, "utf8"), cipher.final()]);
}

/**
 * Decrypt a ciphertext using AES-256-CBC and branch on the plaintext content.
 *
 * This is the function under test.  The crypto boundary feature intercepts
 * the `createDecipheriv` call so the engine can record the plaintext and use
 * it to craft inputs that reach each branch without inverting AES.
 */
export function classifySecret(ciphertext: Buffer): {
  role: string;
  username?: string;
} {
  const decipher = crypto.createDecipheriv("aes-256-cbc", KEY, IV);
  const plaintext = Buffer.concat([
    decipher.update(ciphertext),
    decipher.final(),
  ]).toString("utf8");

  if (plaintext === "admin") {
    return { role: "superuser" };
  }
  if (plaintext.startsWith("user:")) {
    return { role: "user", username: plaintext.slice(5) };
  }
  return { role: "unknown" };
}
