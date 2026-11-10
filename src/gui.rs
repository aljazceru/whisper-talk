//! GTK4 control panel with a StatusNotifier (system tray) icon.
//! Polls the shared `Control` state; mic enumeration and the tray service
//! run on their own threads and report back via `std::sync::mpsc`.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk::gio::prelude::*;
use gtk::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::audio::capture::DeviceInfo;
use crate::control::{Command, Control};
use crate::types::{AudioConfig, Config, TranscriptionBackend};

const DEFAULT_WHISPER_MODEL: &str = "medium";
const DEFAULT_PARAKEET_MODEL: &str = "parakeet-v3";

fn default_model(backend: TranscriptionBackend) -> &'static str {
    match backend {
        TranscriptionBackend::Whisper => DEFAULT_WHISPER_MODEL,
        TranscriptionBackend::ParakeetV3 => DEFAULT_PARAKEET_MODEL,
    }
}

const BACKENDS: [(TranscriptionBackend, &str); 2] = [
    (TranscriptionBackend::Whisper, "Whisper"),
    (TranscriptionBackend::ParakeetV3, "Parakeet v3"),
];

/// One entry in the microphone dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MicRow {
    /// Use the system default input device (clears all stored selectors).
    Default,
    /// An enumerated input device, identified by canonical name.
    Device(String),
    /// The configured device is not currently plugged in; kept so an
    /// unchanged Apply does not silently drop the stored selection.
    Unavailable(String),
}

impl MicRow {
    fn label(&self) -> String {
        match self {
            MicRow::Default => "System default".to_string(),
            MicRow::Device(name) => name.clone(),
            MicRow::Unavailable(name) => format!("{} (unavailable)", name),
        }
    }

    fn name(&self) -> Option<&str> {
        match self {
            MicRow::Device(name) | MicRow::Unavailable(name) => Some(name),
            MicRow::Default => None,
        }
    }
}

/// Build dropdown rows. If `configured` names a device that is not currently
/// enumerated it is appended as `Unavailable` so it stays selectable and is
/// preserved verbatim on Apply.
fn mic_rows(configured: Option<&str>, devices: &[DeviceInfo]) -> Vec<MicRow> {
    let mut rows = vec![MicRow::Default];
    rows.extend(devices.iter().map(|d| MicRow::Device(d.name.clone())));
    if let Some(name) = configured {
        if !name.is_empty() && !devices.iter().any(|d| d.name == name) {
            rows.push(MicRow::Unavailable(name.to_string()));
        }
    }
    rows
}

/// Index of the row matching the configured device name (0 = system default).
fn mic_selected_index(rows: &[MicRow], configured: Option<&str>) -> u32 {
    match configured {
        None => 0,
        Some(name) => rows
            .iter()
            .position(|r| r.name() == Some(name))
            .map(|i| i as u32)
            .unwrap_or(0),
    }
}

/// Apply the chosen row to the audio config. Only called on an explicit user
/// choice: `Default`/`Device` persist the canonical name and clear stale
/// numeric/vendor/model selectors; `Unavailable` (or no selection) leaves the
/// stored selectors untouched.
fn apply_mic_choice(choice: Option<&MicRow>, audio: &mut AudioConfig) {
    match choice {
        Some(MicRow::Default) => {
            audio.device_name = None;
            audio.device_id = None;
            audio.device_vendor_id = None;
            audio.device_model_id = None;
        }
        Some(MicRow::Device(name)) => {
            audio.device_name = Some(name.clone());
            audio.device_id = None;
            audio.device_vendor_id = None;
            audio.device_model_id = None;
        }
        _ => {}
    }
}

/// Events crossing into the GTK main thread.
enum UiEvent {
    Show,
    Quit,
    Devices(Result<Vec<DeviceInfo>, String>),
    TrayReady,
    TrayFailed(String),
}

/// StatusNotifier tray icon. Lives on its own thread; only channel messages
/// cross back to the GTK thread.
struct GuiTray {
    events: mpsc::Sender<UiEvent>,
}

impl ksni::Tray for GuiTray {
    fn id(&self) -> String {
        "whisper-talk".into()
    }

    fn title(&self) -> String {
        "Whisper Talk".into()
    }

