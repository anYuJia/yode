use std::env;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

static CDP_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub(super) struct BrowserExecution {
    pub message: String,
    pub metadata: Value,
}

struct ChromeRuntime {
    child: Child,
    port: u16,
    executable: PathBuf,
}

fn runtime_state() -> &'static Mutex<Option<ChromeRuntime>> {
    static STATE: OnceLock<Mutex<Option<ChromeRuntime>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

pub async fn shutdown_browser_runtime() -> Result<()> {
    let mut state = runtime_state().lock().await;
    if let Some(mut runtime) = state.take() {
        let _ = runtime.child.kill().await;
        let _ = runtime.child.wait().await;
    }
    Ok(())
}

pub(super) async fn execute_browser_action(
    action: &str,
    params: &Value,
    workspace: &Path,
) -> Result<BrowserExecution> {
    let mut state = runtime_state().lock().await;
    let needs_restart = match state.as_mut() {
        Some(runtime) => !runtime.is_ready().await,
        None => true,
    };
    if needs_restart {
        if let Some(mut stale) = state.take() {
            let _ = stale.child.kill().await;
            let _ = stale.child.wait().await;
        }
        *state = Some(ChromeRuntime::launch().await?);
    }

    state
        .as_mut()
        .ok_or_else(|| anyhow!("browser runtime did not initialize"))?
        .perform(action, params, workspace)
        .await
}

impl ChromeRuntime {
    async fn launch() -> Result<Self> {
        let executable = find_browser_executable().ok_or_else(|| {
            anyhow!(
                "No supported Chrome/Chromium/Edge executable was found. Install a Chromium-based browser or set YODE_BROWSER_EXECUTABLE."
            )
        })?;
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .context("failed to reserve a local browser debugging port")?;
        let port = listener.local_addr()?.port();
        drop(listener);

        let profile_dir = browser_profile_dir();
        tokio::fs::create_dir_all(&profile_dir)
            .await
            .with_context(|| {
                format!("failed to create browser profile {}", profile_dir.display())
            })?;

        let mut command = Command::new(&executable);
        command
            .arg("--headless=new")
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--remote-debugging-port={port}"))
            .arg(format!("--user-data-dir={}", profile_dir.display()))
            .arg("--disable-background-networking")
            .arg("--disable-component-update")
            .arg("--disable-default-apps")
            .arg("--disable-sync")
            .arg("--metrics-recording-only")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--window-size=1440,1000")
            .arg("about:blank")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let child = command
            .spawn()
            .with_context(|| format!("failed to start browser {}", executable.display()))?;
        let mut runtime = Self {
            child,
            port,
            executable,
        };
        runtime.wait_until_ready().await?;
        Ok(runtime)
    }

