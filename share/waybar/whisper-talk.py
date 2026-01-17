#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

STATE_DIR = Path.home() / ".local/state/whisper-talk"
RECORDING_STATUS_FILE = STATE_DIR / "recording_status"
AUDIO_LEVEL_FILE = STATE_DIR / "audio_level"


def read_file_content(file_path):
    try:
        with open(file_path, "r") as f:
            return f.read().strip()
    except (FileNotFoundError, IOError):
        return None


def main():
    recording_status = read_file_content(RECORDING_STATUS_FILE)
    audio_level = read_file_content(AUDIO_LEVEL_FILE)

    is_recording = recording_status == "recording"

    if is_recording:
        text = "🎙️ REC"
        tooltip = "Recording..."
        class_name = "recording"
    else:
        text = "🎙️"
        tooltip = "Ready to record"
        class_name = "idle"

    try:
        level = float(audio_level) if audio_level else 0.0
    except ValueError:
        level = 0.0

    output = {
        "text": text,
        "tooltip": tooltip,
        "class": class_name,
        "alt": "recording" if is_recording else "idle",
    }

    print(json.dumps(output))


if __name__ == "__main__":
    main()
