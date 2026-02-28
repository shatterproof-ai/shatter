// Example 10: HTTP path routing and middleware resolution
// Tests shatter's ability to reason about string patterns, array lookups,
// and multi-condition dispatch. Inspired by Express.js routing patterns (MIT).

// Simple route matcher supporting static segments, named params (:id),
// and wildcards (*).
//
// EXPECTED BRANCHES (12):
//   1. path is empty                             → "error: empty path"
//   2. path doesn't start with "/"               → "error: path must start with /"
//   3. method is empty                           → "error: empty method"
//   4. no routes defined                         → "not-found"
//   5. exact static match found                  → returns matched route
//   6. parameterized match (:param) found        → returns route with extracted params
//   7. wildcard match (*) found                  → returns route with wildcard segment
//   8. method matches path but wrong HTTP method → "method-not-allowed"
//   9. no match found                            → "not-found"
//  10. multiple matches, first wins              → returns first matched route
//  11. trailing slash normalization (path/)       → matches path without slash
//  12. route with multiple params                → extracts all params
//
// DIFFICULTY: Hard. Requires generating arrays of route definitions with
// specific pattern strings, then paths that match or don't match those
// patterns. The segment-by-segment matching logic is hard to satisfy randomly.

interface Route {
    method: string;
    pattern: string;
    handler: string;
}

interface MatchResult {
    status: "matched" | "not-found" | "method-not-allowed" | "error";
    handler?: string;
    params?: Record<string, string>;
    message?: string;
}

export function matchRoute(
    routes: Route[],
    method: string,
    path: string
): MatchResult {
    if (path.length === 0) {
        return { status: "error", message: "empty path" };
    }
    if (!path.startsWith("/")) {
        return { status: "error", message: "path must start with /" };
    }
    if (method.length === 0) {
        return { status: "error", message: "empty method" };
    }
    if (routes.length === 0) {
        return { status: "not-found" };
    }

    // Normalize: strip trailing slash (except for root "/")
    const normalizedPath = path.length > 1 && path.endsWith("/")
        ? path.slice(0, -1)
        : path;

    const pathSegments = normalizedPath.split("/").filter(s => s.length > 0);
    const upperMethod = method.toUpperCase();

    let pathMatchedButMethodDiffers = false;

    for (const route of routes) {
        const patternSegments = route.pattern.split("/").filter(s => s.length > 0);

        // Wildcard: pattern ending with * matches any path prefix
        if (patternSegments.length > 0 && patternSegments[patternSegments.length - 1] === "*") {
            const prefixSegments = patternSegments.slice(0, -1);
            if (pathSegments.length >= prefixSegments.length) {
                let prefixMatches = true;
                const params: Record<string, string> = {};

                for (let i = 0; i < prefixSegments.length; i++) {
                    const ps = prefixSegments[i];
                    if (ps.startsWith(":")) {
                        params[ps.slice(1)] = pathSegments[i];
                    } else if (ps !== pathSegments[i]) {
                        prefixMatches = false;
                        break;
                    }
                }

                if (prefixMatches) {
                    if (route.method.toUpperCase() === upperMethod) {
                        params["wildcard"] = pathSegments.slice(prefixSegments.length).join("/");
                        return { status: "matched", handler: route.handler, params };
                    }
                    pathMatchedButMethodDiffers = true;
                }
            }
            continue;
        }

        // Non-wildcard: segment count must match
        if (patternSegments.length !== pathSegments.length) {
            continue;
        }

        let matches = true;
        const params: Record<string, string> = {};

        for (let i = 0; i < patternSegments.length; i++) {
            const ps = patternSegments[i];
            if (ps.startsWith(":")) {
                params[ps.slice(1)] = pathSegments[i];
            } else if (ps !== pathSegments[i]) {
                matches = false;
                break;
            }
        }

        if (matches) {
            if (route.method.toUpperCase() === upperMethod) {
                return { status: "matched", handler: route.handler, params };
            }
            pathMatchedButMethodDiffers = true;
        }
    }

    if (pathMatchedButMethodDiffers) {
        return { status: "method-not-allowed" };
    }

    return { status: "not-found" };
}

// Middleware chain resolution based on route metadata.
//
// EXPECTED BRANCHES (8):
//   1. requiresAuth && no authHeader              → "reject: missing auth"
//   2. requiresAuth && authHeader invalid format  → "reject: invalid auth format"
//   3. requiresAuth && valid auth                 → adds auth middleware
//   4. contentType is "application/json"          → adds JSON parser middleware
//   5. contentType is "multipart/form-data"       → adds multipart middleware
//   6. corsOrigin provided && in allowedOrigins   → adds CORS middleware
//   7. corsOrigin provided && not in allowed      → "reject: origin not allowed"
//   8. no special middleware needed               → returns base chain only
//
// DIFFICULTY: Medium. Requires matching specific string values across
// multiple independent conditions that combine into a middleware chain.

interface RouteMetadata {
    requiresAuth: boolean;
    contentType: string;
    corsOrigin: string | null;
    allowedOrigins: string[];
}

interface MiddlewareChain {
    status: "ok" | "rejected";
    middleware: string[];
    reason?: string;
}

export function resolveMiddleware(
    metadata: RouteMetadata,
    authHeader: string | null
): MiddlewareChain {
    const chain: string[] = ["logging", "error-handler"];

    if (metadata.requiresAuth) {
        if (!authHeader) {
            return { status: "rejected", middleware: [], reason: "missing auth" };
        }
        if (!authHeader.startsWith("Bearer ") || authHeader.length <= 7) {
            return { status: "rejected", middleware: [], reason: "invalid auth format" };
        }
        chain.push("auth");
    }

    if (metadata.contentType === "application/json") {
        chain.push("json-parser");
    } else if (metadata.contentType === "multipart/form-data") {
        chain.push("multipart-parser");
    }

    if (metadata.corsOrigin !== null) {
        if (metadata.allowedOrigins.includes(metadata.corsOrigin)) {
            chain.push("cors");
        } else {
            return { status: "rejected", middleware: [], reason: "origin not allowed" };
        }
    }

    return { status: "ok", middleware: chain };
}
