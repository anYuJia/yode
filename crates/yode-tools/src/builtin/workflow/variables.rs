use serde_json::{Map, Value};

pub(super) fn workflow_variables_from_params(params: &Value) -> Map<String, Value> {
    params
        .get("variables")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default()
}

pub(super) fn apply_variables(value: Value, variables: &Map<String, Value>) -> Value {
    match value {
        Value::String(text) => Value::String(replace_variables(&text, variables)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| apply_variables(item, variables))
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| (key, apply_variables(value, variables)))
                .collect(),
        ),
        other => other,
    }
}

fn replace_variables(input: &str, variables: &Map<String, Value>) -> String {
    let mut output = input.to_string();
    for (key, value) in variables {
        let placeholder = format!("${{{}}}", key);
        let replacement = value
            .as_str()
            .map(|value| value.to_string())
            .unwrap_or_else(|| value.to_string());
        output = output.replace(&placeholder, &replacement);
    }
    output
}
