#!/bin/bash
set -euo pipefail

# ============================================================
# autoresearch.sh — rendering frame time benchmark
# ============================================================
# Builds the release binary and runs the --benchmark flag.
# The app opens a real window, feeds synthetic colored terminal
# content, renders 300 frames (after 30 warmup), and reports
# average frame time in microseconds.
# ============================================================

METRIC_NAME="frame_time_us"

# Build release binary (skip if already up to date)
~/.cargo/bin/cargo build --release 2>&1 | tail -5

# Run the benchmark — the app prints METRIC frame_time_us=<value> on stdout
OUTPUT=$(target/release/smooth_terminal --benchmark 2>/dev/null)

# Extract the METRIC line
METRIC_LINE=$(echo "$OUTPUT" | grep "^METRIC ")

if [ -z "$METRIC_LINE" ]; then
    echo "ERROR: No METRIC line found in output" >&2
    echo "Output was: $OUTPUT" >&2
    exit 1
fi

echo "$METRIC_LINE"
