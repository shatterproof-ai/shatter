// Example 5: Functions with enum/union parameters
// Tests shatter's ability to enumerate discriminated union variants.

type Shape =
    | { kind: "circle"; radius: number }
    | { kind: "rectangle"; width: number; height: number }
    | { kind: "triangle"; base: number; height: number };

// EXPECTED BRANCHES:
//   1. kind === "circle"    AND radius <= 0    → throws Error("non-positive radius")
//   2. kind === "circle"    AND radius > 0     → returns π * r²
//   3. kind === "rectangle" AND w/h <= 0       → throws Error("non-positive dimension")
//   4. kind === "rectangle" AND w/h > 0        → returns w * h
//   5. kind === "triangle"  AND base/h <= 0    → throws Error("non-positive dimension")
//   6. kind === "triangle"  AND base/h > 0     → returns 0.5 * base * h

export function computeArea(shape: Shape): number {
    switch (shape.kind) {
        case "circle":
            if (shape.radius <= 0) {
                throw new Error("non-positive radius");
            }
            return Math.PI * shape.radius * shape.radius;

        case "rectangle":
            if (shape.width <= 0 || shape.height <= 0) {
                throw new Error("non-positive dimension");
            }
            return shape.width * shape.height;

        case "triangle":
            if (shape.base <= 0 || shape.height <= 0) {
                throw new Error("non-positive dimension");
            }
            return 0.5 * shape.base * shape.height;
    }
}

type HttpMethod = "GET" | "POST" | "PUT" | "DELETE";

interface ApiRequest {
    method: HttpMethod;
    path: string;
    body?: string;
    authenticated: boolean;
}

// EXPECTED BRANCHES:
//   1. path is empty                          → throws Error("empty path")
//   2. method === "GET"                       → "read"
//   3. method === "DELETE" AND !authenticated  → throws Error("auth required")
//   4. method === "DELETE" AND authenticated   → "delete"
//   5. method === "POST"  AND no body         → throws Error("body required")
//   6. method === "POST"  AND has body        → "create"
//   7. method === "PUT"   AND no body         → throws Error("body required")
//   8. method === "PUT"   AND has body        → "update"

export function routeRequest(req: ApiRequest): string {
    if (req.path.length === 0) {
        throw new Error("empty path");
    }

    switch (req.method) {
        case "GET":
            return "read";

        case "DELETE":
            if (!req.authenticated) {
                throw new Error("auth required");
            }
            return "delete";

        case "POST":
            if (!req.body) {
                throw new Error("body required");
            }
            return "create";

        case "PUT":
            if (!req.body) {
                throw new Error("body required");
            }
            return "update";
    }
}
