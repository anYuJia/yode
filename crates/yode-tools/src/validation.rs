use serde_json::Value;

/// Validate and coerce tool parameters against a JSON Schema.
///
/// Checks required fields, validates types, coerces compatible types (e.g. "123" → 123),
/// and injects default values from the schema.
pub fn validate_and_coerce(schema: &Value, params: &mut Value) -> Result<(), String> {
    let schema_obj = schema.as_object().ok_or("Schema must be an object")?;

    // Ensure params is an object
    if !params.is_object() {
        *params = Value::Object(serde_json::Map::new());
    }

    let params_obj = params.as_object_mut().unwrap();

    // Get properties and required fields from schema
    let properties = schema_obj.get("properties").and_then(|v| v.as_object());
    let required: Vec<&str> = schema_obj
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    // Check required fields
    for field in &required {
        if !params_obj.contains_key(*field) {
            // Check if there's a default in the schema
            if let Some(props) = properties {
                if let Some(prop_schema) = props.get(*field) {
                    if let Some(default_val) = prop_schema.get("default") {
                        params_obj.insert(field.to_string(), default_val.clone());
                        continue;
                    }
                }
            }
            return Err(format!("Missing required parameter: {}", field));
        }
    }

    // Validate and coerce types for each provided parameter
    if let Some(props) = properties {
        for (key, prop_schema) in props {
            if let Some(value) = params_obj.get(key).cloned() {
                let expected_type = prop_schema.get("type").and_then(|v| v.as_str());

                if let Some(expected) = expected_type {
                    match coerce_type(&value, expected) {
                        Ok(coerced) => {
                            params_obj.insert(key.clone(), coerced);
                        }
                        Err(e) => {
                            return Err(format!("Parameter '{}': {}", key, e));
                        }
                    }
                }
            } else {
                // Parameter not provided — inject default if available
                if let Some(default_val) = prop_schema.get("default") {
                    params_obj.insert(key.clone(), default_val.clone());
                }
            }
        }
    }

    Ok(())
}

/// Attempt to coerce a value to the expected JSON Schema type.
fn coerce_type(value: &Value, expected: &str) -> Result<Value, String> {
    match expected {
        "string" => match value {
            Value::String(_) => Ok(value.clone()),
            Value::Number(n) => Ok(Value::String(n.to_string())),
            Value::Bool(b) => Ok(Value::String(b.to_string())),
            Value::Null => Ok(Value::String(String::new())),
            _ => Err(format!("expected string, got {}", type_name(value))),
        },
        "integer" => match value {
            Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    Ok(value.clone())
                } else if let Some(f) = n.as_f64() {
                    // Coerce float to int if it's a whole number
                    if f.fract() == 0.0 {
                        Ok(Value::Number(serde_json::Number::from(f as i64)))
                    } else {
                        Err(format!("expected integer, got float {}", f))
                    }
                } else {
                    Ok(value.clone())
                }
            }
            Value::String(s) => {
                // Coerce "123" → 123
                s.parse::<i64>()
                    .map(|n| Value::Number(serde_json::Number::from(n)))
                    .map_err(|_| format!("expected integer, got string \"{}\"", s))
            }
            _ => Err(format!("expected integer, got {}", type_name(value))),
        },
        "number" => match value {
            Value::Number(_) => Ok(value.clone()),
            Value::String(s) => {
                // Coerce "1.5" → 1.5
                s.parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .ok_or_else(|| format!("expected number, got string \"{}\"", s))
            }
            _ => Err(format!("expected number, got {}", type_name(value))),
        },
        "boolean" => match value {
            Value::Bool(_) => Ok(value.clone()),
            Value::String(s) => match s.as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                _ => Err(format!("expected boolean, got string \"{}\"", s)),
            },
            _ => Err(format!("expected boolean, got {}", type_name(value))),
        },
        "array" => {
            if value.is_array() {
                Ok(value.clone())
            } else {
                Err(format!("expected array, got {}", type_name(value)))
            }
        }
        "object" => {
            if value.is_object() {
                Ok(value.clone())
            } else {
                Err(format!("expected object, got {}", type_name(value)))
            }
        }
        _ => Ok(value.clone()), // Unknown type, pass through
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_required_field_missing() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let mut params = json!({});
        assert!(validate_and_coerce(&schema, &mut params).is_err());
    }

    #[test]
    fn test_required_field_with_default() {
        let schema = json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer", "default": 10 }
            },
            "required": ["count"]
        });
        let mut params = json!({});
        assert!(validate_and_coerce(&schema, &mut params).is_ok());
        assert_eq!(params["count"], 10);
    }

    #[test]
    fn test_string_to_integer_coercion() {
        let schema = json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer" }
            }
        });
        let mut params = json!({ "limit": "123" });
        assert!(validate_and_coerce(&schema, &mut params).is_ok());
        assert_eq!(params["limit"], 123);
    }

    #[test]
    fn test_default_injection() {
        let schema = json!({
            "type": "object",
            "properties": {
                "depth": { "type": "integer", "default": 2 },
                "verbose": { "type": "boolean", "default": false }
            }
        });
        let mut params = json!({});
        assert!(validate_and_coerce(&schema, &mut params).is_ok());
        assert_eq!(params["depth"], 2);
        assert_eq!(params["verbose"], false);
    }

    #[test]
    fn test_type_mismatch() {
        let schema = json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            }
        });
        let mut params = json!({ "count": "abc" });
        assert!(validate_and_coerce(&schema, &mut params).is_err());
    }

    #[test]
    fn test_valid_params_pass_through() {
        let schema = json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "lines": { "type": "integer" }
            },
            "required": ["path"]
        });
        let mut params = json!({ "path": "/tmp/test", "lines": 50 });
        assert!(validate_and_coerce(&schema, &mut params).is_ok());
        assert_eq!(params["path"], "/tmp/test");
        assert_eq!(params["lines"], 50);
    }
}
