// NoTargetReason::DeclarationOnly — a `.d.ts` declaration file with no
// runnable definitions.
export interface UserRecord {
  id: number;
  email: string;
}

export type Role = "admin" | "user" | "guest";
