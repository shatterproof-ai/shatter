// str-ja70: Fixture module that must remain byte-for-byte unchanged after
// shatter explore or scan. Exercises typical patterns (struct, enum, nested
// branches) that the instrumentor targets.

#[derive(Debug, Clone)]
pub struct Item {
    pub name: String,
    pub price: f64,
    pub quantity: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Category {
    Electronics,
    Clothing,
    Food,
}

/// Classify an item by price tier. Multiple branches to exercise instrumentation.
pub fn price_tier(price: f64) -> &'static str {
    if price < 0.0 {
        "invalid"
    } else if price < 10.0 {
        "budget"
    } else if price < 100.0 {
        "mid-range"
    } else {
        "premium"
    }
}

/// Match-based dispatch on category.
pub fn category_label(cat: &Category) -> &'static str {
    match cat {
        Category::Electronics => "Electronics & Tech",
        Category::Clothing => "Apparel & Fashion",
        Category::Food => "Groceries & Food",
    }
}

/// Compute total with a for loop and conditional discount.
pub fn compute_total(items: &[Item], discount_threshold: f64) -> f64 {
    let mut total = 0.0;
    for item in items {
        let subtotal = item.price * item.quantity as f64;
        if subtotal > discount_threshold {
            total += subtotal * 0.9;
        } else {
            total += subtotal;
        }
    }
    total
}
