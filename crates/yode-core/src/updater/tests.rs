use super::*;

#[test]
fn test_version_compare() {
    assert!(versioning::is_version_newer("0.1.0", "0.2.0"));
    assert!(versioning::is_version_newer("0.1.9", "0.2.0"));
    assert!(versioning::is_version_newer("0.9.0", "1.0.0"));
    assert!(!versioning::is_version_newer("0.2.0", "0.1.0"));
    assert!(!versioning::is_version_newer("1.0.0", "0.9.0"));
    assert!(!versioning::is_version_newer("0.2.0", "0.2.0"));
    assert!(versioning::is_version_newer("0.2.0", "0.2.1"));
}

#[test]
fn test_parse_version() {
    let v: Vec<u32> = vec![];
    assert_eq!(versioning::parse_version("0.2.0"), vec![0, 2, 0]);
    assert_eq!(versioning::parse_version("1.0.0"), vec![1, 0, 0]);
    assert_eq!(versioning::parse_version("invalid"), v);
}

#[test]
fn test_release_version_matches_tag() {
    assert!(versioning::release_version_matches_tag("v0.2.1", "0.2.1"));
    assert!(versioning::release_version_matches_tag("0.2.1", "0.2.1"));
    assert!(!versioning::release_version_matches_tag("v0.2.0", "0.2.1"));
}
