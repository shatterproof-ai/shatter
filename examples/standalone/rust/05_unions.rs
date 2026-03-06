// Example 5: Enum-based dispatch (Rust equivalent of discriminated unions).
// Tests shatter's ability to enumerate enum variants and nested conditions.

use std::f64::consts::PI;

enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Triangle { base: f64, height: f64 },
}

/// compute_area — 6 branches: circle+radius≤0→error, circle→πr²,
/// rectangle+dim≤0→error, rectangle→w*h, triangle+dim≤0→error, triangle→0.5*b*h.
fn compute_area(shape: &Shape) -> Result<f64, String> {
    match shape {
        Shape::Circle { radius } => {
            if *radius <= 0.0 {
                return Err("non-positive radius".to_string());
            }
            Ok(PI * radius * radius)
        }
        Shape::Rectangle { width, height } => {
            if *width <= 0.0 || *height <= 0.0 {
                return Err("non-positive dimension".to_string());
            }
            Ok(width * height)
        }
        Shape::Triangle { base, height } => {
            if *base <= 0.0 || *height <= 0.0 {
                return Err("non-positive dimension".to_string());
            }
            Ok(0.5 * base * height)
        }
    }
}

struct ApiRequest {
    method: String,
    path: String,
    body: Option<String>,
    authenticated: bool,
}

/// route_request — 8 branches: empty path→error, GET→"read",
/// DELETE+!auth→error, DELETE→"delete", POST+no body→error, POST→"create",
/// PUT+no body→error, PUT→"update".
fn route_request(req: &ApiRequest) -> Result<&'static str, String> {
    if req.path.is_empty() {
        return Err("empty path".to_string());
    }

    match req.method.to_uppercase().as_str() {
        "GET" => Ok("read"),
        "DELETE" => {
            if !req.authenticated {
                return Err("auth required".to_string());
            }
            Ok("delete")
        }
        "POST" => {
            if req.body.is_none() {
                return Err("body required".to_string());
            }
            Ok("create")
        }
        "PUT" => {
            if req.body.is_none() {
                return Err("body required".to_string());
            }
            Ok("update")
        }
        _ => Err("unknown method".to_string()),
    }
}

fn main() {
    println!("{:?}", compute_area(&Shape::Circle { radius: 5.0 }));
    println!("{:?}", compute_area(&Shape::Rectangle { width: 3.0, height: 4.0 }));

    let req = ApiRequest {
        method: "GET".to_string(),
        path: "/api/users".to_string(),
        body: None,
        authenticated: false,
    };
    println!("{:?}", route_request(&req));
}
