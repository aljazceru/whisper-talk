# whisper-talk

System-wide voice dictation for Linux with Whisper and Parakeet-v3 backends.

## Overview

**whisper-talk** is a lightweight, efficient voice dictation daemon for Linux that provides system-wide, local speech-to-text with either Whisper or NVIDIA Parakeet-v3. Press a global hotkey, speak, and have the transcribed text copied to your clipboard so you can paste it into any application.

### Key Features

- **Privacy-focused** - Local transcription only, no cloud services required
- **Two transcription backends** - Whisper for broad model and translation support, or Parakeet-v3 for fast multilingual transcription
- **CPU and GPU acceleration** - CPU by default, NVIDIA CUDA and AMD ROCm support
- **Recording modes** - Toggle, Push-to-Talk, and Auto (hybrid) modes
- **OpenAI compatible whisper endpoint** - Use it in other applications as well
- **Optional tray control panel** - Build with `--features gui` to configure the backend and microphone and inspect recent errors

### Backend and GPU Support

| Backend | CPU | NVIDIA CUDA | AMD ROCm | Translation to English |
|---------|-----|-------------|----------|------------------------|
| Whisper | Yes | `--features cuda` | `--features hipblas` | Yes |
| Parakeet-v3 | Yes | `--features cuda` | Yes, with a ROCm-enabled ONNX Runtime | No |

Parakeet-v3 requires a compatible ONNX Runtime 1.20.1 shared library for CPU,
CUDA, or ROCm. See [INSTALL.md](INSTALL.md) for platform-specific runtime and
driver setup.

### How It Works

1. Press your configured global shortcut (default: `SUPER+ALT+D`)
2. whisper-talk starts recording from your microphone
3. Speak your text
4. Press the shortcut again
5. Audio is transcribed locally using your selected Whisper or Parakeet-v3 backend
6. Transcribed text is injected into clipboard

## Quick Start

### Prerequisites

- Linux system (Fedora, Ubuntu, Arch, or derivative)
- Rust 1.92+ for building from source
- Microphone input device
- ydotool for clipboard injection
- ONNX Runtime 1.20.1 when using Parakeet-v3
- CUDA or ROCm libraries only when building for the corresponding GPU backend
- Optional GUI: GTK 4.10+ development libraries and a StatusNotifier tray host for hidden startup

### Installation

```bash
# Clone the repository
git clone https://github.com/aljazceru/whisper-talk.git
cd whisper-talk

# Install base system dependencies (Fedora)
sudo dnf install alsa-lib-devel pulseaudio-libs-devel systemd-devel ydotool wl-clipboard

# Or for Ubuntu/Debian
sudo apt install libasound2-dev libpulse-dev libudev-dev ydotool wl-clipboard

# Build and install for CPU
cargo install --locked --path .

# Or build and install with NVIDIA CUDA support for Whisper and Parakeet-v3
cargo install --locked --path . --features cuda

# Or build and install with AMD ROCm support for Whisper
# Parakeet-v3 uses a separately installed ROCm-enabled ONNX Runtime.
cargo install --locked --path . --features hipblas

# Run setup wizard
whisper-talk setup
```

### Initial Setup

The interactive setup wizard guides you through:

1. Selecting Whisper or Parakeet-v3 as the transcription backend
2. Choosing a Whisper model size, or downloading the Parakeet-v3 model
3. Selecting your audio input device
4. Setting up user permissions
5. Optionally installing systemd service

```bash
whisper-talk setup
```

You can also download and select a backend manually:

```bash
# Whisper (default)
whisper-talk model download base
whisper-talk config set transcription.backend whisper
whisper-talk config set transcription.model base

# Parakeet-v3 (install ONNX Runtime 1.20.1 first)
whisper-talk model download parakeet-v3
whisper-talk config set transcription.backend parakeet_v3
whisper-talk config set transcription.model parakeet-v3
```

### Basic Usage

```bash
# Start the daemon (headless by default)
whisper-talk daemon

# Or run in background with systemd
systemctl --user start whisper-talk

# Press SUPER+ALT+D to start/stop recording
```

### Optional GUI

Install `gtk4-devel` (Fedora), `libgtk-4-dev` (Ubuntu/Debian), or `gtk4` (Arch)
before enabling the GUI feature.

