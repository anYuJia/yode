use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct RemoteQueueDispatchParams {
    #[serde(default)]
    pub(super) target: Option<String>,
    #[serde(default)]
    pub(super) command: Option<String>,
    #[serde(default)]
    pub(super) transcript_path: Option<String>,
    #[serde(default)]
    pub(super) summary: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RemoteQueueResultParams {
    #[serde(default)]
    pub(super) target: Option<String>,
    pub(super) status: String,
    pub(super) summary: String,
    #[serde(default)]
    pub(super) transcript_path: Option<String>,
    #[serde(default)]
    pub(super) result_id: Option<String>,
    #[serde(default)]
    pub(super) endpoint_id: Option<String>,
    #[serde(default)]
    pub(super) device_kind: Option<String>,
    #[serde(default)]
    pub(super) device_label: Option<String>,
    #[serde(default)]
    pub(super) source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RemoteTransportControlParams {
    pub(super) action: String,
    #[serde(default)]
    pub(super) detail: Option<String>,
}
