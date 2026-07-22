use crate::{LynoraError, Result};
use std::collections::HashMap;

/// Expand `{{name}}` placeholders using `vars`.
/// Unknown variables return `Err(LynoraError::MissingVariable)`.
pub fn expand(input: &str, vars: &HashMap<String, String>) -> Result<String> {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = input[i + 2..].find("}}") {
                let key = input[i + 2..i + 2 + end].trim();
                let value = vars
                    .get(key)
                    .ok_or_else(|| LynoraError::MissingVariable(key.to_string()))?;
                out.push_str(value);
                i = i + 2 + end + 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LynoraError;

    #[test]
    fn expands_simple_vars() {
        let mut vars = HashMap::new();
        vars.insert("baseUrl".into(), "https://api.example.com".into());
        vars.insert("id".into(), "42".into());
        let out = expand("{{baseUrl}}/users/{{id}}", &vars).unwrap();
        assert_eq!(out, "https://api.example.com/users/42");
    }

    #[test]
    fn missing_var_errors() {
        let vars = HashMap::new();
        let err = expand("{{missing}}", &vars).unwrap_err();
        assert!(matches!(err, LynoraError::MissingVariable(n) if n == "missing"));
    }

    #[test]
    fn leaves_plain_text() {
        let vars = HashMap::new();
        assert_eq!(expand("hello", &vars).unwrap(), "hello");
    }
}
