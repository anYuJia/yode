pub mod config;
mod constants;
pub mod context;
pub mod context_manager;
pub mod cost_tracker;
pub mod db;
pub mod engine;
pub mod error;
pub mod hooks;
pub mod instructions;
pub mod permission;
pub mod session;
pub mod session_memory;
pub mod setup;
pub mod skills;
pub mod tool_runtime;
pub mod transcript;
pub mod updater;

pub use context::EffortLevel;
pub use permission::PermissionMode;

#[cfg(all(test, windows))]
pub(crate) mod test_support {
    pub(crate) fn powershell_encoded_command(script: &str) -> String {
        let mut utf16_le = Vec::new();
        for unit in script.encode_utf16() {
            utf16_le.extend_from_slice(&unit.to_le_bytes());
        }

        format!(
            "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
            base64_encode(&utf16_le)
        )
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
