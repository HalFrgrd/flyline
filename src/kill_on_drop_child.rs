//! A small wrapper around [`std::process::Child`] that kills and reaps the
//! child when it is dropped.
//!
//! Used both for the AI agent process (see [`crate::app::ContentMode::AgentModeWaiting`])
//! and for custom prompt widget child processes (see
//! [`crate::prompt_manager`]) so that no orphaned processes are left behind
//! when their owning state transitions away.

/// A [`std::process::Child`] that is killed and waited on when dropped, so it
/// does not outlive its owner.
pub struct KillOnDropChild(pub std::process::Child);

impl KillOnDropChild {
    pub fn new(child: std::process::Child) -> Self {
        KillOnDropChild(child)
    }
}

impl std::ops::Deref for KillOnDropChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for KillOnDropChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Debug for KillOnDropChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KillOnDropChild")
            .field("pid", &self.0.id())
            .finish()
    }
}

impl Drop for KillOnDropChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
