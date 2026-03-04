// Example 7: Auth and token validation
// Tests shatter's ability to reason about string structure, JSON parsing,
// and multi-step validation pipelines. These patterns are ubiquitous in
// web backends and security-critical code.

// EXPECTED BRANCHES (10):
//   1. token is empty                            → "invalid: empty token"
//   2. token doesn't have 3 dot-separated parts  → "invalid: malformed token"
//   3. header JSON parse fails                   → "invalid: corrupt header"
//   4. header.alg !== "HS256"                    → "invalid: unsupported algorithm"
//   5. payload JSON parse fails                  → "invalid: corrupt payload"
//   6. payload.exp < now (expired)               → "invalid: token expired"
//   7. payload.iss !== expectedIssuer            → "invalid: wrong issuer"
//   8. payload.aud doesn't include audience      → "invalid: wrong audience"
//   9. payload.scope missing required scope      → "invalid: insufficient scope"
//  10. all checks pass                           → "valid"
//
// DIFFICULTY: Hard. The token must be a well-formed string with exactly
// two dots, where each segment decodes from base64 to valid JSON with
// specific field values. Random guessing essentially never produces this.

export function validateJwt(
    token: string,
    expectedIssuer: string,
    expectedAudience: string,
    requiredScope: string,
    nowEpochSeconds: number
): string {
    if (token.length === 0) {
        return "invalid: empty token";
    }

    const parts = token.split(".");
    if (parts.length !== 3) {
        return "invalid: malformed token";
    }

    let header: { alg?: string };
    try {
        header = JSON.parse(Buffer.from(parts[0], "base64").toString());
    } catch {
        return "invalid: corrupt header";
    }

    if (header.alg !== "HS256") {
        return "invalid: unsupported algorithm";
    }

    let payload: {
        exp?: number;
        iss?: string;
        aud?: string | string[];
        scope?: string;
    };
    try {
        payload = JSON.parse(Buffer.from(parts[1], "base64").toString());
    } catch {
        return "invalid: corrupt payload";
    }

    if (typeof payload.exp === "number" && payload.exp < nowEpochSeconds) {
        return "invalid: token expired";
    }

    if (payload.iss !== expectedIssuer) {
        return "invalid: wrong issuer";
    }

    const audiences = Array.isArray(payload.aud)
        ? payload.aud
        : [payload.aud];
    if (!audiences.includes(expectedAudience)) {
        return "invalid: wrong audience";
    }

    const scopes = (payload.scope ?? "").split(" ");
    if (!scopes.includes(requiredScope)) {
        return "invalid: insufficient scope";
    }

    return "valid";
}

// Role-based access control.
//
// EXPECTED BRANCHES (12):
//   1. user.roles is empty                       → "denied: no roles"
//   2. user.active === false                     → "denied: inactive user"
//   3. resource is empty string                  → "denied: invalid resource"
//   4. action not in allowed actions             → "denied: invalid action"
//   5. user has "superadmin" role                → "granted: superadmin"
//   6. user is resource owner AND action is "read" → "granted: owner-read"
//   7. user is resource owner AND action is "write" → "granted: owner-write"
//   8. user is resource owner AND action is "delete" → "granted: owner-delete"
//   9. user has "admin" role AND action is "read" → "granted: admin-read"
//  10. user has "admin" role AND action is "write" → "granted: admin-write"
//  11. user has "admin" role AND action is "delete" → "denied: admin-no-delete"
//  12. user has only "viewer" role               → action === "read" ? "granted: viewer" : "denied: viewer-readonly"
//
// DIFFICULTY: Medium-hard. Requires specific combinations of roles, ownership
// status, and actions. The priority ordering (superadmin > owner > admin > viewer)
// means the solver must understand which checks happen first.

interface User {
    id: string;
    roles: string[];
    active: boolean;
}

const VALID_ACTIONS = ["read", "write", "delete"] as const;
type Action = typeof VALID_ACTIONS[number];

export function authorizeRequest(
    user: User,
    resource: string,
    resourceOwnerId: string,
    action: string
): string {
    if (user.roles.length === 0) {
        return "denied: no roles";
    }

    if (!user.active) {
        return "denied: inactive user";
    }

    if (resource.length === 0) {
        return "denied: invalid resource";
    }

    if (!VALID_ACTIONS.includes(action as Action)) {
        return "denied: invalid action";
    }

    if (user.roles.includes("superadmin")) {
        return "granted: superadmin";
    }

    const isOwner = user.id === resourceOwnerId;

    if (isOwner) {
        if (action === "read") {
            return "granted: owner-read";
        }
        if (action === "write") {
            return "granted: owner-write";
        }
        if (action === "delete") {
            return "granted: owner-delete";
        }
    }

    if (user.roles.includes("admin")) {
        if (action === "read") {
            return "granted: admin-read";
        }
        if (action === "write") {
            return "granted: admin-write";
        }
        return "denied: admin-no-delete";
    }

    if (user.roles.includes("viewer")) {
        if (action === "read") {
            return "granted: viewer";
        }
        return "denied: viewer-readonly";
    }

    return "denied: no matching role";
}
