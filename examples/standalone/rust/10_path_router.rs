// Example 10: HTTP path routing and middleware resolution.
// Tests reasoning about string patterns, vector lookups, and multi-condition dispatch.

use std::collections::HashMap;

struct Route {
    method: String,
    pattern: String,
    handler: String,
}

struct MatchResult {
    status: &'static str,
    handler: Option<String>,
    params: HashMap<String, String>,
    message: Option<String>,
}

/// match_route — 12 branches: empty path→error, no leading /→error,
/// empty method→error, no routes→not-found, exact static→matched,
/// param (:id)→matched+params, wildcard (*)→matched+wildcard,
/// method mismatch→method-not-allowed, no match→not-found,
/// first match wins, trailing slash normalized, multiple params extracted.
fn match_route(routes: &[Route], method: &str, path: &str) -> MatchResult {
    if path.is_empty() {
        return MatchResult {
            status: "error",
            handler: None,
            params: HashMap::new(),
            message: Some("empty path".to_string()),
        };
    }
    if !path.starts_with('/') {
        return MatchResult {
            status: "error",
            handler: None,
            params: HashMap::new(),
            message: Some("path must start with /".to_string()),
        };
    }
    if method.is_empty() {
        return MatchResult {
            status: "error",
            handler: None,
            params: HashMap::new(),
            message: Some("empty method".to_string()),
        };
    }
    if routes.is_empty() {
        return MatchResult {
            status: "not-found",
            handler: None,
            params: HashMap::new(),
            message: None,
        };
    }

    let normalized = if path.len() > 1 && path.ends_with('/') {
        &path[..path.len() - 1]
    } else {
        path
    };

    let path_segments = split_segments(normalized);
    let upper_method = method.to_uppercase();
    let mut path_matched_but_method_differs = false;

    for route in routes {
        let pattern_segments = split_segments(&route.pattern);

        if !pattern_segments.is_empty() && pattern_segments.last() == Some(&"*".to_string()) {
            let prefix_segments = &pattern_segments[..pattern_segments.len() - 1];
            if path_segments.len() >= prefix_segments.len() {
                let mut prefix_matches = true;
                let mut params = HashMap::new();

                for (i, ps) in prefix_segments.iter().enumerate() {
                    if ps.starts_with(':') {
                        params.insert(ps[1..].to_string(), path_segments[i].clone());
                    } else if *ps != path_segments[i] {
                        prefix_matches = false;
                        break;
                    }
                }

                if prefix_matches {
                    if route.method.to_uppercase() == upper_method {
                        params.insert(
                            "wildcard".to_string(),
                            path_segments[prefix_segments.len()..].join("/"),
                        );
                        return MatchResult {
                            status: "matched",
                            handler: Some(route.handler.clone()),
                            params,
                            message: None,
                        };
                    }
                    path_matched_but_method_differs = true;
                }
            }
            continue;
        }

        if pattern_segments.len() != path_segments.len() {
            continue;
        }

        let mut matches = true;
        let mut params = HashMap::new();
        for (i, ps) in pattern_segments.iter().enumerate() {
            if ps.starts_with(':') {
                params.insert(ps[1..].to_string(), path_segments[i].clone());
            } else if *ps != path_segments[i] {
                matches = false;
                break;
            }
        }

        if matches {
            if route.method.to_uppercase() == upper_method {
                return MatchResult {
                    status: "matched",
                    handler: Some(route.handler.clone()),
                    params,
                    message: None,
                };
            }
            path_matched_but_method_differs = true;
        }
    }

    if path_matched_but_method_differs {
        return MatchResult {
            status: "method-not-allowed",
            handler: None,
            params: HashMap::new(),
            message: None,
        };
    }

    MatchResult {
        status: "not-found",
        handler: None,
        params: HashMap::new(),
        message: None,
    }
}

fn split_segments(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

struct RouteMetadata {
    requires_auth: bool,
    content_type: String,
    cors_origin: Option<String>,
    allowed_origins: Vec<String>,
}

struct MiddlewareChain {
    status: &'static str,
    middleware: Vec<String>,
    reason: Option<String>,
}

/// resolve_middleware — 8 branches: auth+no header→rejected,
/// auth+invalid format→rejected, auth+valid→adds "auth",
/// json→adds "json-parser", multipart→adds "multipart-parser",
/// cors+allowed→adds "cors", cors+disallowed→rejected, base chain only.
fn resolve_middleware(metadata: &RouteMetadata, auth_header: Option<&str>) -> MiddlewareChain {
    let mut chain = vec!["logging".to_string(), "error-handler".to_string()];

    if metadata.requires_auth {
        match auth_header {
            None | Some("") => {
                return MiddlewareChain {
                    status: "rejected",
                    middleware: vec![],
                    reason: Some("missing auth".to_string()),
                };
            }
            Some(h) => {
                if !h.starts_with("Bearer ") || h.len() <= 7 {
                    return MiddlewareChain {
                        status: "rejected",
                        middleware: vec![],
                        reason: Some("invalid auth format".to_string()),
                    };
                }
                chain.push("auth".to_string());
            }
        }
    }

    if metadata.content_type == "application/json" {
        chain.push("json-parser".to_string());
    } else if metadata.content_type == "multipart/form-data" {
        chain.push("multipart-parser".to_string());
    }

    if let Some(origin) = &metadata.cors_origin {
        if metadata.allowed_origins.iter().any(|o| o == origin) {
            chain.push("cors".to_string());
        } else {
            return MiddlewareChain {
                status: "rejected",
                middleware: vec![],
                reason: Some("origin not allowed".to_string()),
            };
        }
    }

    MiddlewareChain {
        status: "ok",
        middleware: chain,
        reason: None,
    }
}

fn main() {
    let routes = vec![
        Route {
            method: "GET".to_string(),
            pattern: "/users/:id".to_string(),
            handler: "get_user".to_string(),
        },
    ];
    let result = match_route(&routes, "GET", "/users/42");
    println!("status={}, handler={:?}, params={:?}", result.status, result.handler, result.params);
}
