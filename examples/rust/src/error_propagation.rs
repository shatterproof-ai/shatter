/// parse_config_line — 4 branches: missing '=' → Err, empty key → Err,
/// empty value → Err, valid key=value → Ok.
pub fn parse_config_line(line: &str) -> Result<(String, String), String> {
    let eq_pos = line.find('=').ok_or_else(|| "missing '='".to_string())?;
    let key = line[..eq_pos].trim().to_string();
    if key.is_empty() {
        return Err("empty key".to_string());
    }
    let value = line[eq_pos + 1..].trim().to_string();
    if value.is_empty() {
        return Err("empty value".to_string());
    }
    Ok((key, value))
}
