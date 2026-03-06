// Example 4: Functions with error handling via Result.
// Tests shatter's ability to discover error paths and distinguish normal vs exceptional returns.

/// safe_divide — 4 branches: denominator==0→error, numerator not finite→error,
/// integer result→Ok, non-integer result→Ok.
fn safe_divide(numerator: f64, denominator: f64) -> Result<f64, String> {
    if denominator == 0.0 {
        return Err("division by zero".to_string());
    }
    if numerator.is_infinite() || numerator.is_nan() {
        return Err("non-finite numerator".to_string());
    }
    Ok(numerator / denominator)
}

struct Stats {
    sum: f64,
    avg: f64,
    max: f64,
    flag: Option<String>,
}

/// compute_stats — 5 branches: empty slice→error, any negative→error,
/// all zeros→{0,0,0}, max>100→stats with "high-max" flag, else→stats.
fn compute_stats(items: &[f64]) -> Result<Stats, String> {
    if items.is_empty() {
        return Err("empty array".to_string());
    }

    let mut sum = 0.0;
    let mut max = items[0];

    for &item in items {
        if item < 0.0 {
            return Err("negative value".to_string());
        }
        sum += item;
        if item > max {
            max = item;
        }
    }

    let avg = sum / items.len() as f64;

    if max == 0.0 {
        return Ok(Stats { sum: 0.0, avg: 0.0, max: 0.0, flag: None });
    }

    if max > 100.0 {
        return Ok(Stats { sum, avg, max, flag: Some("high-max".to_string()) });
    }

    Ok(Stats { sum, avg, max, flag: None })
}

fn main() {
    println!("{:?}", safe_divide(10.0, 3.0));
    println!("{:?}", safe_divide(10.0, 0.0));
    match compute_stats(&[1.0, 2.0, 150.0]) {
        Ok(s) => println!("sum={}, avg={}, max={}, flag={:?}", s.sum, s.avg, s.max, s.flag),
        Err(e) => println!("Error: {e}"),
    }
}
