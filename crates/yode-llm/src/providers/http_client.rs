use reqwest::Client;
use tracing::warn;

pub(crate) fn provider_http_client(provider: &str) -> Client {
    Client::builder()
        .user_agent(format!("Yode/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_else(|err| {
            warn!(
                provider = provider,
                error = %err,
                "Failed to build provider HTTP client; falling back to default client"
            );
            Client::new()
        })
}
