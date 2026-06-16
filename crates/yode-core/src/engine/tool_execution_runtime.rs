mod finalize;
mod parallel;
mod protocol;
mod single_call;

use super::*;

use crate::tool_runtime::ToolResultTruncationView;

fn strip_action_narrative_param(params: &mut serde_json::Value) -> Option<String> {
    params
        .as_object_mut()
        .and_then(|object| object.remove("action_narrative"))
        .and_then(|value| value.as_str().map(str::trim).map(str::to_string))
        .filter(|value| !value.is_empty())
}

fn strip_nested_action_narrative_params(params: &mut serde_json::Value) {
    let Some(invocations) = params
        .get_mut("invocations")
        .and_then(|value| value.as_array_mut())
    else {
        return;
    };

    for invocation in invocations {
        if let Some(nested_params) = invocation.get_mut("params") {
            strip_action_narrative_param(nested_params);
        }
    }
}

fn normalize_nested_tool_parameter_aliases(params: &mut serde_json::Value) {
    let Some(invocations) = params
        .get_mut("invocations")
        .and_then(|value| value.as_array_mut())
    else {
        return;
    };

    for invocation in invocations {
        let Some(tool_name) = invocation
            .get("tool_name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        if let Some(nested_params) = invocation.get_mut("params") {
            normalize_tool_parameter_aliases(&tool_name, nested_params);
        }
    }
}

fn normalize_tool_parameter_aliases(tool_name: &str, params: &mut serde_json::Value) {
    let Some(object) = params.as_object_mut() else {
        return;
    };

    match tool_name {
        "read_file" | "write_file" | "edit_file" | "multi_edit" | "notebook_edit" | "snip" => {
            copy_first_string_alias(
                object,
                "file_path",
                &[
                    "path",
                    "file",
                    "filename",
                    "fileName",
                    "filepath",
                    "filePath",
                    "target",
                    "target_file",
                    "targetFile",
                    "target_path",
                    "targetPath",
                    "TargetFile",
                    "AbsolutePath",
                    "Path",
                ],
            );
        }
        _ => {}
    }

    if tool_name == "write_file" {
        copy_first_string_alias(
            object,
            "content",
            &[
                "text",
                "data",
                "body",
                "contents",
                "file_content",
                "fileContent",
                "FileContent",
                "CodeContent",
            ],
        );
    }
}

fn copy_first_string_alias(
    object: &mut serde_json::Map<String, serde_json::Value>,
    canonical: &str,
    aliases: &[&str],
) {
    if object
        .get(canonical)
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
    {
        return;
    }

    let Some(value) = aliases.iter().find_map(|alias| {
        object
            .get(*alias)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }) else {
        return;
    };

    object.insert(canonical.to_string(), serde_json::Value::String(value));
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{normalize_nested_tool_parameter_aliases, normalize_tool_parameter_aliases};

    #[test]
    fn normalizes_file_tool_path_aliases() {
        let mut params = json!({
            "path": "/tmp/demo.txt",
            "text": "hello"
        });

        normalize_tool_parameter_aliases("write_file", &mut params);

        assert_eq!(params["file_path"], json!("/tmp/demo.txt"));
        assert_eq!(params["content"], json!("hello"));
    }

    #[test]
    fn normalizes_nested_batch_tool_aliases() {
        let mut params = json!({
            "invocations": [
                {
                    "tool_name": "read_file",
                    "params": { "filename": "/tmp/a.rs" }
                },
                {
                    "tool_name": "write_file",
                    "params": { "target_file": "/tmp/b.rs", "file_content": "fn main() {}" }
                }
            ]
        });

        normalize_nested_tool_parameter_aliases(&mut params);

        assert_eq!(
            params["invocations"][0]["params"]["file_path"],
            json!("/tmp/a.rs")
        );
        assert_eq!(
            params["invocations"][1]["params"]["file_path"],
            json!("/tmp/b.rs")
        );
        assert_eq!(
            params["invocations"][1]["params"]["content"],
            json!("fn main() {}")
        );
    }
}
