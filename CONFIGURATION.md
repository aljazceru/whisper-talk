# Configuration Reference

This document provides a comprehensive reference for all whisper-talk configuration options.

## Table of Contents

- [Configuration File Location](#configuration-file-location)
- [Configuration Structure](#configuration-structure)
- [Shortcuts Configuration](#shortcuts-configuration)
- [Audio Configuration](#audio-configuration)
- [Transcription Configuration](#transcription-configuration)
- [Injection Configuration](#injection-configuration)
- [Feedback Configuration](#feedback-configuration)
- [API Configuration](#api-configuration)
- [Example Configurations](#example-configurations)
- [CLI Configuration Commands](#cli-configuration-commands)

## Configuration File Location

whisper-talk follows XDG Base Directory specifications:

| Type | Location |
|------|----------|
| Config file | `~/.config/whisper-talk/config.json` |
| Models | `~/.local/share/whisper-talk/models/` |
| State | `~/.local/state/whisper-talk/` |
| Cache | `~/.cache/whisper-talk/` |

## Configuration Structure

The configuration file is JSON with the following top-level sections:

```json
{
  "shortcuts": { ... },
  "audio": { ... },
  "transcription": { ... },
  "injection": { ... },
  "feedback": { ... },
  "api_bind": "127.0.0.1:11434"
}
```

## Shortcuts Configuration

Controls global keyboard shortcuts and recording behavior.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `primary_shortcut` | string | `"SUPER+ALT+D"` | Global hotkey to trigger dictation |
| `recording_mode` | enum | `"toggle"` | Recording mode: `toggle`, `push_to_talk`, `auto` |
| `grab_keys` | boolean | `false` | Grab keyboard events during recording |
| `auto_mode_threshold_ms` | integer | `400` | Auto mode detection threshold in milliseconds |

### primary_shortcut

Global shortcut to start/stop dictation. Format:

```
<SUPER|CTRL|ALT|SHIFT>+<KEY>
```

**Examples:**
- `SUPER+ALT+D` - Default
- `CTRL+SHIFT+SPACE` - Alternative
- `ALT+F1` - Simple
- `SUPER+F12` - No conflicts

**Key names:** A-Z, 0-9, F1-F12, SPACE, TAB, ENTER, ESCAPE, BACKSPACE, DELETE, HOME, END, PAGE_UP, PAGE_DOWN

**Modifiers:** SUPER, CTRL, ALT, SHIFT (can combine multiple)

### recording_mode

How the shortcut controls recording:

| Mode | Description |
|------|-------------|
| `toggle` | Press shortcut to start, press again to stop (default) |
| `push_to_talk` | Hold shortcut to record, release to stop |
| `auto` | Press shortcut once, auto-detect when you stop speaking |

#### Toggle Mode

Press the shortcut to start recording. Press it again to stop and transcribe.

**Best for:** Long dictation sessions, multiple sentences.

#### Push-to-Talk Mode

Hold down the shortcut while speaking. Release when done.

**Best for:** Quick phrases, single commands, privacy (only records while holding).

#### Auto Mode

Press the shortcut once. Recording automatically stops when speech stops (silence detected).

**Best for:** Convenience, hands-free operation.

### grab_keys

If `true`, grabs all keyboard input during recording to prevent keypresses from being recorded.

**Caution:** This prevents typing while recording. Only enable if you have issues with keypresses being recorded.

### auto_mode_threshold_ms

Time (in milliseconds) of silence before auto-mode stops recording.

**Range:** 100-5000ms
**Default:** 400ms

Adjust based on your speaking style:
- Faster speakers: 200-300ms
- Normal pace: 400-500ms (default)
- Slower speakers: 600-800ms

## Audio Configuration

Controls audio input device and detection settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `device_id` | integer | `null` | ALSA/PulseAudio device ID |
| `device_name` | string | `null` | Device name for matching |
| `device_vendor_id` | string | `null` | USB vendor ID for matching |
| `device_model_id` | string | `null` | USB model ID for matching |
| `mute_detection` | boolean | `true` | Detect muted/unplugged microphones |
| `zero_volume_threshold` | float | `5e-7` | Threshold for detecting zero volume |

### Device Selection

whisper-talk uses a priority order for device selection:

1. `device_id` (exact match)
2. `device_name` + `device_vendor_id` + `device_model_id` (USB match)
3. `device_name` (partial match)
4. Default system input device

#### Finding Your Device ID

```bash
# List ALSA devices
arecord -l

# List PulseAudio devices
pactl list sources short

# Or use whisper-talk
whisper-talk setup  # Lists devices during setup
```

**ALSA output example:**
```
card 1: USB [USB Microphone], device 0: USB Audio [USB Audio]
  Subdevices: 1/1
  Subdevice #0: subdevice #0
```

Device ID format: `1,0` (card,device)

#### USB Device Matching

For USB microphones, you can match by vendor/model ID:

```bash
# Find USB device IDs
lsusb | grep -i microphone
# Output: Bus 001 Device 005: ID 0d8c:0014 C-Media Electronics, Inc.

# vendor_id = "0d8c"
# model_id = "0014"
```

**Configuration example:**
```json
{
  "audio": {
    "device_vendor_id": "0d8c",
    "device_model_id": "0014"
  }
}
```

#### Device Name Matching

Partial string match against device name:

```json
{
  "audio": {
    "device_name": "USB Microphone"
  }
}
```

Matches: "USB Microphone", "USB Microphone XYZ", "My USB Microphone"

### mute_detection

Automatically detect if microphone is muted or unplugged.

**Behavior:**
- If detected mute/unplug: Stops recording and shows error
- Prevents: Recording silence and wasting time
- False positives: Increase `zero_volume_threshold`

**Recommended:** `true` (default)

### zero_volume_threshold

RMS threshold below which audio is considered silent.

**Range:** 1e-9 to 1e-3
**Default:** 5e-7

**Adjustment guidelines:**
- Too sensitive (false positives): Increase to 1e-6 or 1e-5
- Not sensitive (misses mute): Decrease to 1e-8 or 1e-9

**Testing:**
```bash
# Run whisper-talk with debug to see audio levels
RUST_LOG=debug whisper-talk run

# Speak and watch RMS values
# Adjust threshold accordingly
```

## Transcription Configuration

Controls speech recognition backend and model settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `backend` | enum | `"whisper"` | Backend: `whisper` or `parakeet_v3` |
| `model` | string | `"base"` | Model name |
| `threads` | integer | `4` | Number of CPU threads |
| `language` | string | `null` | Language code (null = auto-detect) |
| `word_overrides` | object | `{}` | Word replacement mappings |
| `whisper_prompt` | string | See below | Whisper transcription prompt |
| `hallucination_markers` | array | See below | Text to filter out |

### backend

Speech recognition backend:

| Backend | Description | GPU Support |
|---------|-------------|-------------|
| `whisper` | OpenAI Whisper model | CUDA, ROCm, CPU |
| `parakeet_v3` | NVIDIA Parakeet-v3 | CUDA only |

See [BACKENDS.md](BACKENDS.md) for detailed backend setup.

### model

Model name/size:

#### Whisper Models

| Model | Size | Speed | Accuracy | VRAM (GPU) | RAM (CPU) |
|-------|------|-------|----------|------------|-----------|
| `tiny` | 39MB | Fastest | Lowest | 1GB | 500MB |
| `base` | 74MB | Fast | Good | 1GB | 500MB |
| `small` | 244MB | Medium | Better | 2GB | 1GB |
| `medium` | 769MB | Slow | High | 4GB | 2GB |
| `large-v3` | 1.5GB | Slowest | Highest | 6GB | 4GB |

**Recommendation:** Start with `base`, upgrade to `small` or `medium` if accuracy is insufficient.

#### Parakeet Models

| Model | Size | Accuracy | Speed |
|-------|------|----------|-------|
| `parakeet-v3` | ~500MB | High | Fast |

**Requirement:** NVIDIA GPU with CUDA

### threads

Number of CPU threads for transcription (Whisper backend only).

**Guidelines:**
- `2-4`: Dual/Quad core systems
- `4-8`: Hex/Octa core systems
- `8+`: High-performance systems (diminishing returns)

**Formula:** `min(physical_cores / 2, 8)`

**GPU mode:** Can reduce threads since GPU does most work.

### language

Force specific language for transcription (null = auto-detect).

**Format:** ISO 639-1 language code

**Common languages:**
- `en` - English
- `es` - Spanish
- `fr` - French
- `de` - German
- `it` - Italian
- `pt` - Portuguese
- `ru` - Russian
- `zh` - Chinese
- `ja` - Japanese
- `ko` - Korean

**Recommendation:** Set this if you primarily speak one language for faster, more accurate transcription.

**Example:**
```json
{
  "transcription": {
    "language": "en"
  }
}
```

### word_overrides

Map words/phrases to replace during transcription.

**Use cases:**
- Spoken words that should be symbols
- Special terminology corrections
- Auto-capitalization hints
- Formatting shortcuts

**Example:**
```json
{
  "transcription": {
    "word_overrides": {
      "period": ".",
      "comma": ",",
      "question mark": "?",
      "exclamation mark": "!",
      "new line": "\n",
      "new paragraph": "\n\n",
      "colon": ":",
      "semicolon": ";",
      "dollar sign": "$",
      "at sign": "@",
      "hashtag": "#",
      "percent": "%"
    }
  }
}
```

**Advanced example:**
```json
{
  "transcription": {
    "word_overrides": {
      "open parenthesis": "(",
      "close parenthesis": ")",
      "open bracket": "[",
      "close bracket": "]",
      "open brace": "{",
      "close brace": "}",
      "ampersand": "&",
      "asterisk": "*",
      "underscore": "_",
      "plus": "+",
      "minus": "-",
      "equals": "=",
      "less than": "<",
      "greater than": ">",
      "forward slash": "/",
      "backslash": "\\",
      "pipe": "|",
      "tilde": "~",
      "caret": "^"
    }
  }
}
```

### whisper_prompt

Prompt to guide Whisper's transcription style.

**Default:**
```
"Transcribe with proper capitalization, including sentence beginnings, proper nouns, titles, and standard English capitalization rules."
```

**Customization ideas:**

**For technical dictation:**
```json
{
  "transcription": {
    "whisper_prompt": "Transcribe with technical precision, including programming terms, commands, and code formatting."
  }
}
```

**For casual dictation:**
```json
{
  "transcription": {
    "whisper_prompt": "Transcribe naturally with appropriate contractions and informal language."
  }
}
```

**For meeting notes:**
```json
{
  "transcription": {
    "whisper_prompt": "Transcribe as meeting notes with clear separation of speakers and action items."
  }
}
```

### hallucination_markers

Text patterns that indicate transcription failure (silence, noise, etc.).

**Default:**
```json
["(blank audio)", "[BLANK_AUDIO]", "[ Silence ]", "♪"]
```

**Behavior:**
- If transcription output matches any marker, it's treated as empty
- Result: No text is typed
- Feedback: Error sound plays

**Add custom markers:**
```json
{
  "transcription": {
    "hallucination_markers": [
      "(blank audio)",
      "[BLANK_AUDIO]",
      "[ Silence ]",
      "♪",
      "(unclear)",
      "[NO SPEECH]",
      "(noise)",
      "(mumble)"
    ]
  }
}
```

## Injection Configuration

Controls how transcribed text is inserted into applications.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `paste_mode` | enum | `"ctrl_shift"` | Paste shortcut: `ctrl_shift`, `ctrl`, `super` |
| `auto_submit` | boolean | `false` | Press Enter after injection |
| `clipboard_behavior` | boolean | `false` | Use clipboard instead of typing |
| `clipboard_clear_delay` | float | `5.0` | Seconds before clearing clipboard |

### paste_mode

Keyboard shortcut to paste text.

| Mode | Shortcut | Best For |
|------|----------|----------|
| `ctrl_shift` | Ctrl+Shift+V | Wayland compositors, terminal |
| `ctrl` | Ctrl+V | X11, some Wayland apps |
| `super` | Super+V | Hyprland, some compositors |

**Note:** Some terminals override Ctrl+Shift+V for paste from primary selection.

**If text doesn't paste:**
- Try different `paste_mode`
- Use `clipboard_behavior` instead
- Check application-specific paste shortcuts

### auto_submit

Automatically press Enter after injecting text.

**Use cases:**
- **False (default):** For multiple sentences, editing
- **True:** For single-line inputs, commands

**Examples:**
```json
{
  "injection": {
    "auto_submit": true
  }
}
```

**Best for:** Chat messages, search queries, command line

### clipboard_behavior

Use clipboard paste instead of typing each character.

**Advantages:**
- Faster for long text
- More reliable in some apps
- Preserves formatting

**Disadvantages:**
- Overwrites clipboard content
- Some apps block clipboard paste

**Example:**
```json
{
  "injection": {
    "clipboard_behavior": true,
    "clipboard_clear_delay": 10.0
  }
}
```

### clipboard_clear_delay

Seconds to wait before clearing clipboard after paste.

**Range:** 1-60 seconds
**Default:** 5.0 seconds

**Purpose:** Allow clipboard managers and other apps to capture the content.

**Adjustment:**
- **Shorter (1-3s):** Privacy-focused, clears quickly
- **Longer (10-30s):** For clipboard managers, multiple pastes
- **0:** Never clear (not recommended)

## Feedback Configuration

Controls audio and visual feedback during recording.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `mic_osd_enabled` | boolean | `true` | Enable microphone OSD |
| `audio_feedback` | boolean | `true` | Enable start/stop sounds |
| `master_volume` | float | `1.0` | Master feedback volume (0-2) |
| `start_sound_volume` | float | `1.0` | Recording start sound volume |
| `stop_sound_volume` | float | `1.0` | Recording stop sound volume |
| `error_sound_volume` | float | `1.0` | Error sound volume |
| `start_sound_path` | string | `null` | Custom start sound file path |
| `stop_sound_path` | string | `null` | Custom stop sound file path |
| `error_sound_path` | string | `null` | Custom error sound file path |

### mic_osd_enabled

Enable on-screen display showing recording status.

**Features:**
- Visual indicator when recording
- Audio level meter
- Recording timer
- Status messages

**Note:** The overlay daemon is not implemented yet; enabling this flag only
stores the preference for the future overlay.

**Disable for:**
- Headless systems
- Minimal setups
- Privacy concerns

### audio_feedback

Play sounds for recording events.

**Sounds:**
- **Start:** "Ding" when recording begins
- **Stop:** "Dong" when recording ends
- **Error:** "Buzz" on errors

**Disable for:**
- Quiet environments
- Privacy
- Reduced distractions

### Volume Controls

All volumes are multipliers relative to system volume.

**Range:** 0.0 (muted) to 2.0 (double)

**Settings:**
- `master_volume`: Global multiplier for all sounds
- `start_sound_volume`: Recording start sound
- `stop_sound_volume`: Recording stop sound
- `error_sound_volume`: Error sound

**Example:**
```json
{
  "feedback": {
    "master_volume": 0.5,
    "start_sound_volume": 0.8,
    "stop_sound_volume": 0.8,
    "error_sound_volume": 1.0
  }
}
```

### Custom Sound Files

Replace default sounds with custom audio files.

**Supported formats:** WAV, OGG, FLAC, MP3

**Example:**
```json
{
  "feedback": {
    "start_sound_path": "/home/user/sounds/record_start.ogg",
    "stop_sound_path": "/home/user/sounds/record_stop.ogg",
    "error_sound_path": "/home/user/sounds/error.ogg"
  }
}
```

**Default sound locations:**
- `/usr/share/gwhspr/sounds/`
- `~/.local/share/gwhspr/sounds/`

## API Configuration

Controls the optional OpenAI-compatible API listener.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `api_bind` | string | `null` | Address for API listener, e.g. `"127.0.0.1:11434"` |

### api_bind

Set `api_bind` to a socket address to enable the API server. If unset or `null`,
the API server is disabled and `whisper-talk` runs local hotkey mode only.

When configured, API routes are served under `/v1`:

- `GET /v1/models`
- `POST /v1/audio/transcriptions`
- `POST /v1/audio/translations`

**Example:**

```json
{
  "api_bind": "127.0.0.1:11434"
}
```

To disable after it was enabled in config, set it to `null`.

## Example Configurations

### Minimal Configuration

```json
{
  "shortcuts": {
    "primary_shortcut": "SUPER+ALT+D"
  },
  "transcription": {
    "backend": "whisper",
    "model": "base"
  }
}
```

### Fast Configuration (Tiny Model, No Feedback)

```json
{
  "shortcuts": {
    "primary_shortcut": "SUPER+SPACE",
    "recording_mode": "push_to_talk"
  },
  "transcription": {
    "model": "tiny",
    "language": "en",
    "threads": 2
  },
  "feedback": {
    "mic_osd_enabled": false,
    "audio_feedback": false
  }
}
```

### High Accuracy Configuration

```json
{
  "shortcuts": {
    "primary_shortcut": "CTRL+ALT+D",
    "recording_mode": "toggle"
  },
  "audio": {
    "device_name": "USB Microphone",
    "mute_detection": true,
    "zero_volume_threshold": 1e-7
  },
  "transcription": {
    "model": "medium",
    "language": "en",
    "threads": 8,
    "whisper_prompt": "Transcribe with high accuracy, including proper punctuation, capitalization, and technical terms."
  },
  "injection": {
    "paste_mode": "ctrl_shift",
    "auto_submit": false,
    "clipboard_behavior": true,
    "clipboard_clear_delay": 10.0
  },
  "feedback": {
    "mic_osd_enabled": true,
    "audio_feedback": true,
    "master_volume": 0.7
  }
}
```

### Push-to-Talk Configuration

```json
{
  "shortcuts": {
    "primary_shortcut": "ALT+GRAVE",
    "recording_mode": "push_to_talk",
    "auto_mode_threshold_ms": 500
  },
  "transcription": {
    "model": "small",
    "threads": 6
  },
  "injection": {
    "auto_submit": true
  },
  "feedback": {
    "audio_feedback": true,
    "start_sound_volume": 0.5,
    "stop_sound_volume": 0.5
  }
}
```

### Developer Configuration (Code Dictation)

```json
{
  "shortcuts": {
    "primary_shortcut": "SUPER+F5"
  },
  "transcription": {
    "model": "small",
    "threads": 8,
    "whisper_prompt": "Transcribe code and technical content with proper syntax, including variable names, function names, and programming constructs.",
    "word_overrides": {
      "open paren": "(",
      "close paren": ")",
      "open bracket": "[",
      "close bracket": "]",
      "open brace": "{",
      "close brace": "}",
      "new line": "\n",
      "tab": "\t",
      "colon": ":",
      "semicolon": ";",
      "equals": "=",
      "double quote": "\"",
      "single quote": "'",
      "backslash": "\\",
      "forward slash": "/",
      "less than": "<",
      "greater than": ">",
      "dot": ".",
      "comma": ",",
      "plus": "+",
      "minus": "-",
      "star": "*",
      "ampersand": "&",
      "pipe": "|",
      "dollar": "$",
      "hash": "#",
      "at": "@",
      "percent": "%",
      "caret": "^",
      "tilde": "~",
      "exclamation": "!",
      "question": "?",
      "underscore": "_"
    }
  },
  "injection": {
    "auto_submit": false,
    "paste_mode": "ctrl_shift"
  }
}
```

### Multi-Language Configuration

```json
{
  "transcription": {
    "model": "medium",
    "language": null,
    "word_overrides": {
      "period": ".",
      "comma": ",",
      "question mark": "?",
      "exclamation mark": "!",
      "colon": ":",
      "semicolon": ";"
    }
  }
}
```

Note: Set `language: null` for auto-detection, or specify a single language for better accuracy.

## CLI Configuration Commands

### Initialize Default Config

```bash
whisper-talk config init
```

Creates default configuration file if it doesn't exist.

### Get Configuration Value

```bash
whisper-talk config get <key>

# Examples:
whisper-talk config get shortcuts.primary_shortcut
whisper-talk config get transcription.model
whisper-talk config get feedback.audio_feedback
```

### Set Configuration Value

```bash
whisper-talk config set <key> <value>

# Examples:
whisper-talk config set transcription.model small
whisper-talk config set shortcuts.primary_shortcut "CTRL+SPACE"
whisper-talk config set feedback.audio_feedback false
```

### List All Configuration

```bash
whisper-talk config list
```

Prints entire configuration as JSON.

### Edit Configuration

```bash
whisper-talk config edit
```

Opens configuration in default editor (uses $EDITOR or falls back to nano).

### Validate Configuration

```bash
whisper-talk config validate
```

Checks configuration for errors and prints warnings.

### Reset Configuration

```bash
whisper-talk config reset
```

Resets to default configuration (backs up old config as `config.json.backup`).

## Configuration Best Practices

1. **Start simple:** Use minimal config, add options as needed
2. **Test changes:** Modify one option at a time
3. **Back up config:** Before major changes, copy `config.json`
4. **Use setup wizard:** First-time users should run `whisper-talk setup`
5. **Monitor logs:** Check logs for configuration-related issues
6. **Validate regularly:** Run `whisper-talk validate` after changes
7. **Document customizations:** Comment in config file (use comments in JSON5)

For more information, see:
- [README.md](README.md) - General project information
- [INSTALL.md](INSTALL.md) - Installation and setup guide
- [BACKENDS.md](BACKENDS.md) - Backend-specific configuration
