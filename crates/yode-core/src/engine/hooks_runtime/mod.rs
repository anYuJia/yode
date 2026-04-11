mod session_hooks;
mod tool_hooks;

use super::*;

impl AgentEngine {
    /// Set hook manager.
    pub fn set_hook_manager(&mut self, mgr: HookManager) {
        self.hook_manager = Some(mgr);
    }

    pub(super) fn append_hook_wake_notifications_as_system_message(&mut self) {
        let Some(hook_mgr) = self.hook_manager.as_ref() else {
            return;
        };
        for wake in hook_mgr.drain_wake_notifications() {
            let message = format!(
                "[Hook Wake via {}: {}]\n{}",
                wake.event, wake.hook_command, wake.message
            );
            self.messages.push(Message::system(&message));
            self.persist_message("system", Some(&message), None, None, None);
        }
    }
}