    fn icon_name(&self) -> String {
        "audio-input-microphone".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.events.send(UiEvent::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let show_tx = self.events.clone();
        let quit_tx = self.events.clone();
        vec![
            StandardItem {
                label: "Show Settings".into(),
                activate: Box::new(move |_| {
                    let _ = show_tx.send(UiEvent::Show);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(move |_| {
                    let _ = quit_tx.send(UiEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }

    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        let _ = self.events.send(UiEvent::TrayFailed(
            "status notifier watcher went offline; tray icon may be unavailable".into(),
        ));
        // Keep the service: ksni re-registers when the watcher comes back.
        true
    }

    fn watcher_online(&self) {
        let _ = self.events.send(UiEvent::TrayReady);
    }
}

/// ksni's tokio-compat spawn needs a tokio runtime context; drive the tray
/// service on a dedicated single-thread runtime so no GTK objects cross
/// threads. A failed spawn reports back so the window can be shown.
fn spawn_tray(events: mpsc::Sender<UiEvent>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = events.send(UiEvent::TrayFailed(format!(
                    "could not start tray runtime: {e}"
                )));
                return;
            }
        };
        rt.block_on(async move {
            match ksni::TrayMethods::spawn(GuiTray {
                events: events.clone(),
            })
            .await
            {
                Ok(_handle) => {
                    let _ = events.send(UiEvent::TrayReady);
                    // Keep the service alive until the process exits.
                    std::future::pending::<()>().await;
                }
                Err(e) => {
                    let _ = events.send(UiEvent::TrayFailed(format!(
                        "system tray registration failed: {e}"
                    )));
                }
            }
        });
    });
}

/// Enumerate input devices off the UI thread (hardware probing can block).
fn spawn_enumerate(audio: AudioConfig, events: mpsc::Sender<UiEvent>) {
    std::thread::spawn(move || {
        let result = crate::audio::AudioCapture::new(&audio)
            .and_then(|capture| capture.enumerate_devices())
            .map_err(|e| e.to_string());
        let _ = events.send(UiEvent::Devices(result));
    });
}

/// Panel state shared between GTK callbacks.
struct PanelState {
    control: Control,
    commands: UnboundedSender<Command>,
    /// Current dropdown rows, parallel to the dropdown's model.
    mic_rows: RefCell<Vec<MicRow>>,
    /// Configured mic name to preserve; updated on Apply.
    preserved_mic: RefCell<Option<String>>,
    /// Ignore programmatic selection changes while repopulating.
    suppress_select: Cell<bool>,
    /// User explicitly changed the mic dropdown since Apply.
    mic_dirty: Cell<bool>,
    /// Config sent via Apply, kept until `control.config` matches it.
    pending_apply: RefCell<Option<Config>>,
    /// ksni registered a working tray; while false the window must not hide.
    tray_available: Cell<bool>,
    last_status: RefCell<String>,
    last_errors: RefCell<VecDeque<String>>,
}

/// Load the on-disk config so Apply preserves fields the panel doesn't edit.
/// Never fall back to the shared snapshot: a stale base would silently
/// overwrite settings changed elsewhere.
fn fresh_config() -> anyhow::Result<Config> {
    let paths = crate::paths::Paths::new()?;
    let manager = crate::config::ConfigManager::new(paths).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(manager.get_config().clone())
}

fn set_mic_dropdown(dropdown: &gtk::DropDown, rows: &[MicRow], selected: u32) {
    let labels: Vec<String> = rows.iter().map(MicRow::label).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let list = gtk::StringList::new(&refs);
    dropdown.set_model(Some(&list));
    dropdown.set_selected(selected);
}

fn refresh_mic_dropdown(dropdown: &gtk::DropDown, panel: &Rc<PanelState>, devices: &[DeviceInfo]) {
    // If the user changed the selection, try to keep the equivalent row.
    let previous = panel
        .mic_rows
        .borrow()
        .get(dropdown.selected() as usize)
        .cloned();

    let preserved = panel.preserved_mic.borrow();
    let selected_name = if panel.mic_dirty.get() {
        previous.as_ref().and_then(MicRow::name)
    } else {
        preserved.as_deref()
    };
    let rows = mic_rows(selected_name, devices);
    let selected = mic_selected_index(&rows, selected_name);

    panel.suppress_select.set(true);
    set_mic_dropdown(dropdown, &rows, selected);
    panel.suppress_select.set(false);
    *panel.mic_rows.borrow_mut() = rows;
}

pub fn run(control: Control, commands: UnboundedSender<Command>, show: bool) -> anyhow::Result<()> {
    gtk::init().map_err(|e| anyhow::anyhow!("GTK initialization failed (no display?): {e}"))?;

    let (events_tx, events_rx) = mpsc::channel::<UiEvent>();
    spawn_tray(events_tx.clone());

    // Kick off initial device enumeration immediately; results are drained
    // by the poll timer once the window exists.
    let initial_audio = control.0.lock().config.audio.clone();
    spawn_enumerate(initial_audio, events_tx.clone());

    // NON_UNIQUE: the daemon's own instance lock owns uniqueness; isolated
    // XDG configs must be able to run side by side for testing.
    let app = gtk::Application::new(
        Some("com.github.aljazceru.whisper_talk"),
        gtk::gio::ApplicationFlags::NON_UNIQUE,
    );
    // The daemon lives in the tray: keep the app running with the window hidden.
    let _hold = app.hold();

    // `connect_activate` is `Fn`; stash the receiver and take it once.
    let events_rx = RefCell::new(Some(events_rx));
    let panel = Rc::new(PanelState {
        control: control.clone(),
        commands: commands.clone(),
        mic_rows: RefCell::new(vec![MicRow::Default]),
        preserved_mic: RefCell::new(control.0.lock().config.audio.device_name.clone()),
        suppress_select: Cell::new(false),
        mic_dirty: Cell::new(false),
        pending_apply: RefCell::new(None),
        tray_available: Cell::new(false),
        last_status: RefCell::new(String::new()),
        last_errors: RefCell::new(VecDeque::new()),
    });

    app.connect_activate(move |app| {
        let panel = panel.clone();
        let Some(events_rx) = events_rx.borrow_mut().take() else {
            // Re-activated (e.g. second instance IPC): just raise the window.
            if let Some(window) = app.active_window() {
                window.present();
            }
            return;
        };

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Whisper Talk")
            .icon_name("audio-input-microphone")
            .default_width(440)
            .build();

        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);
        window.set_child(Some(&root));

        // Tray warning banner (hidden until a tray failure arrives).
        let tray_banner = gtk::Label::new(None);
        tray_banner.set_xalign(0.0);
        tray_banner.set_wrap(true);
        tray_banner.set_visible(false);
        root.append(&tray_banner);

        // --- Settings -------------------------------------------------------
        let grid = gtk::Grid::new();
        grid.set_row_spacing(8);
        grid.set_column_spacing(12);
        root.append(&grid);

        let backend_label = gtk::Label::with_mnemonic("_Backend:");
        backend_label.set_xalign(0.0);
        let backend = gtk::DropDown::from_strings(
            &BACKENDS.iter().map(|(_, l)| *l).collect::<Vec<_>>(),
        );
        backend_label.set_mnemonic_widget(Some(&backend));
        grid.attach(&backend_label, 0, 0, 1, 1);
        grid.attach(&backend, 1, 0, 2, 1);
        backend.set_hexpand(true);

        let model_label = gtk::Label::with_mnemonic("_Model:");
        model_label.set_xalign(0.0);
        let model = gtk::Entry::new();
        model_label.set_mnemonic_widget(Some(&model));
        grid.attach(&model_label, 0, 1, 1, 1);
        grid.attach(&model, 1, 1, 2, 1);
        model.set_hexpand(true);

        let mic_label = gtk::Label::with_mnemonic("M_icrophone:");
        mic_label.set_xalign(0.0);
        let mic = gtk::DropDown::from_strings(&[]);
        mic_label.set_mnemonic_widget(Some(&mic));
        grid.attach(&mic_label, 0, 2, 1, 1);
        grid.attach(&mic, 1, 2, 1, 1);
        mic.set_hexpand(true);

        let refresh = gtk::Button::with_mnemonic("_Refresh");
        refresh.set_tooltip_text(Some("Re-scan input devices"));
        grid.attach(&refresh, 2, 2, 1, 1);

        // Restore current settings.
        {
            let state = panel.control.0.lock();
            let cfg = &state.config;
            let backend_idx = BACKENDS
                .iter()
                .position(|(b, _)| *b == cfg.transcription.backend)
                .unwrap_or(0) as u32;
            backend.set_selected(backend_idx);
            model.set_text(&cfg.transcription.model);
            set_mic_dropdown(&mic, &panel.mic_rows.borrow(), 0);
        }

        // Switching backends resets the model to that backend's default;
        // the user edits the entry afterwards if needed.
        {
            let model = model.clone();
            backend.connect_selected_notify(move |dd| {
                let idx = dd.selected() as usize;
                let backend = BACKENDS.get(idx).map(|(b, _)| *b).unwrap_or_default();
                model.set_text(default_model(backend));
            });
        }
        {
            let panel = panel.clone();
            mic.connect_selected_notify(move |_| {
                if !panel.suppress_select.get() {
                    panel.mic_dirty.set(true);
                }
            });
        }
        {
            let events_tx = events_tx.clone();
            let panel = panel.clone();
            refresh.connect_clicked(move |_| {
                let audio = panel.control.0.lock().config.audio.clone();
                spawn_enumerate(audio, events_tx.clone());
            });
        }

        // --- Status ---------------------------------------------------------
        let status_caption = gtk::Label::new(Some("Status:"));
        status_caption.set_xalign(0.0);
        root.append(&status_caption);
        let status = gtk::Label::new(None);
        status.set_xalign(0.0);
        status.set_selectable(true);
        status.set_wrap(true);
        root.append(&status);

        let errors_caption = gtk::Label::new(Some("Recent errors:"));
        errors_caption.set_xalign(0.0);
        root.append(&errors_caption);
        let errors = gtk::TextView::new();
        errors.set_editable(false);
        errors.set_cursor_visible(false);
        errors.set_wrap_mode(gtk::WrapMode::Word);
        errors.set_top_margin(4);
        errors.set_bottom_margin(4);
        errors.set_left_margin(4);
        errors.set_right_margin(4);
        errors.update_property(&[gtk::accessible::Property::Label("Recent errors")]);
        let errors_scroll = gtk::ScrolledWindow::new();
        errors_scroll.set_min_content_height(90);
        errors_scroll.set_child(Some(&errors));
        root.append(&errors_scroll);

        // --- Actions ----------------------------------------------------------
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::End);
        root.append(&actions);

        let quit = gtk::Button::with_mnemonic("_Quit");
        let apply = gtk::Button::with_mnemonic("_Apply");
        apply.add_css_class("suggested-action");
        actions.append(&apply);
        actions.append(&quit);
        window.set_default_widget(Some(&apply));

        let do_quit = {
            let commands = panel.commands.clone();
            let app = app.clone();
            move || {
                let _ = commands.send(Command::Quit);
                app.quit();
            }
        };
        // With no tray the window is the only UI: closing quits instead of
        // hiding into an unreachable process.
        {
            let panel = panel.clone();
            let do_quit = do_quit.clone();
            window.connect_close_request(move |w| {
                if panel.tray_available.get() {
                    w.set_visible(false);
                } else {
                    do_quit();
                }
                gtk::glib::Propagation::Stop
            });
        }

        {
            let do_quit = do_quit.clone();
            quit.connect_clicked(move |_| do_quit());
        }
        {
            let panel = panel.clone();
            let backend = backend.clone();
            let model = model.clone();
            let mic = mic.clone();
            apply.connect_clicked(move |_| {
                let mut cfg = match fresh_config() {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        panel.control.push_error(format!("apply rejected, config unreadable: {e}"));
                        return;
                    }
                };

                let idx = backend.selected() as usize;
                cfg.transcription.backend =
                    BACKENDS.get(idx).map(|(b, _)| *b).unwrap_or_default();

                // Send as-is: empty stays empty so validation reports it.
                cfg.transcription.model = model.text().trim().to_string();

                if panel.mic_dirty.get() {
                    let choice = panel
                        .mic_rows
                        .borrow()
                        .get(mic.selected() as usize)
                        .cloned();
                    if let Some(MicRow::Unavailable(name)) = &choice {
                        panel.control.push_error(format!("Microphone unavailable: {name}. Refresh or choose another input."));
                        return;
                    }
                    apply_mic_choice(choice.as_ref(), &mut cfg.audio);
                }

                // Kept until control.config reflects it; a rejected Apply must
                // not lose the pending mic choice.
                *panel.pending_apply.borrow_mut() = Some(cfg.clone());
                let _ = panel.commands.send(Command::Apply(Box::new(cfg)));
            });
        }

        // --- Polling: tray/device events + shared status ----------------------
        {
            let panel = panel.clone();
            let window = window.clone();
            let mic = mic.clone();
            let status = status.clone();
            let errors = errors.clone();
            let apply_btn = apply.clone();
            let tray_banner = tray_banner.clone();
            let _source_id = gtk::glib::timeout_add_local(Duration::from_millis(250), move || {
                while let Ok(ev) = events_rx.try_recv() {
                    match ev {
                        UiEvent::Show => window.present(),
                        UiEvent::Quit => do_quit(),
                        UiEvent::Devices(Ok(devices)) => {
                            refresh_mic_dropdown(&mic, &panel, &devices);
                        }
                        UiEvent::Devices(Err(e)) => {
                            panel.control.push_error(format!("microphone enumeration failed: {e}"));
                        }
                        UiEvent::TrayReady => {
                            panel.tray_available.set(true);
                            tray_banner.set_visible(false);
                        }
                        UiEvent::TrayFailed(e) => {
                            panel.tray_available.set(false);
                            tray_banner.set_text(&format!(
                                "{e} — the settings window stays open because no tray icon is available."
                            ));
                            tray_banner.set_visible(true);
                            panel.control.push_error(e);
                            window.present();
                        }
                    }
                }

                let state = panel.control.0.lock();
                if state.status == "stopped" {
                    drop(state);
                    do_quit();
                    return gtk::glib::ControlFlow::Break;
                }
                if *panel.last_status.borrow() != state.status {
                    status.set_text(&state.status);
                    *panel.last_status.borrow_mut() = state.status.clone();
                }
                apply_btn.set_sensitive(!state.busy);
                if *panel.last_errors.borrow() != state.errors {
                    let text = if state.errors.is_empty() {
                        "None".to_string()
                    } else {
                        state.errors.iter().cloned().collect::<Vec<_>>().join("\n")
                    };
                    errors.buffer().set_text(&text);
                    *panel.last_errors.borrow_mut() = state.errors.clone();
                }

                // Acknowledge Apply only once the runtime's config matches
                // what we sent; rejected applies keep the pending choice.
                if panel.pending_apply.borrow().as_ref() == Some(&state.config) {
                    *panel.preserved_mic.borrow_mut() = state.config.audio.device_name.clone();
                    panel.mic_dirty.set(false);
                    *panel.pending_apply.borrow_mut() = None;
                }

                gtk::glib::ControlFlow::Continue
            });
        }

        if show {
            window.present();
        }
    });

