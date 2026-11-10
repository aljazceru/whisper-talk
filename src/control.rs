use crate::types::Config;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

const MAX_ERRORS: usize = 100;

/// Shared daemon state consumed by the GUI tray/panel.
#[derive(Clone)]
pub struct Control(pub Arc<Mutex<ControlState>>);

pub struct ControlState {
    pub config: Config,
    /// "starting" | "idle" | "recording" | "processing" | "applying settings" | "startup failed" | "stopped"
    pub status: String,
    /// Bounded log of daemon errors and warnings.
    pub errors: VecDeque<String>,
    /// True while recording/processing is active or a config apply is in flight.
    /// GUI MUST disable Apply while busy.
    pub busy: bool,
}

/// GUI → daemon commands. Never constructed without the `gui` feature.
#[allow(dead_code)]
pub enum Command {
    Apply(Box<Config>),
    Quit,
}

impl Control {
    pub fn new(config: Config) -> Self {
        Self(Arc::new(Mutex::new(ControlState {
            config,
            status: "starting".to_string(),
            errors: VecDeque::new(),
            busy: false,
        })))
    }

    pub fn set_status(&self, status: &str) {
        self.0.lock().status = status.to_string();
    }

    pub fn set_busy(&self, busy: bool) {
        self.0.lock().busy = busy;
    }

    pub fn push_error(&self, error: String) {
        let mut state = self.0.lock();
        if state.errors.len() >= MAX_ERRORS {
            state.errors.pop_front();
        }
        state.errors.push_back(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_bounded() {
        let control = Control::new(Config::default());
        for i in 0..(MAX_ERRORS + 20) {
            control.push_error(format!("e{}", i));
        }
        let state = control.0.lock();
        assert_eq!(state.errors.len(), MAX_ERRORS);
        assert_eq!(state.errors.front().unwrap(), "e20");
        assert_eq!(
            state.errors.back().unwrap(),
            &format!("e{}", MAX_ERRORS + 19)
        );
    }
}
