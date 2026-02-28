// Example 6: Complex nested control flow
// Tests shatter's ability to explore deeply nested conditionals and state machines.
// These patterns appear throughout real-world code and require the solver to
// satisfy multiple layered constraints simultaneously.

// EXPECTED BRANCHES (15):
//   1. status < 100                              → throws Error("invalid status")
//   2. status >= 600                             → throws Error("invalid status")
//   3. status 100-199, informational             → "informational"
//   4. status 200, contentType starts "text/"    → "ok-text"
//   5. status 200, contentType starts "application/json" → "ok-json"
//   6. status 200, other contentType             → "ok-binary"
//   7. status 201, body present                  → "created-with-body"
//   8. status 201, body absent                   → "created-empty"
//   9. status 204                                → "no-content"
//  10. status 202-203 or 205-299                 → "success-other"
//  11. status 301 or 302                         → "redirect"
//  12. status 300 or 303-399                     → "redirect-other"
//  13. status 400-499, status 401 or 403         → "auth-error"
//  14. status 400-499, other                     → "client-error"
//  15. status 500-599                            → "server-error"
//
// DIFFICULTY: Medium-hard. Random guessing struggles because it must hit
// specific numeric ranges AND match string prefixes within those ranges.
// The solver must combine numeric interval constraints with string constraints.

export function classifyHttpResponse(
    status: number,
    contentType: string,
    body: string | null
): string {
    if (status < 100 || status >= 600) {
        throw new Error("invalid status");
    }

    if (status < 200) {
        return "informational";
    }

    if (status < 300) {
        if (status === 200) {
            if (contentType.startsWith("text/")) {
                return "ok-text";
            }
            if (contentType.startsWith("application/json")) {
                return "ok-json";
            }
            return "ok-binary";
        }
        if (status === 201) {
            if (body !== null && body.length > 0) {
                return "created-with-body";
            }
            return "created-empty";
        }
        if (status === 204) {
            return "no-content";
        }
        return "success-other";
    }

    if (status < 400) {
        if (status === 301 || status === 302) {
            return "redirect";
        }
        return "redirect-other";
    }

    if (status < 500) {
        if (status === 401 || status === 403) {
            return "auth-error";
        }
        return "client-error";
    }

    return "server-error";
}

// State machine that processes a sequence of events.
// States: idle -> loading -> (success | error) -> done
// With retry logic: error -> loading (up to maxRetries)
//
// EXPECTED BRANCHES (12):
//   1. empty events array                        → "idle"
//   2. idle + "start" event                      → transitions to loading
//   3. idle + non-"start" event                  → "invalid-transition"
//   4. loading + "success" event                 → transitions to success
//   5. loading + "error" event, retries left     → stays in loading (retry)
//   6. loading + "error" event, no retries left  → transitions to error
//   7. loading + other event                     → "invalid-transition"
//   8. success + "reset" event                   → transitions to done
//   9. success + other event                     → "invalid-transition"
//  10. error + "reset" event                     → transitions to done
//  11. error + other event                       → "invalid-transition"
//  12. done state reached                        → "done"
//
// DIFFICULTY: Hard. The solver must generate a specific sequence of string
// events in the right order. Random guessing almost never produces a valid
// multi-step sequence like ["start", "error", "error", "success", "reset"].

type MachineState = "idle" | "loading" | "success" | "error" | "done";

export function processStateMachine(
    events: string[],
    maxRetries: number
): string {
    let state: MachineState = "idle";
    let retries = 0;

    if (events.length === 0) {
        return "idle";
    }

    for (const event of events) {
        switch (state) {
            case "idle":
                if (event === "start") {
                    state = "loading";
                } else {
                    return "invalid-transition";
                }
                break;

            case "loading":
                if (event === "success") {
                    state = "success";
                } else if (event === "error") {
                    retries++;
                    if (retries <= maxRetries) {
                        state = "loading";
                    } else {
                        state = "error";
                    }
                } else {
                    return "invalid-transition";
                }
                break;

            case "success":
                if (event === "reset") {
                    state = "done";
                } else {
                    return "invalid-transition";
                }
                break;

            case "error":
                if (event === "reset") {
                    state = "done";
                } else {
                    return "invalid-transition";
                }
                break;

            case "done":
                return "done";
        }
    }

    return state;
}
