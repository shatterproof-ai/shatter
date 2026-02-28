/**
 * Test fixture: a generator module that exports named generator functions.
 */

export function User(): { id: number; name: string; email: string } {
  return { id: 1, name: "Alice", email: "alice@example.com" };
}

export function authToken(): string {
  return "tok_test_abc123";
}

export function count(): number {
  return 42;
}