    async fn is_ready(&mut self) -> bool {
        if !matches!(self.child.try_wait(), Ok(None)) {
            return false;
        }
        browser_http_client()
            .get(format!("http://127.0.0.1:{}/json/version", self.port))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    async fn wait_until_ready(&mut self) -> Result<()> {
        for _ in 0..50 {
            if self.is_ready().await {
                return Ok(());
            }
            if let Some(status) = self.child.try_wait()? {
                bail!(
                    "browser process {} exited before CDP became ready: {}",
                    self.executable.display(),
                    status
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        bail!(
            "browser {} did not expose CDP on port {} within 5 seconds",
            self.executable.display(),
            self.port
        )
    }

    async fn perform(
        &mut self,
        action: &str,
        params: &Value,
        workspace: &Path,
    ) -> Result<BrowserExecution> {
        let (message, mut metadata) = match action {
            "navigate" => {
                let url = required_string(params, "url")?;
                self.navigate(url).await?;
                (
                    format!("Navigated to {url}"),
                    json!({"action":"navigate", "requested_url": url}),
                )
            }
            "click" => {
                let selector = required_string(params, "selector")?;
                let selector_literal = serde_json::to_string(selector)?;
                let value = self
                    .evaluate_value(&format!(
                        "(() => {{ const el = document.querySelector({selector_literal}); if (!el) return {{ok:false,error:'selector not found'}}; el.scrollIntoView({{block:'center',inline:'center'}}); el.click(); return {{ok:true,tag:el.tagName,text:(el.innerText||el.value||'').slice(0,240)}}; }})()"
                    ))
                    .await?;
                ensure_js_ok(&value)?;
                (
                    format!("Clicked element: {selector}"),
                    json!({"action":"click", "selector": selector, "result": value}),
                )
            }
            "type" => {
                let selector = required_string(params, "selector")?;
                let text = required_string(params, "text")?;
                let selector_literal = serde_json::to_string(selector)?;
                let text_literal = serde_json::to_string(text)?;
                let value = self
                    .evaluate_value(&format!(
                        "(() => {{ const el = document.querySelector({selector_literal}); if (!el) return {{ok:false,error:'selector not found'}}; el.focus(); const value = {text_literal}; if ('value' in el) {{ const proto = Object.getPrototypeOf(el); const descriptor = Object.getOwnPropertyDescriptor(proto, 'value'); if (descriptor && descriptor.set) descriptor.set.call(el, value); else el.value = value; }} else {{ el.textContent = value; }} el.dispatchEvent(new Event('input', {{bubbles:true}})); el.dispatchEvent(new Event('change', {{bubbles:true}})); return {{ok:true,length:value.length}}; }})()"
                    ))
                    .await?;
                ensure_js_ok(&value)?;
                (
                    format!("Typed text into element: {selector}"),
                    json!({"action":"type", "selector": selector, "text_length": text.chars().count(), "result": value}),
                )
            }
            "scroll" => {
                let delta_y = params.get("delta_y").and_then(Value::as_i64).unwrap_or(600);
                let value = self
                    .evaluate_value(&format!(
                        "(() => {{ window.scrollBy(0, {delta_y}); return {{x:window.scrollX,y:window.scrollY}}; }})()"
                    ))
                    .await?;
                (
                    format!("Scrolled browser by {delta_y}px"),
                    json!({"action":"scroll", "delta_y": delta_y, "position": value}),
                )
            }
            "screenshot" => {
                let result = self
                    .cdp_call(
                        "Page.captureScreenshot",
                        json!({"format":"png", "fromSurface":true, "captureBeyondViewport":false}),
                    )
                    .await?;
                let encoded = result
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("CDP screenshot response did not include image data"))?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(encoded.as_bytes())
                    .context("failed to decode CDP screenshot")?;
                let dir = workspace
                    .join(".yode")
                    .join("browser-cache")
                    .join("screenshots");
                tokio::fs::create_dir_all(&dir).await?;
                let path = dir.join(format!("browser-{}.png", uuid::Uuid::new_v4()));
                tokio::fs::write(&path, bytes).await?;
                (
                    format!("Captured browser screenshot: {}", path.display()),
                    json!({"action":"screenshot", "screenshot_path": path.display().to_string()}),
                )
            }
            "evaluate" => {
                let code = required_string(params, "code")?;
                let value = self.evaluate_value(code).await?;
                (
                    format!("Evaluated JavaScript: {}", compact_json(&value, 600)),
                    json!({"action":"evaluate", "result": value}),
                )
            }
            other => bail!("unsupported browser action '{other}'"),
        };

        let page = self.page_info().await.unwrap_or(Value::Null);
        if let Some(object) = metadata.as_object_mut() {
            object.insert("executor".to_string(), json!("cdp"));
            object.insert(
                "browser_executable".to_string(),
                json!(self.executable.display().to_string()),
            );
            object.insert("page".to_string(), page);
        }
        Ok(BrowserExecution { message, metadata })
    }

    async fn navigate(&self, url: &str) -> Result<()> {
        let result = self.cdp_call("Page.navigate", json!({"url": url})).await?;
        if let Some(error_text) = result.get("errorText").and_then(Value::as_str) {
            if !error_text.is_empty() {
                bail!("browser navigation failed: {error_text}");
            }
        }
        for _ in 0..40 {
            match self.evaluate_value("document.readyState").await {
                Ok(Value::String(state)) if state == "complete" || state == "interactive" => {
                    return Ok(())
                }
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
        Ok(())
    }

    async fn page_info(&self) -> Result<Value> {
        self.evaluate_value(
            "({url:location.href,title:document.title,readyState:document.readyState})",
        )
        .await
    }

    async fn evaluate_value(&self, expression: &str) -> Result<Value> {
        let result = self
            .cdp_call(
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                    "userGesture": true
                }),
            )
            .await?;
        if let Some(exception) = result.get("exceptionDetails") {
            bail!(
                "JavaScript evaluation failed: {}",
                compact_json(exception, 800)
            );
        }
        Ok(result
            .get("result")
            .and_then(|remote| remote.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    async fn cdp_call(&self, method: &str, params: Value) -> Result<Value> {
        let ws_url = self.page_websocket_url().await?;
        let (mut socket, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .with_context(|| format!("failed to connect to browser CDP target {ws_url}"))?;
        let id = CDP_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let payload = json!({"id": id, "method": method, "params": params}).to_string();
        socket.send(Message::Text(payload.into())).await?;

        let response = tokio::time::timeout(Duration::from_secs(15), async {
            while let Some(message) = socket.next().await {
                let message = message?;
                let Message::Text(text) = message else {
                    continue;
                };
                let value: Value = serde_json::from_str(text.as_str())?;
                if value.get("id").and_then(Value::as_u64) != Some(id) {
                    continue;
                }
                if let Some(error) = value.get("error") {
                    bail!("CDP {method} failed: {}", compact_json(error, 800));
                }
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
            bail!("browser CDP socket closed before response for {method}")
        })
        .await
        .with_context(|| format!("browser CDP call timed out: {method}"))??;
        let _ = socket.close(None).await;
        Ok(response)
    }

    async fn page_websocket_url(&self) -> Result<String> {
        let response = browser_http_client()
            .get(format!("http://127.0.0.1:{}/json/list", self.port))
            .send()
            .await?
            .error_for_status()?;
        let targets: Vec<Value> = response.json().await?;
        targets
            .iter()
            .find(|target| target.get("type").and_then(Value::as_str) == Some("page"))
            .and_then(|target| target.get("webSocketDebuggerUrl"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("browser CDP did not expose a page target"))
    }
}

fn required_string<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("browser action requires non-empty '{key}'"))
}

fn ensure_js_ok(value: &Value) -> Result<()> {
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        bail!(
            "browser page action failed: {}",
            value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown page error")
        );
    }
    Ok(())
}

fn compact_json(value: &Value, max_chars: usize) -> String {
    let rendered = value.to_string();
    if rendered.chars().count() <= max_chars {
        rendered
    } else {
        format!(
            "{}...",
            rendered.chars().take(max_chars).collect::<String>()
        )
    }
}

fn browser_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .build()
        .unwrap_or_default()
}

fn browser_profile_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(env::temp_dir)
        .join(".yode")
        .join("browser-data")
        .join("cdp-profile")
}

fn find_browser_executable() -> Option<PathBuf> {
    if let Some(configured) = env::var_os("YODE_BROWSER_EXECUTABLE").map(PathBuf::from) {
        if configured.is_file() {
            return Some(configured);
        }
    }

    if let Some(found) = platform_browser_candidates()
        .into_iter()
        .find(|candidate| candidate.is_file())
    {
        return Some(found);
    }

    let names: &[&str] = if cfg!(target_os = "windows") {
        &["chrome.exe", "msedge.exe", "chromium.exe"]
    } else {
        &[
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
        ]
    };
    names.iter().find_map(|name| find_on_path(name))
}

#[cfg(target_os = "macos")]
fn platform_browser_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
        PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
    ]
}

#[cfg(target_os = "windows")]
fn platform_browser_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for base in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
        if let Some(base) = env::var_os(base).map(PathBuf::from) {
            candidates.push(base.join("Google/Chrome/Application/chrome.exe"));
            candidates.push(base.join("Microsoft/Edge/Application/msedge.exe"));
            candidates.push(base.join("Chromium/Application/chrome.exe"));
        }
    }
    candidates
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_browser_candidates() -> Vec<PathBuf> {
    Vec::new()
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn javascript_failures_are_not_treated_as_success() {
        assert!(ensure_js_ok(&json!({"ok":false,"error":"missing"})).is_err());
        assert!(ensure_js_ok(&json!({"ok":true})).is_ok());
    }

    #[test]
    fn browser_profile_is_outside_project_workspace() {
        assert!(browser_profile_dir().ends_with(Path::new(".yode/browser-data/cdp-profile")));
    }
}
