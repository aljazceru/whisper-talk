pub mod notifications;
pub mod waybar;

pub use notifications::{NotificationManager, Urgency, Notifications};
pub use waybar::{WaybarCommand, handle_waybar, write_recording_status, write_audio_level};
