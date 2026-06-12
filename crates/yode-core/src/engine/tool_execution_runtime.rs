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
