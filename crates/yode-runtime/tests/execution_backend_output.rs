use std::collections::BTreeMap;

use yode_runtime::{ExecutionBackend, ExecutionRequest, LocalExecutionBackend};

#[tokio::test]
async fn local_backend_captures_stdout_and_stderr() {
    let dir = tempfile::tempdir().expect("temporary workspace");
    let request = ExecutionRequest {
        command: "echo yode-out && echo yode-err 1>&2".to_string(),
        workspace: dir.path().to_path_buf(),
        timeout_secs: 5,
        env: BTreeMap::new(),
    };

    let result = LocalExecutionBackend
        .execute(&request)
        .await
        .expect("local execution succeeds");

    assert!(result.success());
    assert!(result.stdout.contains("yode-out"));
    assert!(result.stderr.contains("yode-err"));
}