    // Empty args: do not let GTK parse the daemon's own CLI flags.
    let _exit_code = app.run_with_args::<&str>(&[]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(name: &str) -> DeviceInfo {
        DeviceInfo {
            name: name.to_string(),
            id: Some(0),
            default_sample_rate: 48_000,
            max_channels: 2,
        }
    }

    #[test]
    fn unavailable_configured_mic_is_listed_and_preserved() {
        let devices = vec![dev("USB Microphone")];
        let rows = mic_rows(Some("Desk Mic"), &devices);
        assert_eq!(
            rows,
            vec![
                MicRow::Default,
                MicRow::Device("USB Microphone".into()),
                MicRow::Unavailable("Desk Mic".into()),
            ]
        );
        assert_eq!(mic_selected_index(&rows, Some("Desk Mic")), 2);

        // Applying the preserved row must not touch the stored selectors.
        let mut audio = AudioConfig {
            device_id: Some(3),
            device_name: Some("Desk Mic".into()),
            device_vendor_id: Some("046d".into()),
            device_model_id: Some("0825".into()),
            ..Default::default()
        };
        let before = audio.clone();
        apply_mic_choice(rows.get(2), &mut audio);
        assert_eq!(audio, before);
    }

    #[test]
    fn explicit_default_clears_stale_selectors() {
        let mut audio = AudioConfig {
            device_id: Some(3),
            device_name: Some("Desk Mic".into()),
            device_vendor_id: Some("046d".into()),
            device_model_id: Some("0825".into()),
            ..Default::default()
        };
        apply_mic_choice(Some(&MicRow::Default), &mut audio);
        assert_eq!(audio.device_name, None);
        assert_eq!(audio.device_id, None);
        assert_eq!(audio.device_vendor_id, None);
        assert_eq!(audio.device_model_id, None);
    }

    #[test]
    fn explicit_device_persists_canonical_name_and_clears_ids() {
        let devices = vec![dev("USB Microphone"), dev("Desk Mic")];
        let rows = mic_rows(Some("Missing Mic"), &devices);
        assert_eq!(mic_selected_index(&rows, Some("Missing Mic")), 3);

        let mut audio = AudioConfig {
            device_id: Some(3),
            device_name: Some("Missing Mic".into()),
            device_vendor_id: Some("046d".into()),
            device_model_id: Some("0825".into()),
            ..Default::default()
        };
        apply_mic_choice(rows.get(2), &mut audio);
        assert_eq!(audio.device_name.as_deref(), Some("Desk Mic"));
        assert_eq!(audio.device_id, None);
        assert_eq!(audio.device_vendor_id, None);
        assert_eq!(audio.device_model_id, None);
    }

    #[test]
    fn known_configured_mic_selects_device_row() {
        let devices = vec![dev("USB Microphone"), dev("Desk Mic")];
        let rows = mic_rows(Some("Desk Mic"), &devices);
        assert_eq!(rows.len(), 3);
        assert_eq!(mic_selected_index(&rows, Some("Desk Mic")), 2);
    }
}
