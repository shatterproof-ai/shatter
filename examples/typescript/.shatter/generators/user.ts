// Type-name generator for the UserProfile type used in 03-objects.ts.
//
// When Shatter encounters a parameter typed as UserProfile, it calls this
// generator to produce a domain-realistic seed value instead of relying
// solely on random/boundary values.
//
// Usage in .shatter/config.yaml:
//   defaults:
//     generators:
//       UserProfile: ./generators/user.ts

export function UserProfile(): {
  name: string;
  age: number;
  isVerified: boolean;
  role: string;
} {
  return { name: "Alice", age: 25, isVerified: true, role: "admin" };
}
