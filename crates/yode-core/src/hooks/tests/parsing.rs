use crate::hooks::parsing::parse_structured_hook_output;

#[test]
fn test_parse_structured_hook_output_supports_memory_sections() {
    let output = parse_structured_hook_output(
        "{\"hookSpecificOutput\":{\"memorySections\":{\"goals\":[\"Goal one\"],\"findings\":[\"Finding one\"],\"confidence\":[\"Medium\"]}}}",
    )
    .unwrap();
    let stdout = output.stdout.unwrap();
    assert!(stdout.contains("### Goals"));
    assert!(stdout.contains("- Goal one"));
    assert!(stdout.contains("### Findings"));
    assert!(stdout.contains("- Finding one"));
    assert!(stdout.contains("### Confidence"));
}

#[test]
fn test_parse_structured_hook_output_merges_text_outputs_in_order() {
    let output = parse_structured_hook_output(
        "{\"systemMessage\":\"primary\",\"additional_context\":\"secondary\",\"hookSpecificOutput\":{\"additionalContext\":\"tertiary\",\"memorySections\":{\"goals\":[\"Goal one\"]}}}",
    )
    .unwrap();
    let stdout = output.stdout.unwrap();
    let primary_idx = stdout.find("primary").unwrap();
    let secondary_idx = stdout.find("secondary").unwrap();
    let tertiary_idx = stdout.find("tertiary").unwrap();
    let goals_idx = stdout.find("### Goals").unwrap();
    assert!(primary_idx < secondary_idx);
    assert!(secondary_idx < tertiary_idx);
    assert!(tertiary_idx < goals_idx);
}
