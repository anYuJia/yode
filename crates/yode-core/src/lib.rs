#[cfg(test)]
mod benchmark;
pub mod config;
mod constants;
pub mod context;
pub mod context_collapse;
pub mod context_manager;
pub mod cost_tracker;
pub mod db;
pub mod engine;
pub mod error;
pub mod hooks;
pub mod instructions;
pub mod learning;
pub mod permission;
pub mod plugin_trust;
pub mod plugins;
pub mod run_controller;
pub mod session;
pub mod session_artifact;
pub mod session_lock;
pub mod session_memory;
pub mod setup;
pub mod skills;
pub mod tool_runtime;
pub mod transcript;
pub mod verification;
pub mod workspace_trust;

pub use context::EffortLevel;
pub use permission::PermissionMode;

#[cfg(all(test, windows))]
pub(crate) mod test_support {
    use std::path::Path;

    pub(crate) fn powershell_encoded_command(script: &str) -> String {
        let mut utf16_le = Vec::new();
        for unit in script.encode_utf16() {
            utf16_le.extend_from_slice(&unit.to_le_bytes());
        }

        format!(
            "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -OutputFormat Text -EncodedCommand {}",
            base64_encode(&utf16_le)
        )
    }

    /// Build a cmd.exe command that emits text byte-for-byte without embedding the payload in the
    /// command line. This avoids the special quote parsing performed by `cmd /s /c` on Windows and
    /// also avoids `echo` adding CRLF to structured hook output.
    pub(crate) fn cmd_literal_output_command(
        working_dir: &Path,
        text: &str,
        exit_code: Option<i32>,
    ) -> String {
        let file_name = format!("yode-hook-output-{}.txt", uuid::Uuid::new_v4());
        std::fs::write(working_dir.join(&file_name), text)
            .expect("write Windows hook literal output fixture");

        match exit_code {
            Some(code) => format!("type {file_name} & exit /b {code}"),
            None => format!("type {file_name}"),
        }
    }

    fn base64_encode(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let first = chunk[0];
            let second = chunk.get(1).copied().unwrap_or(0);
            let third = chunk.get(2).copied().unwrap_or(0);

            output.push(TABLE[(first >> 2) as usize] as char);
            output.push(TABLE[(((first & 0b0000_0011) << 4) | (second >> 4)) as usize] as char);

            if chunk.len() > 1 {
                output.push(TABLE[(((second & 0b0000_1111) << 2) | (third >> 6)) as usize] as char);
            } else {
                output.push('=');
            }

            if chunk.len() > 2 {
                output.push(TABLE[(third & 0b0011_1111) as usize] as char);
            } else {
                output.push('=');
            }
        }

        output
    }
}
