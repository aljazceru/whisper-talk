# Installation Guide

This guide covers installing whisper-talk on various Linux distributions, building from source, and setting up system permissions.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Distribution-Specific Installation](#distribution-specific-installation)
- [Building from Source](#building-from-source)
- [Permission Setup](#permission-setup)
- [Model Download](#model-download)
- [First-Time Setup](#first-time-setup)
- [Uninstallation](#uninstallation)
- [Troubleshooting](#troubleshooting)

## Prerequisites

Before installing whisper-talk, ensure your system meets the following requirements:

### System Requirements

- **Operating System**: Linux (kernel 5.0+ recommended)
- **Architecture**: x86_64 or ARM64
- **Memory**: 2GB minimum, 4GB+ recommended for larger models
- **Storage**: 100MB for base model, up to 2GB for large-v3 model
- **Microphone**: Working audio input device
- **Audio System**: ALSA, PulseAudio, or PipeWire

### Software Requirements

- **Rust**: 1.70 or later (for building from source)
- **C Compiler**: gcc or clang
- **Audio Libraries**: ALSA development libraries
- **ONNX Runtime 1.20.1**: required only for the Parakeet-v3 backend
- **CUDA toolkit**: optional, for NVIDIA GPU acceleration
- **ROCm**: optional, for AMD GPU acceleration

## Installation

The commands below use x86_64 (`x64`) binaries. If you are on ARM64,
replace `onnxruntime-linux-x64` with `onnxruntime-linux-aarch64` in the
ONNX Runtime URLs.

### Fedora / RHEL

#### Install System Dependencies

```bash
sudo dnf install alsa-lib-devel pulseaudio-libs-devel gcc pkg-config
```

#### Optional: GPU support

**NVIDIA (CUDA):**
```bash
sudo dnf install cuda-toolkit
```

**AMD (ROCm):**
```bash
sudo dnf install rocm-devel rocm-hip-runtime
```

#### ONNX Runtime (only for Parakeet-v3)

The Parakeet-v3 backend needs a compatible `libonnxruntime.so.1.20.1`. The
`whisper-talk model download parakeet-v3` command will refuse to start until it
can locate the runtime, so install it first.

**CPU (default):**
```bash
mkdir -p ~/.local/share/whisper/ort
curl -L -o /tmp/onnxruntime-linux-x64-1.20.1.tgz \
  https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz
tar -xzf /tmp/onnxruntime-linux-x64-1.20.1.tgz -C ~/.local/share/whisper/ort --strip-components=1
# Result: ~/.local/share/whisper/ort/lib/libonnxruntime.so.1.20.1
```

**NVIDIA (CUDA):**
```bash
mkdir -p ~/.local/share/whisper/ort
curl -L -o /tmp/onnxruntime-linux-x64-gpu-1.20.1.tgz \
  https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-gpu-1.20.1.tgz
tar -xzf /tmp/onnxruntime-linux-x64-gpu-1.20.1.tgz -C ~/.local/share/whisper/ort --strip-components=1
```

**AMD (ROCm):**
Microsoft does not ship a prebuilt ROCm ONNX Runtime. Install a ROCm-enabled
package such as `onnxruntime-rocm` from Fedora, or extract a custom build to
`~/.local/share/whisper/ort-rocm/usr/lib64/rocm/lib/`.

Override the search path if necessary:
```bash
export ORT_DYLIB_PATH=/path/to/libonnxruntime.so.1.20.1
```

#### Build and Install

```bash
# Clone repository
git clone https://github.com/aljazceru/whisper-talk.git
cd whisper-talk

# Build release binary
cargo build --release

# Optional: build with GPU support
cargo build --release --features cuda   # NVIDIA CUDA

# Install to system
sudo install -m 755 target/release/whisper-talk /usr/local/bin/

# Verify installation
whisper-talk --version
```

### Ubuntu / Debian

#### Install System Dependencies

```bash
sudo apt update
sudo apt install \
    libasound2-dev \
    libpulse-dev \
    build-essential \
    pkg-config
```

#### Install Rust (if not already installed)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

#### Optional: GPU support

**NVIDIA (CUDA):**
```bash
# Install NVIDIA drivers and CUDA
sudo apt install nvidia-driver-535 cuda-toolkit-12

# Verify CUDA installation
nvcc --version
```

**AMD (ROCm):**
```bash
# Install AMD drivers and ROCm
sudo apt install amdgpu-install
sudo amdgpu-install --usecase=rocm

# Verify ROCm installation
rocminfo
```

#### ONNX Runtime (only for Parakeet-v3)

The Parakeet-v3 backend needs a compatible `libonnxruntime.so.1.20.1`.
Install it before downloading the Parakeet model.

**CPU (default):**
```bash
mkdir -p ~/.local/share/whisper/ort
curl -L -o /tmp/onnxruntime-linux-x64-1.20.1.tgz \
  https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz
tar -xzf /tmp/onnxruntime-linux-x64-1.20.1.tgz -C ~/.local/share/whisper/ort --strip-components=1
# Result: ~/.local/share/whisper/ort/lib/libonnxruntime.so.1.20.1
```

**NVIDIA (CUDA):**
```bash
mkdir -p ~/.local/share/whisper/ort
curl -L -o /tmp/onnxruntime-linux-x64-gpu-1.20.1.tgz \
  https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-gpu-1.20.1.tgz
tar -xzf /tmp/onnxruntime-linux-x64-gpu-1.20.1.tgz -C ~/.local/share/whisper/ort --strip-components=1
```

**AMD (ROCm):**
Microsoft does not ship a prebuilt ROCm ONNX Runtime. Build or install a
ROCm-enabled `libonnxruntime.so.1.20.1`, or extract it to
`~/.local/share/whisper/ort-rocm/usr/lib64/rocm/lib/`.

Override the search path if necessary:
```bash
export ORT_DYLIB_PATH=/path/to/libonnxruntime.so.1.20.1
```

#### Build and Install

```bash
# Clone repository
git clone https://github.com/aljazceru/whisper-talk.git
cd whisper-talk

# Build release binary
cargo build --release

# Optional: build with GPU support
cargo build --release --features cuda   # NVIDIA CUDA

# Install to system
sudo install -m 755 target/release/whisper-talk /usr/local/bin/

# Verify installation
whisper-talk --version
```

### Arch Linux

#### Install System Dependencies

```bash
sudo pacman -S \
    alsa-lib \
    pulseaudio \
    base-devel \
    pkg-config
```

#### Optional: GPU support

**NVIDIA (CUDA):**
```bash
sudo pacman -S cuda
```

**AMD (ROCm):**
```bash
sudo pacman -S rocm-hip-runtime rocm-dev
```

#### ONNX Runtime (only for Parakeet-v3)

The Parakeet-v3 backend needs a compatible `libonnxruntime.so.1.20.1`.
Install it before downloading the Parakeet model.

**CPU (default):**
```bash
mkdir -p ~/.local/share/whisper/ort
curl -L -o /tmp/onnxruntime-linux-x64-1.20.1.tgz \
  https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz
tar -xzf /tmp/onnxruntime-linux-x64-1.20.1.tgz -C ~/.local/share/whisper/ort --strip-components=1
# Result: ~/.local/share/whisper/ort/lib/libonnxruntime.so.1.20.1
```

**NVIDIA (CUDA):**
```bash
mkdir -p ~/.local/share/whisper/ort
curl -L -o /tmp/onnxruntime-linux-x64-gpu-1.20.1.tgz \
  https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-gpu-1.20.1.tgz
tar -xzf /tmp/onnxruntime-linux-x64-gpu-1.20.1.tgz -C ~/.local/share/whisper/ort --strip-components=1
```

**AMD (ROCm):**
Microsoft does not ship a prebuilt ROCm ONNX Runtime. Build or install a
ROCm-enabled `libonnxruntime.so.1.20.1`, or extract it to
`~/.local/share/whisper/ort-rocm/usr/lib64/rocm/lib/`.

Override the search path if necessary:
```bash
export ORT_DYLIB_PATH=/path/to/libonnxruntime.so.1.20.1
```

#### Build and Install

```bash
# Clone repository
git clone https://github.com/aljazceru/whisper-talk.git
cd whisper-talk

# Build release binary
cargo build --release

# Optional: build with GPU support
cargo build --release --features cuda   # NVIDIA CUDA

# Install to system
sudo install -m 755 target/release/whisper-talk /usr/local/bin/

# Verify installation
whisper-talk --version
```

#### AUR Installation (Alternative)

If an AUR package is available:

```bash
paru -S whisper-talk
# or
yay -S whisper-talk
```

## Building from Source

### Development Build

For development and testing:

```bash
# Clone repository
git clone https://github.com/aljazceru/whisper-talk.git
cd whisper-talk

# Development build (faster compilation)
cargo build

# Run from build directory
./target/debug/whisper-talk --help
```

### Release Build (Optimized)

For production use:

```bash
# Release build with optimizations
cargo build --release

# The release binary will be at target/release/whisper-talk
```

### Optional Features

Build with optional features:

```bash
# Desktop notifications
cargo build --release --features notifications

# NVIDIA CUDA support for Whisper and Parakeet
cargo build --release --features cuda

# AMD ROCm support for Whisper
cargo build --release --features hipblas
```

### Testing

Run tests after building:

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_audio_capture
```

## Permission Setup

whisper-talk requires specific system permissions to function correctly. Run these commands to set up proper permissions.

### ydotool (Text Injection)

ydotool is used for injecting text into applications. It requires a dedicated group.

```bash
# Create ydotool group
sudo groupadd ydotool

# Add your user to ydotool group
sudo usermod -aG ydotool $USER

# Start ydotool daemon
ydotool --version

# If ydotool is not installed, install it:
# Fedora/RHEL
sudo dnf install ydotool

# Ubuntu/Debian
sudo apt install ydotool

# Arch Linux
sudo pacman -S ydotool
```

### Audio Group

Access to microphone requires audio group membership.

```bash
# Add your user to audio group
sudo usermod -aG audio $USER
```

### Input Group

Global keyboard shortcuts require input group membership.

```bash
# Add your user to input group
sudo usermod -aG input $USER
```

### Verify Groups

```bash
# Check current groups
groups

# Expected output should include: audio, input, ydotool
```

### Apply Changes

```bash
# Log out and log back in for group changes to take effect
# Or restart your session

# On GNOME/GDM
# Click your user → Log Out → Log in again

# On TTY
# Press Ctrl+Alt+F[1-6] to switch to TTY
# Login, then run: systemctl restart gdm
```

## Model Download

whisper-talk requires a speech recognition model. Models can be downloaded via the CLI or setup wizard.

### Using Setup Wizard (Recommended)

```bash
# Interactive model selection and download
whisper-talk setup
```

### Manual Model Download

#### Whisper Models

Models are automatically downloaded from HuggingFace on first use:

```bash
# List available models
whisper-talk model list

# Download specific model
whisper-talk model download base

# Download multiple models
whisper-talk model download tiny
whisper-talk model download small

# Remove model
whisper-talk model remove base
```

**Model Sizes:**

| Model   | Size  | Speed | Accuracy | Recommended For |
|---------|-------|-------|----------|-----------------|
| tiny    | 39MB  | Fastest | Lowest   | Testing, low-end systems |
| base    | 74MB  | Fast   | Good     | Daily use (recommended) |
| small   | 244MB | Medium | Better   | Better accuracy needed |
| medium  | 769MB | Slow   | High     | Professional use |
| large-v3| 1.5GB | Slowest| Highest  | Best accuracy |

**Download Locations:**

Models are stored in:
- `~/.local/share/whisper/`

#### Manual Download

If automatic download fails, download models manually:

**Whisper Models (from HuggingFace):**

```bash
# Base model example
mkdir -p ~/.local/share/whisper
wget https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin \
  -O ~/.local/share/whisper/base.bin
```

**Available models:** tiny, base, small, medium, large-v1, large-v2, large-v3

**Parakeet-v3 Model (from HuggingFace):**

The Parakeet-v3 files are fetched from `istupakov/parakeet-tdt-0.6b-v3-onnx`.
Make sure you have already installed ONNX Runtime 1.20.1 (see the
platform-specific instructions above).

```bash
# Create model directory
mkdir -p ~/.local/share/whisper/parakeet-v3

# Download the three required files
base_url="https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main"
wget "$base_url/vocab.txt" \
  -O ~/.local/share/whisper/parakeet-v3/vocab.txt
wget "$base_url/encoder-model.int8.onnx" \
  -O ~/.local/share/whisper/parakeet-v3/encoder-model.int8.onnx
wget "$base_url/decoder_joint-model.int8.onnx" \
  -O ~/.local/share/whisper/parakeet-v3/decoder_joint-model.int8.onnx
```

Configure Parakeet as the active backend:

```bash
whisper-talk config set transcription.backend ParakeetV3
whisper-talk config set transcription.model parakeet-v3
```

## First-Time Setup

### Interactive Setup Wizard

The setup wizard guides you through all configuration steps:

```bash
whisper-talk setup
```

The wizard will:

1. **Select Backend**: Choose Whisper (recommended) or Parakeet-v3
2. **Select Model**: Choose model size based on your needs
3. **Select Audio Device**: Choose your microphone from detected devices
4. **Download Model**: Automatically download selected model
5. **Setup Permissions**: Display commands to set up groups
6. **Install systemd**: Optionally install and enable auto-start service

### Manual Configuration

If you prefer manual configuration:

```bash
# Create config directory
mkdir -p ~/.config/whisper-talk

# Create default config
whisper-talk config init

# Edit configuration
whisper-talk config edit

# Or edit manually
nano ~/.config/whisper-talk/config.json
```

Example minimal config:

```json
{
  "shortcuts": {
    "primary_shortcut": "SUPER+ALT+D"
  },
  "transcription": {
    "backend": "whisper",
    "model": "base"
  },
  "audio": {
    "mute_detection": true
  }
}
```

### Validate Setup

After setup, validate your configuration:

```bash
# Validate configuration and system
whisper-talk validate

# Check status
whisper-talk status

# Test recording (interactive mode)
whisper-talk run --interactive
```

### Start the Daemon

```bash
# Start in foreground (for testing)
whisper-talk run

# Start in background
whisper-talk run &

# Or use systemd for auto-start
systemctl --user start whisper-talk
```

## Uninstallation

### Remove Binary

```bash
# If installed system-wide
sudo rm /usr/local/bin/whisper-talk

# If installed via cargo
cargo uninstall whisper-talk
```

### Remove Configuration and Data

```bash
# Remove config directory
rm -rf ~/.config/whisper-talk

# Remove data directory (state)
rm -rf ~/.local/share/whisper-talk

# Remove model and ONNX runtime directory
rm -rf ~/.local/share/whisper

# Remove state directory
rm -rf ~/.local/state/whisper-talk

# Remove cache
rm -rf ~/.cache/whisper-talk
```

### Remove systemd Service

```bash
# Stop service
systemctl --user stop whisper-talk

# Disable service
systemctl --user disable whisper-talk

# Remove service file
rm -f ~/.config/systemd/user/whisper-talk.service

# Reload systemd
systemctl --user daemon-reload
```

### Automated Uninstall

```bash
# Use uninstall command (removes everything)
whisper-talk uninstall

# Confirm prompts will guide you through removal
```

## Troubleshooting

### Build Errors

#### Missing ALSA development libraries

**Error:**
```
error: The system library `alsa` required by crate `alsa-sys` was not found
```

**Solution:**
```bash
# Fedora/RHEL
sudo dnf install alsa-lib-devel

# Ubuntu/Debian
sudo apt install libasound2-dev

# Arch Linux
sudo pacman -S alsa-lib
```

### Runtime Errors

#### Permission denied on /dev/input/event*

**Error:**
```
Failed to open input device: Permission denied
```

**Solution:**
```bash
# Add user to input group
sudo usermod -aG input $USER

# Set device permissions (temporary)
sudo chmod 666 /dev/input/event*

# Log out and log back in
```

#### ydotool: command not found

**Error:**
```
Failed to inject text: No such file or directory (os error 2)
```

**Solution:**
```bash
# Install ydotool
sudo dnf install ydotool  # Fedora
sudo apt install ydotool  # Ubuntu
sudo pacman -S ydotool    # Arch

# Add user to ydotool group
sudo groupadd ydotool
sudo usermod -aG ydotool $USER

# Log out and log back in
```

#### No audio devices found

**Error:**
```
No audio input devices detected
```

**Solution:**
```bash
# Check if microphone is detected
arecord -l

# Or with PulseAudio
pactl list sources short

# Check audio permissions
groups $USER  # Should include 'audio'

# Test microphone
arecord -f cd -d 5 test.wav
```

### Model Download Issues

#### Network connection failed

**Error:**
```
Failed to download model: Network error
```

**Solution:**
```bash
# Check internet connection
ping huggingface.co

# Manually download model
mkdir -p ~/.local/share/whisper
wget https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin \
  -O ~/.local/share/whisper/base.bin

# Verify model exists
ls -l ~/.local/share/whisper/
```

#### Model file corrupted

**Error:**
```
Failed to load model: Invalid model file
```

**Solution:**
```bash
# Remove corrupted model
rm ~/.local/share/whisper/base.bin

# Re-download
whisper-talk model download base
```

### systemd Service Issues

#### Service fails to start

**Error:**
```
Failed to start whisper-talk.service: Unit not found
```

**Solution:**
```bash
# Install service file
whisper-talk systemd install

# Reload systemd
systemctl --user daemon-reload

# Start service
systemctl --user start whisper-talk
```

#### Check service logs

```bash
# View service logs
journalctl --user -u whisper-talk -f

# View last 100 lines
journalctl --user -u whisper-talk -n 100

# View logs since boot
journalctl --user -u whisper-talk --since boot
```

### Performance Issues

#### Slow transcription

**Symptoms:** Long delay between stopping recording and text appearing.

**Solutions:**

1. **Use smaller model:**
```bash
whisper-talk config set transcription.model tiny
```

2. **Enable GPU acceleration:**
   - Install CUDA or ROCm
   - Rebuild with GPU support
   - Model will automatically use GPU if available

3. **Increase threads:**
```bash
whisper-talk config set transcription.threads 8
```

4. **Disable audio feedback:**
```bash
whisper-talk config set feedback.audio_feedback false
```

#### High CPU usage

**Symptoms:** High CPU usage even when idle.

**Solutions:**

1. **Use smaller model**
2. **Reduce thread count**
3. **Check for stuck recording state:**
```bash
whisper-talk status
whisper-talk state stop-recording  # Force stop if stuck
```

### Additional Help

If you encounter issues not covered here:

1. **Check logs:**
```bash
journalctl --user -u whisper-talk -f
```

2. **Validate configuration:**
```bash
whisper-talk validate
```

3. **Enable debug logging:**
```bash
RUST_LOG=debug whisper-talk run
```

4. **Report issues:**
   - https://github.com/aljazceru/whisper-talk/issues
   - Include system info, logs, and reproduction steps

## Next Steps

After successful installation:

- Read [CONFIGURATION.md](CONFIGURATION.md) for detailed configuration options
- Read [CONTRIBUTING.md](CONTRIBUTING.md) if you want to contribute

Enjoy using whisper-talk!