Build with `cargo install --locked --path . --features gui` to include the
control panel. GUI-enabled builds start hidden in the tray on a graphical
desktop. GPU features can be combined, for example `--features gui,cuda`.

Open **Show Settings** from the microphone tray icon to select **Whisper** or
**Parakeet v3**, enter an installed model name, and choose a microphone. **Apply**
loads the replacement backend before saving and switching; a failed change
leaves the working settings intact. Download models with `whisper-talk model
download <model>` first. Recording/processing disables Apply.

Closing the window hides it; **Quit** stops dictation too. If no tray host is
available, the window opens instead, and closing it quits. GNOME requires a
StatusNotifier/AppIndicator extension for tray icons.

```bash
# Start with settings visible
whisper-talk daemon --show-settings

# Run without a desktop window or tray
whisper-talk daemon --no-gui

# Build without GTK/tray dependencies
cargo install --locked --path .
```

The panel keeps the latest 100 operational errors for this process. It does not
replace service logs: `journalctl --user -u whisper-talk`.

## OpenAI-Compatible API

Run the daemon with either:

```bash
whisper-talk daemon --api-bind 127.0.0.1:11434
```

Or configure it in the config file (see below):

```json
{
  "api_bind": "127.0.0.1:11434"
}
```

If neither is set, API is disabled and existing local hotkey behavior is unchanged.

The API serves OpenAI-compatible routes under `/v1` (for example `/v1/audio/transcriptions`).

Then request transcription with OpenAI-compatible multipart format:

```bash
curl -X POST \
  -F file=@/path/to/audio.wav \
  -F model=whisper-1 \
  -F language=en \
  -F response_format=json \
  http://127.0.0.1:11434/v1/audio/transcriptions
```

Response:

```json
{ "text": "..." }
```

Supported uploads include WAV by default and common formats like MP3/OGG/FLAC when detected.
Response format is JSON by default and `text` when `response_format=text`.

Whisper also supports translation requests to English:

```bash
curl -X POST \
  -F file=@/path/to/audio.mp3 \
  -F model=whisper-1 \
  -F prompt="Translate the audio to English" \
  http://127.0.0.1:11434/v1/audio/translations
```

The Parakeet-v3 backend supports transcription only and returns an unsupported-operation error for this endpoint.

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
  },
  "api_bind": "127.0.0.1:11434"
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

For the tray to appear, the user service must inherit the graphical session's
display and D-Bus environment. If it starts headless, import the display
environment from a desktop terminal, then restart:

```bash
systemctl --user import-environment DISPLAY WAYLAND_DISPLAY XDG_CURRENT_DESKTOP
systemctl --user restart whisper-talk
```

### Dependency maintenance

Use `--locked` for reproducible builds and `cargo audit` to check the lockfile.
The November 2026 audit updated vulnerable TLS/network dependencies and moved
Rodio to 0.23, sharing CPAL 0.18 with microphone capture. The resulting lockfile
has no reported vulnerabilities, but these maintenance constraints remain:

| Dependency | Retained version | Reason |
|------------|------------------|--------|
| Parakeet / ORT | Vendored 0.3.7 / 2.0.0-rc.13 | The graph-optimization patch and `api-20` preserve ONNX Runtime 1.20.1 / ROCm compatibility |
| zbus / notify-rust | 5.19 / 4.18.2 | A coordinated major migration is needed; current ksni also requires zbus 5 |
| dirs | 6.0 | Version 7 changes home-directory behavior; no security fix requires the change |

`cargo audit` still reports four **unmaintained** warnings: `paste` through
Parakeet's tokenizer, and `zbus_names`, `zvariant`, `zvariant_derive` through the
D-Bus stack. A direct zbus upgrade alone cannot remove the transitive warnings.
Native ONNX Runtime and GPU drivers are managed separately from Cargo.

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
- Build with `--features cuda` for NVIDIA acceleration with Whisper or Parakeet-v3
- Build with `--features hipblas` for AMD ROCm acceleration with Whisper
- Install a ROCm-enabled ONNX Runtime 1.20.1 for AMD acceleration with Parakeet-v3

## Inspiration

Project was inspired by [hyprwhspr](github.com/goodroot/hyprwhspr).
