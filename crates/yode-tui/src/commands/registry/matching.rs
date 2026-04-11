pub(super) fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        dp[i][0] = i;
    }
    for j in 0..=b.len() {
        dp[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a.len()][b.len()]
}

pub(super) fn is_boundary_match(name: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    name.match_indices(needle).any(|(index, _)| {
        index == 0
            || matches!(
                name.as_bytes()
                    .get(index.saturating_sub(1))
                    .copied()
                    .map(char::from),
                Some('-' | '_' | '/')
            )
    })
}
