pub(super) fn parse_test_counts(output: &str, framework: &str) -> (u32, u32) {
    match framework {
        "cargo" => parse_cargo_counts(output),
        "jest" | "vitest" => parse_node_counts(output),
        "pytest" => parse_pytest_counts(output),
        "go" => parse_go_counts(output),
        _ => (0, 0),
    }
}

fn parse_cargo_counts(output: &str) -> (u32, u32) {
    let passed = output
        .lines()
        .filter(|line| line.contains(" ... ok"))
        .count() as u32;
    let failed = output
        .lines()
        .filter(|line| line.contains(" ... FAILED"))
        .count() as u32;
    (passed, failed)
}

fn parse_node_counts(output: &str) -> (u32, u32) {
    let mut passed = 0;
    let mut failed = 0;
    for line in output.lines() {
        if line.contains("Tests:") || line.contains("Test Files") {
            for segment in line.split(',') {
                let segment = segment.trim();
                if segment.contains("passed") {
                    passed = segment
                        .split_whitespace()
                        .find_map(|part| part.parse::<u32>().ok())
                        .unwrap_or(passed);
                } else if segment.contains("failed") {
                    failed = segment
                        .split_whitespace()
                        .find_map(|part| part.parse::<u32>().ok())
                        .unwrap_or(failed);
                }
            }
        }
    }
    (passed, failed)
}

fn parse_pytest_counts(output: &str) -> (u32, u32) {
    for line in output.lines() {
        if line.contains("passed") || line.contains("failed") {
            let mut passed = 0;
            let mut failed = 0;
            for segment in line.split(',') {
                let segment = segment.trim();
                if segment.contains("passed") {
                    passed = segment
                        .split_whitespace()
                        .find_map(|part| part.parse::<u32>().ok())
                        .unwrap_or(0);
                } else if segment.contains("failed") {
                    failed = segment
                        .split_whitespace()
                        .find_map(|part| part.parse::<u32>().ok())
                        .unwrap_or(0);
                }
            }
            return (passed, failed);
        }
    }
    (0, 0)
}

fn parse_go_counts(output: &str) -> (u32, u32) {
    let passed = output.lines().filter(|line| line.starts_with("--- PASS:")).count() as u32;
    let failed = output.lines().filter(|line| line.starts_with("--- FAIL:")).count() as u32;
    (passed, failed)
}

#[cfg(test)]
mod tests {
    use super::parse_test_counts;

    #[test]
    fn parses_framework_outputs() {
        assert_eq!(parse_test_counts("test a ... ok\ntest b ... FAILED", "cargo"), (1, 1));
        assert_eq!(parse_test_counts("Tests: 3 passed, 1 failed", "jest"), (3, 1));
        assert_eq!(parse_test_counts("=== 2 passed, 1 failed in 0.12s ===", "pytest"), (2, 1));
        assert_eq!(parse_test_counts("--- PASS: A\n--- FAIL: B", "go"), (1, 1));
    }
}
