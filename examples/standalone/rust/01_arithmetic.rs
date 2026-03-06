// Example 1: Pure arithmetic with branches.
// Tests shatter's ability to explore numeric conditions.

/// classify_number — 4 branches: n<0→"negative", n==0→"zero",
/// n>0 even→"positive-even", n>0 odd→"positive-odd".
fn classify_number(n: i64) -> &'static str {
    if n < 0 {
        return "negative";
    }
    if n == 0 {
        return "zero";
    }
    if n % 2 == 0 {
        "positive-even"
    } else {
        "positive-odd"
    }
}

/// compare_magnitudes — 4 branches: sum>100 AND product>1000→"both-large",
/// sum>100→"sum-large", product>1000→"product-large", else→"both-small".
fn compare_magnitudes(a: i64, b: i64) -> &'static str {
    let sum = a + b;
    let product = a * b;

    if sum > 100 {
        if product > 1000 {
            return "both-large";
        }
        return "sum-large";
    }
    if product > 1000 {
        return "product-large";
    }
    "both-small"
}

fn main() {
    println!("{}", classify_number(-5));
    println!("{}", classify_number(0));
    println!("{}", classify_number(4));
    println!("{}", classify_number(7));
    println!("{}", compare_magnitudes(60, 50));
    println!("{}", compare_magnitudes(3, 4));
}
