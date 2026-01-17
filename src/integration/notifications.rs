use crate::error::Result;
#[allow(unused_imports)]
use crate::error::WhisperTalkError;
use tracing::debug;
#[allow(unused_imports)]
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

#[cfg(feature = "notifications")]
use notify_rust::Uurgency;

#[cfg(feature = "notifications")]
impl From<Urgency> for Uurgency {
    fn from(urgency: Urgency) -> Self {
        match urgency {
            Urgency::Low => Uurgency::Low,
            Urgency::Normal => Uurgency::Normal,
            Urgency::Critical => Uurgency::Critical,
        }
    }
}

#[allow(dead_code)]
pub struct NotificationManager {
    enabled: bool,
}

impl NotificationManager {
    #[allow(dead_code)]
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    #[allow(dead_code)]
    pub fn send(&self, title: &str, body: &str, _urgency: Urgency) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        #[cfg(feature = "notifications")]
        {
            let result = notify_rust::Notification::new()
                .appname("whisper-talk")
                .summary(title)
                .body(body)
                .urgency(urgency.into())
                .show();

            match result {
                Ok(_) => {
                    debug!("Notification sent: {} - {}", title, body);
                    Ok(())
                }
                Err(e) => {
                    warn!("Failed to send notification: {}", e);
                    Err(WhisperTalkError::System(format!("Notification failed: {}", e)))
                }
            }
        }

        #[cfg(not(feature = "notifications"))]
        {
            debug!("Notification disabled, skipping: {} - {}", title, body);
            Ok(())
        }
    }

    #[allow(dead_code)]
    pub fn send_error(&self, message: &str) -> Result<()> {
        self.send("whisper-talk Error", message, Urgency::Critical)
    }

    #[allow(dead_code)]
    pub fn send_recovery_status(&self, success: bool, reason: &str) -> Result<()> {
        if success {
            self.send("whisper-talk Recovery", reason, Urgency::Normal)
        } else {
            self.send("whisper-talk Recovery Failed", reason, Urgency::Critical)
        }
    }

    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new(true)
    }
}

#[allow(dead_code)]
pub type Notifications = NotificationManager;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_manager_enabled() {
        let manager = NotificationManager::new(true);
        assert!(manager.is_enabled());
    }

    #[test]
    fn test_notification_manager_disabled() {
        let manager = NotificationManager::new(false);
        assert!(!manager.is_enabled());
    }

    #[test]
    fn test_disabled_send_returns_ok() {
        let manager = NotificationManager::new(false);
        let result = manager.send("Test", "Body", Urgency::Normal);
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_error_format() {
        let manager = NotificationManager::new(false);
        let result = manager.send_error("Test error message");
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_recovery_success() {
        let manager = NotificationManager::new(false);
        let result = manager.send_recovery_status(true, "Recovery successful");
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_recovery_failure() {
        let manager = NotificationManager::new(false);
        let result = manager.send_recovery_status(false, "Recovery failed");
        assert!(result.is_ok());
    }
}
