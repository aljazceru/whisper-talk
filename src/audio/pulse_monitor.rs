//! PulseAudio/PipeWire event monitor for whisper-talk
//! Uses pactl to detect default source changes and server restarts

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tracing::{debug, error, info, warn};

use crate::error::Result;

/// Callback type for when the default source changes
pub type OnDefaultChangeCallback = Box<dyn Fn(String) + Send + Sync>;

/// Callback type for when the PulseAudio server restarts
pub type OnServerRestartCallback = Box<dyn Fn() + Send + Sync>;

/// Monitor for PulseAudio/PipeWire events
pub struct PulseMonitor {
    running: Arc<AtomicBool>,
    monitor_thread: Option<JoinHandle<()>>,
    pactl_process: Option<Child>,
    on_default_change: Option<Arc<OnDefaultChangeCallback>>,
    on_server_restart: Option<Arc<OnServerRestartCallback>>,
    default_source_name: Arc<parking_lot::Mutex<Option<String>>>,
}

impl PulseMonitor {
    /// Create a new PulseMonitor
    pub fn new() -> Result<Self> {
        Ok(Self {
            running: Arc::new(AtomicBool::new(false)),
            monitor_thread: None,
            pactl_process: None,
            on_default_change: None,
            on_server_restart: None,
            default_source_name: Arc::new(parking_lot::Mutex::new(None)),
        })
    }

    /// Set the callback for default source changes
    pub fn set_on_default_change<F>(&mut self, callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.on_default_change = Some(Arc::new(Box::new(callback)));
    }

    /// Set the callback for server restarts
    pub fn set_on_server_restart<F>(&mut self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_server_restart = Some(Arc::new(Box::new(callback)));
    }

    /// Get the current default source name
    pub fn get_default_source_name(&self) -> Option<String> {
        // Try to get from pactl
        match Command::new("pactl")
            .args(["get-default-source"])
            .output()
        {
            Ok(output) if output.status.success() => {
                let name = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .to_string();
                if !name.is_empty() {
                    Some(name)
                } else {
                    None
                }
            }
            _ => self.default_source_name.lock().clone(),
        }
    }

    /// Check if pactl is available
    fn is_pactl_available() -> bool {
        Command::new("pactl")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Start monitoring PulseAudio events
    pub fn start(&mut self) -> Result<bool> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(true);
        }

        if !Self::is_pactl_available() {
            warn!("pactl not available, pulse monitoring disabled");
            return Ok(false);
        }

        // Get initial default source
        if let Some(source) = self.get_default_source_name() {
            info!("Initial default source: {}", source);
            *self.default_source_name.lock() = Some(source);
        }

        // Start pactl subscribe process
        let mut child = Command::new("pactl")
            .args(["subscribe"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| crate::error::WhisperTalkError::Audio(format!("Failed to start pactl subscribe: {}", e)))?;

        let stdout = child.stdout.take()
            .ok_or_else(|| crate::error::WhisperTalkError::Audio("Failed to get pactl stdout".to_string()))?;

        self.pactl_process = Some(child);
        self.running.store(true, Ordering::SeqCst);

        // Clone what we need for the thread
        let running = self.running.clone();
        let default_source_name = self.default_source_name.clone();
        let on_default_change = self.on_default_change.clone();
        let on_server_restart = self.on_server_restart.clone();

        // Start monitoring thread
        self.monitor_thread = Some(thread::spawn(move || {
            Self::event_loop(
                stdout,
                running,
                default_source_name,
                on_default_change,
                on_server_restart,
            );
        }));

        info!("Started monitoring for PulseAudio/PipeWire events");
        Ok(true)
    }

    fn event_loop(
        stdout: std::process::ChildStdout,
        running: Arc<AtomicBool>,
        default_source_name: Arc<parking_lot::Mutex<Option<String>>>,
        on_default_change: Option<Arc<OnDefaultChangeCallback>>,
        on_server_restart: Option<Arc<OnServerRestartCallback>>,
    ) {
        let reader = BufReader::new(stdout);

        for line_result in reader.lines() {
            if !running.load(Ordering::SeqCst) {
                break;
            }

            match line_result {
                Ok(line) => {
                    debug!("PulseAudio event: {}", line);

                    // Parse pactl subscribe output
                    // Format: "Event 'change' on server #0"
                    // Format: "Event 'change' on source #1"
                    // Format: "Event 'new' on source #2"

                    if line.contains("on server") || line.contains("on source") {
                        // Check if default source changed
                        if let Some(new_source) = Self::get_default_source_static() {
                            let mut current = default_source_name.lock();
                            if current.as_ref() != Some(&new_source) {
                                let old_source = current.clone();
                                *current = Some(new_source.clone());
                                drop(current); // Release lock before callback

                                info!(
                                    "Default source changed: {} → {}",
                                    old_source.as_deref().unwrap_or("none"),
                                    new_source
                                );

                                if let Some(ref callback) = on_default_change {
                                    // Run callback in separate thread to avoid blocking
                                    let callback = callback.clone();
                                    let new_source = new_source.clone();
                                    thread::spawn(move || {
                                        callback(new_source);
                                    });
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if running.load(Ordering::SeqCst) {
                        warn!("Error reading pactl output: {}", e);

                        // This might indicate a server restart
                        if let Some(ref callback) = on_server_restart {
                            info!("PulseAudio server may have disconnected, triggering restart callback");
                            let callback = callback.clone();
                            thread::spawn(move || {
                                callback();
                            });
                        }

                        // Try to reconnect after a delay
                        thread::sleep(std::time::Duration::from_secs(2));
                    }
                    break;
                }
            }
        }

        debug!("PulseAudio event loop exited");
    }

    fn get_default_source_static() -> Option<String> {
        Command::new("pactl")
            .args(["get-default-source"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Stop monitoring PulseAudio events
    pub fn stop(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        info!("Stopping PulseAudio monitor...");
        self.running.store(false, Ordering::SeqCst);

        // Kill the pactl process
        if let Some(mut child) = self.pactl_process.take() {
            if let Err(e) = child.kill() {
                debug!("Error killing pactl process: {}", e);
            }
            let _ = child.wait();
        }

        // Wait for monitor thread to exit
        if let Some(handle) = self.monitor_thread.take() {
            if handle.join().is_err() {
                error!("Error joining pulse monitor thread");
            }
        }

        info!("PulseAudio monitor stopped");
    }
}

impl Drop for PulseMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Default for PulseMonitor {
    fn default() -> Self {
        Self::new().expect("Failed to create PulseMonitor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pulse_monitor_creation() {
        let monitor = PulseMonitor::new();
        assert!(monitor.is_ok());
    }

    #[test]
    fn test_pactl_availability() {
        // This test just checks that the availability check doesn't panic
        let _ = PulseMonitor::is_pactl_available();
    }

    #[test]
    fn test_get_default_source() {
        let monitor = PulseMonitor::new().unwrap();
        // This may return None if PulseAudio isn't running, which is fine
        let _ = monitor.get_default_source_name();
    }
}
