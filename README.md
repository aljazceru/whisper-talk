# whisper-talk

System-wide voice dictation for Linux with Whisper backend.

## Overview

**whisper-talk** is a lightweight, efficient voice dictation daemon for Linux that provides system-wide speech-to-text functionality. Press a global hotkey, speak, have your text injected into clipboard so you can paste it into any application.

### Key Features

- **Privacy-focused** - Local transcription only, no cloud services required
- **Recording modes** - Toggle, Push-to-Talk, and Auto (hybrid) modes
- **System integration** - systemd service, desktop notifications
- **Low resource usage** - Written in Rust for efficiency

### How It Works

1. Press your configured global shortcut (default: `SUPER+ALT+D`)
2. gwhspr starts recording from your microphone
3. Speak your text
4. Press the shortcut again
5. Audio is transcribed locally using Whisper 
6. Transcribed text is injected into clipboard

## Quick Start

### Prerequisites

- Linux system (Fedora, Ubuntu, Arch, or derivative)
- Rust 1.70+ for building from source
- Microphone input device
- ydotool for clipboard injection

### Installation

```bash
# Clone the repository
git clone https://github.com/aljazceru/whisper-talk.git
cd whisper-talk

# Install system dependencies (Fedora)
sudo dnf install alsa-lib-devel pulseaudio-libs-devel ydotool rocm-devel  

# Or for Ubuntu/Debian
sudo apt install libasound2-dev libpulse-dev ydotool librocm-dev

# Build and install
cargo build --release
cargo install --path .

# Run setup wizard
whisper-talk setup
```

### Initial Setup

The interactive setup wizard guides you through:

1. Selecting a transcription backend (Whisper recommended)
2. Choosing a model size (tiny, base, small, medium, large-v3)
3. Selecting your audio input device
4. Setting up user permissions
5. Optionally installing systemd service

```bash
whisper-talk setup
```

### Basic Usage

```bash
# Start the daemon
whisper-talk daemon

# Or run in background with systemd
systemctl --user start whisper-talk

# Press SUPER+ALT+D to start/stop recording
```

## Configuration

Configuration is stored in `~/.config/whisper-talk/config.json`:

```json
{
  "shortcuts": {
    "primary_shortcut": "SUPER+ALT+d",
    "recording_mode": "toggle",
    "grab_keys": false,
    "auto_mode_threshold_ms": 400
  },
  "audio": {
    "device_id": null,
    "device_name": null,
    "mute_detection": true,
    "zero_volume_threshold": 5e-7
  },
  "transcription": {
    "backend": "whisper",
    "model": "base",
    "threads": 4,
    "language": null,
    "word_overrides": {},
    "whisper_prompt": "Transcribe with proper capitalization, including sentence beginnings, proper nouns, titles, and standard English capitalization rules."
  },
  "injection": {
    "paste_mode": "ctrl_shift",
    "auto_submit": false,
    "clipboard_behavior": false,
    "clipboard_clear_delay": 5.0
  },
  "feedback": {
    "mic_osd_enabled": true,
    "audio_feedback": true,
    "master_volume": 1.0,
    "start_sound_volume": 1.0,
    "stop_sound_volume": 1.0,
    "error_sound_volume": 1.0
  }
}
```

## System Integration

### systemd Service

Enable whisper-talk to start automatically on login:

```bash
# Generate and install service file
whisper-talk systemd install

# Enable and start
systemctl --user enable whisper-talk
systemctl --user start whisper-talk

# Check status
systemctl --user status whisper-talk
```

## Troubleshooting

### No audio input detected

```bash
# Check if microphone is detected by system
pactl list sources short

# Or with ALSA
arecord -l

# Verify audio permissions
groups $USER  # Should include 'audio'
```

### Text not being typed

```bash
# Test ydotool
echo "test" | ydotool type -

# Check ydotool permissions
groups $USER  # Should include 'ydotool'

# Start ydotool daemon (if needed)
ydotool --version
```

### Global shortcut not working

```bash
# Check input device permissions
groups $USER  # Should include 'input'

# Check device access
ls -l /dev/input/event*
```

### Transcription errors

```bash
# Check model exists
whisper-talk model list

# Validate configuration
whisper-talk validate

# Check logs
journalctl --user -u whisper-talk -f
```

### Performance issues

- Reduce model size (use `tiny` or `base` instead of `medium` or `large-v3`)
- Enable GPU acceleration (CUDA/ROCm) for Whisper backend

## Inspiration

Project was inspired by [hyprwhspr](github.com/goodroot/hyprwhspr).
