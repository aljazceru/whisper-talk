use crate::error::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;
use futures_util::StreamExt;
use zbus::Connection;

pub struct SuspendMonitor {
    on_suspend: Arc<dyn Fn() + Send + Sync>,
    on_resume: Arc<dyn Fn() + Send + Sync>,
    running: Arc<AtomicBool>,
    join_handle: Option<tokio::task::JoinHandle<()>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl SuspendMonitor {
    pub fn new<F1, F2>(on_suspend: F1, on_resume: F2) -> Result<Self>
    where
        F1: Fn() + Send + Sync + 'static,
        F2: Fn() + Send + Sync + 'static,
    {
        Ok(Self {
            on_suspend: Arc::new(on_suspend),
            on_resume: Arc::new(on_resume),
            running: Arc::new(AtomicBool::new(false)),
            join_handle: None,
            shutdown_tx: None,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        self.running.store(true, Ordering::Relaxed);

        let on_suspend = Arc::clone(&self.on_suspend);
        let on_resume = Arc::clone(&self.on_resume);
        let running = Arc::clone(&self.running);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        self.join_handle = Some(tokio::task::spawn(async move {
            let conn = match Connection::system().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to connect to system bus: {}", e);
                    running.store(false, Ordering::Relaxed);
                    return;
                }
            };

            // Create a proxy for the login1 manager to receive signals
            let proxy = match zbus::fdo::DBusProxy::new(&conn).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Failed to create DBus proxy: {}", e);
                    running.store(false, Ordering::Relaxed);
                    return;
                }
            };

            // Add match rule for PrepareForSleep signal
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface("org.freedesktop.login1.Manager")
                .unwrap()
                .member("PrepareForSleep")
                .unwrap()
                .build();

            if let Err(e) = proxy.add_match_rule(rule).await {
                tracing::error!("Failed to add match rule: {}", e);
                running.store(false, Ordering::Relaxed);
                return;
            }

            // Create message stream from connection
            let mut stream = zbus::MessageStream::from(&conn);

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    msg = stream.next() => {
                        match msg {
                            Some(msg) => {
                                let msg = match msg {
                                    Ok(m) => m,
                                    Err(e) => {
                                        tracing::error!("Error receiving D-Bus message: {}", e);
                                        continue;
                                    }
                                };

                                // Check if this is the PrepareForSleep signal
                                let header = msg.header();
                                if header.interface().map(|i| i.as_str()) == Some("org.freedesktop.login1.Manager")
                                    && header.member().map(|m| m.as_str()) == Some("PrepareForSleep")
                                {
                                    // Try to deserialize the body as a bool
                                    if let Ok(entering_suspend) = msg.body().deserialize::<bool>() {
                                        if entering_suspend {
                                            on_suspend();
                                        } else {
                                            on_resume();
                                        }
                                    }
                                }
                            }
                            None => {
                                tracing::error!("Message stream ended");
                                break;
                            }
                        }
                    }
                }
            }

            running.store(false, Ordering::Relaxed);
        }));

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);

        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        if let Some(join_handle) = self.join_handle.take() {
            join_handle.abort();
        }
    }
}

impl Drop for SuspendMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}
