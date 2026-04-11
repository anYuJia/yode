use super::report::ssh_context_label;

#[test]
fn ssh_context_label_detects_remote_env() {
    assert_eq!(ssh_context_label(Some("/dev/ttys001"), None), "ssh");
    assert_eq!(ssh_context_label(None, Some("client server 22")), "ssh");
    assert_eq!(ssh_context_label(None, None), "local");
}
