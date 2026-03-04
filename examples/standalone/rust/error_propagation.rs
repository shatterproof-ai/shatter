// Example 7a: Parse Config
// Tests shatter's ability to explore error propagation with the ? operator.
//
// EXPECTED BRANCHES (4):
//   1. missing '=' delimiter       -> Err("missing delimiter in line: {line}")
//   2. empty key (before '=')      -> Err("empty key")
//   3. empty value (after '=')     -> Err("empty value for key: {key}")
//   4. valid key=value             -> Ok((key, value))
//
// DIFFICULTY: Medium. Requires generating strings with specific structure.

#[derive(Debug, PartialEq)]
pub struct ConfigError {
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConfigError {}

pub fn parse_config_line(line: &str) -> Result<(String, String), ConfigError> {
    let (key, value) = line
        .split_once('=')
        .ok_or_else(|| ConfigError {
            message: format!("missing delimiter in line: {line}"),
        })?;

    let key = key.trim();
    if key.is_empty() {
        return Err(ConfigError {
            message: "empty key".to_string(),
        });
    }

    let value = value.trim();
    if value.is_empty() {
        return Err(ConfigError {
            message: format!("empty value for key: {key}"),
        });
    }

    Ok((key.to_string(), value.to_string()))
}

// Example 7b: Validate and Process
// Tests shatter's ability to explore chained ? operator calls.
//
// EXPECTED BRANCHES (4):
//   1. first line fails to parse      -> Err from first parse_config_line
//   2. second line fails to parse     -> Err from second parse_config_line
//   3. host key missing from first    -> Err("expected 'host' key, got: {key}")
//   4. both lines parse successfully  -> Ok("host={host}, port={port}")
//
// DIFFICULTY: Hard. Requires generating two valid config lines with correct keys.
pub fn validate_and_process(
    host_line: &str,
    port_line: &str,
) -> Result<String, ConfigError> {
    let (key, host) = parse_config_line(host_line)?;
    if key != "host" {
        return Err(ConfigError {
            message: format!("expected 'host' key, got: {key}"),
        });
    }

    let (_, port) = parse_config_line(port_line)?;

    Ok(format!("host={host}, port={port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_line_missing_delimiter() {
        let result = parse_config_line("no-equals-here");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().message,
            "missing delimiter in line: no-equals-here"
        );
    }

    #[test]
    fn test_parse_config_line_empty_key() {
        let result = parse_config_line("=value");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().message, "empty key");
    }

    #[test]
    fn test_parse_config_line_empty_value() {
        let result = parse_config_line("key=");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().message, "empty value for key: key");
    }

    #[test]
    fn test_parse_config_line_valid() {
        let result = parse_config_line("host=localhost");
        assert_eq!(
            result.unwrap(),
            ("host".to_string(), "localhost".to_string())
        );
    }

    #[test]
    fn test_validate_first_line_fails() {
        let result = validate_and_process("bad-line", "port=8080");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_second_line_fails() {
        let result = validate_and_process("host=localhost", "bad-line");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_wrong_key() {
        let result = validate_and_process("name=localhost", "port=8080");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().message,
            "expected 'host' key, got: name"
        );
    }

    #[test]
    fn test_validate_success() {
        let result = validate_and_process("host=localhost", "port=8080");
        assert_eq!(result.unwrap(), "host=localhost, port=8080");
    }
}
