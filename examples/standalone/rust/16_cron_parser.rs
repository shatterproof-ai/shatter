// Example 16: Cron expression parser.
// Field-level validation, range/step/list parsing, and special string shortcuts.

struct CronField {
    values: Vec<u32>,
}

struct CronSchedule {
    minute: CronField,
    hour: CronField,
    day_of_month: CronField,
    month: CronField,
    weekday: CronField,
}

const FIELD_BOUNDS: [(u32, u32); 5] = [
    (0, 59),  // minute
    (0, 23),  // hour
    (1, 31),  // day of month
    (1, 12),  // month
    (0, 6),   // weekday (0=Sunday)
];

fn make_range(start: u32, end: u32) -> Vec<u32> {
    (start..=end).collect()
}

fn all_values(min: u32, max: u32) -> CronField {
    CronField { values: make_range(min, max) }
}

fn parse_range_expr(expr: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    let parts: Vec<&str> = expr.splitn(2, '-').collect();
    if parts.len() != 2 {
        return Err("non-numeric value".to_string());
    }
    let start: u32 = parts[0].parse().map_err(|_| "non-numeric value".to_string())?;
    let end: u32 = parts[1].parse().map_err(|_| "non-numeric value".to_string())?;
    if start > end {
        return Err("invalid range".to_string());
    }
    if start < min {
        return Err("value below minimum".to_string());
    }
    if end > max {
        return Err("value above maximum".to_string());
    }
    Ok(make_range(start, end))
}

/// parse_field — handles wildcard, step (*/n, a/n), list (a,b,c with ranges),
/// range (a-b), and single numbers with bounds checking.
fn parse_field(field: &str, min: u32, max: u32) -> Result<CronField, String> {
    if field == "*" {
        return Ok(CronField { values: make_range(min, max) });
    }

    if field.contains('/') {
        let parts: Vec<&str> = field.splitn(2, '/').collect();
        let step: u32 = parts[1].parse().map_err(|_| "non-numeric value".to_string())?;
        if step == 0 {
            return Err("step cannot be zero".to_string());
        }
        let start = if parts[0] == "*" {
            min
        } else {
            parts[0].parse().map_err(|_| "non-numeric value".to_string())?
        };
        let mut values = Vec::new();
        let mut i = start;
        while i <= max {
            values.push(i);
            i += step;
        }
        return Ok(CronField { values });
    }

    if field.contains(',') {
        let mut values = Vec::new();
        for part in field.split(',') {
            if part.contains('-') {
                values.extend(parse_range_expr(part, min, max)?);
            } else {
                let val: u32 = part.parse().map_err(|_| "invalid value in list".to_string())?;
                if val < min {
                    return Err("value below minimum".to_string());
                }
                if val > max {
                    return Err("value above maximum".to_string());
                }
                values.push(val);
            }
        }
        return Ok(CronField { values });
    }

    if field.contains('-') {
        return Ok(CronField { values: parse_range_expr(field, min, max)? });
    }

    let val: u32 = field.parse().map_err(|_| "non-numeric value".to_string())?;
    if val < min {
        return Err("value below minimum".to_string());
    }
    if val > max {
        return Err("value above maximum".to_string());
    }
    Ok(CronField { values: vec![val] })
}

/// parse_cron — 22 branches: empty→error, @yearly/@annually→fixed,
/// @monthly→fixed, @weekly→fixed, @daily/@midnight→fixed, @hourly→fixed,
/// unknown special→error, wrong field count→error,
/// then per-field: wildcard, number, below min, above max, range, invalid range,
/// step */n, step a/n, step zero, list, invalid list element, non-numeric,
/// range in list, weekday 7→0.
fn parse_cron(expression: &str) -> Result<CronSchedule, String> {
    if expression.is_empty() {
        return Err("empty expression".to_string());
    }

    if expression.starts_with('@') {
        return match expression.to_lowercase().as_str() {
            "@yearly" | "@annually" => Ok(CronSchedule {
                minute: CronField { values: vec![0] },
                hour: CronField { values: vec![0] },
                day_of_month: CronField { values: vec![1] },
                month: CronField { values: vec![1] },
                weekday: all_values(0, 6),
            }),
            "@monthly" => Ok(CronSchedule {
                minute: CronField { values: vec![0] },
                hour: CronField { values: vec![0] },
                day_of_month: CronField { values: vec![1] },
                month: all_values(1, 12),
                weekday: all_values(0, 6),
            }),
            "@weekly" => Ok(CronSchedule {
                minute: CronField { values: vec![0] },
                hour: CronField { values: vec![0] },
                day_of_month: all_values(1, 31),
                month: all_values(1, 12),
                weekday: CronField { values: vec![0] },
            }),
            "@daily" | "@midnight" => Ok(CronSchedule {
                minute: CronField { values: vec![0] },
                hour: CronField { values: vec![0] },
                day_of_month: all_values(1, 31),
                month: all_values(1, 12),
                weekday: all_values(0, 6),
            }),
            "@hourly" => Ok(CronSchedule {
                minute: CronField { values: vec![0] },
                hour: all_values(0, 23),
                day_of_month: all_values(1, 31),
                month: all_values(1, 12),
                weekday: all_values(0, 6),
            }),
            _ => Err("unknown special".to_string()),
        };
    }

    let fields: Vec<&str> = expression.split_whitespace().collect();
    if fields.len() != 5 {
        return Err("expected 5 fields".to_string());
    }

    let mut schedule = CronSchedule {
        minute: parse_field(fields[0], FIELD_BOUNDS[0].0, FIELD_BOUNDS[0].1)?,
        hour: parse_field(fields[1], FIELD_BOUNDS[1].0, FIELD_BOUNDS[1].1)?,
        day_of_month: parse_field(fields[2], FIELD_BOUNDS[2].0, FIELD_BOUNDS[2].1)?,
        month: parse_field(fields[3], FIELD_BOUNDS[3].0, FIELD_BOUNDS[3].1)?,
        weekday: parse_field(fields[4], FIELD_BOUNDS[4].0, FIELD_BOUNDS[4].1)?,
    };

    // Normalize weekday 7 → 0 (Sunday)
    for v in &mut schedule.weekday.values {
        if *v == 7 {
            *v = 0;
        }
    }

    Ok(schedule)
}

fn main() {
    match parse_cron("*/15 0 1,15 * 1-5") {
        Ok(s) => println!(
            "minute={:?}, hour={:?}, dom={:?}, month={:?}, weekday={:?}",
            s.minute.values, s.hour.values, s.day_of_month.values,
            s.month.values, s.weekday.values
        ),
        Err(e) => println!("Error: {e}"),
    }
}
