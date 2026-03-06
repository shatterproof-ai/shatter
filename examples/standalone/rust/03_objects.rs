// Example 3: Struct parameters with field access in conditions.
// Tests shatter's ability to generate structured inputs and reason about field values.

struct UserProfile {
    name: String,
    age: i32,
    is_verified: bool,
    role: String,
}

/// categorize_user — 6 branches: age<0→error, age<13→"child", age<18→"teen",
/// age≥18+verified+admin→"admin", age≥18+verified→"verified-user",
/// else→"unverified-user".
fn categorize_user(user: &UserProfile) -> Result<&'static str, String> {
    if user.age < 0 {
        return Err("invalid age".to_string());
    }
    if user.age < 13 {
        return Ok("child");
    }
    if user.age < 18 {
        return Ok("teen");
    }
    if user.is_verified {
        if user.role == "admin" {
            return Ok("admin");
        }
        return Ok("verified-user");
    }
    Ok("unverified-user")
}

struct Rectangle {
    width: f64,
    height: f64,
}

/// describe_rectangle — 4 branches: non-positive dim→error,
/// width==height→"square", area>10000→"large-rectangle", else→"small-rectangle".
fn describe_rectangle(rect: &Rectangle) -> Result<&'static str, String> {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return Err("non-positive dimension".to_string());
    }
    if (rect.width - rect.height).abs() < f64::EPSILON {
        return Ok("square");
    }
    let area = rect.width * rect.height;
    if area > 10000.0 {
        return Ok("large-rectangle");
    }
    Ok("small-rectangle")
}

fn main() {
    let user = UserProfile {
        name: "Alice".to_string(),
        age: 25,
        is_verified: true,
        role: "admin".to_string(),
    };
    println!("{:?}", categorize_user(&user));

    let rect = Rectangle { width: 5.0, height: 5.0 };
    println!("{:?}", describe_rectangle(&rect));
}
