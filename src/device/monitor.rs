use crate::error::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const GRACE_PERIOD: Duration = Duration::from_secs(5);
const DEBOUNCE_PERIOD: Duration = Duration::from_secs(2);

type OnDeviceCallback = dyn Fn(&udev::Device) + Send + Sync;

pub struct DeviceMonitor {
    on_add: Arc<OnDeviceCallback>,
    on_remove: Arc<OnDeviceCallback>,
    running: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,
    start_time: Option<Instant>,
    last_event_time: Arc<std::sync::Mutex<Option<Instant>>>,
}

impl DeviceMonitor {
    pub fn new<F1, F2>(on_add: F1, on_remove: F2) -> Result<Self>
    where
        F1: Fn(&udev::Device) + Send + Sync + 'static,
        F2: Fn(&udev::Device) + Send + Sync + 'static,
    {
        Ok(Self {
            on_add: Arc::new(on_add),
            on_remove: Arc::new(on_remove),
            running: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
            start_time: None,
            last_event_time: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    pub fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.running.store(true, Ordering::SeqCst);
        self.start_time = Some(Instant::now());

        let on_add = Arc::clone(&self.on_add);
        let on_remove = Arc::clone(&self.on_remove);
        let running = Arc::clone(&self.running);
        let last_event_time = Arc::clone(&self.last_event_time);
        let start_time = self.start_time.unwrap();

        let handle = thread::Builder::new()
            .name("whisper-talk-device-monitor".to_string())
            .spawn(move || {
                // Build the monitor using MonitorBuilder
                let socket = match udev::MonitorBuilder::new() {
                    Ok(builder) => {
                        match builder.match_subsystem("sound") {
                            Ok(b) => match b.listen() {
                                Ok(s) => s,
                                Err(e) => {
                                    eprintln!("Failed to listen to udev socket: {}", e);
                                    return;
                                }
                            },
                            Err(e) => {
                                eprintln!("Failed to match sound subsystem: {}", e);
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to create udev monitor builder: {}", e);
                        return;
                    }
                };

                while running.load(Ordering::SeqCst) {
                    // Use iter() to poll for events with a timeout
                    for event in socket.iter() {
                        if !running.load(Ordering::SeqCst) {
                            break;
                        }

                        let elapsed_since_start = start_time.elapsed();

                        if elapsed_since_start < GRACE_PERIOD {
                            continue;
                        }

                        {
                            let mut last = last_event_time.lock().unwrap();
                            if let Some(last_time) = *last {
                                if last_time.elapsed() < DEBOUNCE_PERIOD {
                                    continue;
                                }
                            }
                            *last = Some(Instant::now());
                        }

                        let device = event.device();
                        let action = event.event_type();

                        match action {
                            udev::EventType::Add => on_add(&device),
                            udev::EventType::Remove => on_remove(&device),
                            _ => {}
                        }
                    }

                    // Sleep briefly between polling cycles
                    if running.load(Ordering::SeqCst) {
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            })?;

        self.thread_handle = Some(handle);
        Ok(())
    }

    pub fn stop(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            self.running.store(false, Ordering::SeqCst);
            if let Some(handle) = self.thread_handle.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for DeviceMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn extract_device_properties(device: &udev::Device) -> DeviceProperties {
    let id_model = device
        .property_value("ID_MODEL")
        .and_then(|s| s.to_str().map(|s| s.to_string()));

    let id_model_id = device
        .property_value("ID_MODEL_ID")
        .and_then(|s| s.to_str().map(|s| s.to_string()));

    let id_vendor_id = device
        .property_value("ID_VENDOR_ID")
        .and_then(|s| s.to_str().map(|s| s.to_string()));

    let devname = device
        .property_value("DEVNAME")
        .and_then(|s| s.to_str().map(|s| s.to_string()));

    DeviceProperties {
        id_model,
        id_model_id,
        id_vendor_id,
        devname,
    }
}

#[derive(Debug, Clone)]
pub struct DeviceProperties {
    pub id_model: Option<String>,
    #[allow(dead_code)]
    pub id_model_id: Option<String>,
    #[allow(dead_code)]
    pub id_vendor_id: Option<String>,
    pub devname: Option<String>,
}

impl DeviceProperties {
    pub fn matches(&self, device_name: &str) -> bool {
        if let Some(ref model) = self.id_model {
            if model.contains(device_name) {
                return true;
            }
        }
        if let Some(ref devname) = self.devname {
            if devname.contains(device_name) {
                return true;
            }
        }
        false
    }
}
