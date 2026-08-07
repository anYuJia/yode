//! 共享 HTTP 客户端配置。
//!
//! 核心安全约束：回环地址（127.0.0.1 / localhost / ::1）绝不经过用户或系统代理，
//! 既避免本地流量被代理观察/劫持，也保证本地测试服务与本地资源始终直连。
//! 其余请求按环境变量（NO_PROXY > HTTP(S)_PROXY > ALL_PROXY）路由，与常见代理
//! 约定保持一致。

pub(crate) fn loopback_aware_proxy() -> reqwest::Proxy {
    reqwest::Proxy::custom(|url: &reqwest::Url| -> Option<String> {
        let host = url.host_str().unwrap_or("");
        let is_loopback = matches!(host, "127.0.0.1" | "localhost" | "::1")
            || host.starts_with("127.")
            || host.starts_with("[::1]");
        if is_loopback {
            return None;
        }
        if no_proxy_env_matches(host) {
            return None;
        }
        let scheme = url.scheme();
        let candidates: &[&str] = if scheme == "https" {
            &["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]
        } else {
            &["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"]
        };
        for key in candidates {
            if let Ok(value) = std::env::var(key) {
                if !value.trim().is_empty() && reqwest::Url::parse(value.trim()).is_ok() {
                    return Some(value);
                }
            }
        }
        None
    })
}

fn no_proxy_env_matches(host: &str) -> bool {
    let no_proxy = std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .unwrap_or_default();
    if no_proxy.trim() == "*" {
        return true;
    }
    no_proxy.split(',').any(|entry| {
        let entry = entry.trim().trim_start_matches('.');
        !entry.is_empty() && host == entry
    })
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::no_proxy_env_matches;
    use crate::builtin::http_client::loopback_aware_proxy;

    /// 环境变量是进程级全局状态，所有会改写代理变量的测试必须串行。
    async fn proxy_env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    #[tokio::test]
    async fn no_proxy_star_matches_everything() {
        let _guard = proxy_env_lock().await;
        std::env::set_var("NO_PROXY", "*");
        assert!(no_proxy_env_matches("example.com"));
        std::env::remove_var("NO_PROXY");
    }

    #[tokio::test]
    async fn no_proxy_list_matches_exact_hosts() {
        let _guard = proxy_env_lock().await;
        std::env::set_var("NO_PROXY", "example.com, internal.local");
        assert!(no_proxy_env_matches("example.com"));
        assert!(no_proxy_env_matches("internal.local"));
        assert!(!no_proxy_env_matches("other.example.com"));
        std::env::remove_var("NO_PROXY");
    }

    async fn spawn_echo_server() -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let _ = socket.read(&mut buf).await;
            let _ = tx.send(String::from_utf8_lossy(&buf[..]).to_string());
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
            let _ = socket.write_all(response.as_bytes()).await;
        });
        (format!("http://{addr}"), rx)
    }

    #[tokio::test]
    async fn loopback_fetch_bypasses_proxy_environment() {
        let _guard = proxy_env_lock().await;
        let (url, _rx) = spawn_echo_server().await;
        // 一个必定不可达的代理：如果回环请求被路由到代理，会等到客户端超时并失败。
        std::env::set_var("HTTP_PROXY", "http://127.0.0.1:1");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .proxy(loopback_aware_proxy())
            .build()
            .unwrap();
        let response = client.get(&url).send().await;
        assert!(response.is_ok(), "loopback request must not use the proxy");

        std::env::remove_var("HTTP_PROXY");
    }

    #[tokio::test]
    async fn loopback_fetch_uses_redirected_proxy_for_external_hosts() {
        let _guard = proxy_env_lock().await;
        // 非回环地址应使用环境代理；用本地代理确认请求确实经过代理。
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let _ = socket.read(&mut buf).await;
            let response =
                "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = socket.write_all(response.as_bytes()).await;
        });
        let proxy_url = format!("http://{proxy_addr}");
        std::env::set_var("HTTP_PROXY", &proxy_url);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .proxy(loopback_aware_proxy())
            .build()
            .unwrap();
        let response = client
            .get("http://external.invalid/x")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 502);

        std::env::remove_var("HTTP_PROXY");
    }
}
