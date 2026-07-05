use super::*;
use crate::updater::config_state::parse_updater_config;

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

#[test]
fn parse_expected_sha256_matches_release_asset_name() {
    let body = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  yode-aarch64-apple-darwin.tar.gz\n";
    assert_eq!(
        parse_expected_sha256(body, "yode-aarch64-apple-darwin.tar.gz").as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
}

#[test]
fn parse_expected_sha256_ignores_missing_asset() {
    let body = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  other.tar.gz\n";
    assert!(parse_expected_sha256(body, "yode.tar.gz").is_none());
}

#[tokio::test]
async fn sha256_file_hashes_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("payload");
    tokio::fs::write(&path, b"abc").await.unwrap();
    assert_eq!(
        sha256_file(&path).await.unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn invalid_updater_config_falls_back_to_defaults() {
    let config = parse_updater_config("last_checked = nope", &PathBuf::from("updater.toml"));

    assert!(config.last_checked.is_none());
    assert!(config.downloaded_version.is_none());
}
