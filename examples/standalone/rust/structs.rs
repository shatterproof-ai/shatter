// Example 4: Classify HTTP Request
// Tests shatter's ability to handle struct field access with nested conditions.
//
// EXPECTED BRANCHES (7):
//   1. method == "GET"  && path starts with "/api"  && authenticated -> "authorized api read"
//   2. method == "GET"  && path starts with "/api"  && !authenticated -> "unauthorized api read"
//   3. method == "GET"  && path does not start with "/api"            -> "public page"
//   4. method == "POST" && path starts with "/api"  && authenticated -> "authorized api write"
//   5. method == "POST" && path starts with "/api"  && !authenticated -> "unauthorized api write"
//   6. method == "POST" && path does not start with "/api"            -> "form submission"
//   7. other method                                                   -> "method not allowed"
//
// DIFFICULTY: Medium. Requires exploring combinations of struct fields.

pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub authenticated: bool,
}

pub fn classify_request(req: &HttpRequest) -> &'static str {
    match req.method.as_str() {
        "GET" => {
            if req.path.starts_with("/api") {
                if req.authenticated {
                    "authorized api read"
                } else {
                    "unauthorized api read"
                }
            } else {
                "public page"
            }
        }
        "POST" => {
            if req.path.starts_with("/api") {
                if req.authenticated {
                    "authorized api write"
                } else {
                    "unauthorized api write"
                }
            } else {
                "form submission"
            }
        }
        _ => "method not allowed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(method: &str, path: &str, authenticated: bool) -> HttpRequest {
        HttpRequest {
            method: method.to_string(),
            path: path.to_string(),
            authenticated,
        }
    }

    #[test]
    fn test_authorized_api_read() {
        assert_eq!(
            classify_request(&make_request("GET", "/api/users", true)),
            "authorized api read"
        );
    }

    #[test]
    fn test_unauthorized_api_read() {
        assert_eq!(
            classify_request(&make_request("GET", "/api/users", false)),
            "unauthorized api read"
        );
    }

    #[test]
    fn test_public_page() {
        assert_eq!(
            classify_request(&make_request("GET", "/index.html", false)),
            "public page"
        );
    }

    #[test]
    fn test_authorized_api_write() {
        assert_eq!(
            classify_request(&make_request("POST", "/api/users", true)),
            "authorized api write"
        );
    }

    #[test]
    fn test_unauthorized_api_write() {
        assert_eq!(
            classify_request(&make_request("POST", "/api/users", false)),
            "unauthorized api write"
        );
    }

    #[test]
    fn test_form_submission() {
        assert_eq!(
            classify_request(&make_request("POST", "/login", false)),
            "form submission"
        );
    }

    #[test]
    fn test_method_not_allowed() {
        assert_eq!(
            classify_request(&make_request("DELETE", "/api/users", true)),
            "method not allowed"
        );
    }
}
