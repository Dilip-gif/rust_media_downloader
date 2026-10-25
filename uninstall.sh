#!/usr/bin/env bash
set -e

BIN_NAME="media-tui"
INSTALL_DIR="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/MediaDownloader"

echo "🗑️ Uninstalling Rust Media Downloader ($BIN_NAME)..."

if [ -f "$INSTALL_DIR/$BIN_NAME" ]; then
    rm -f "$INSTALL_DIR/$BIN_NAME"
    echo "✔ Removed binary: $INSTALL_DIR/$BIN_NAME"
fi

if [ -d "$DATA_DIR" ]; then
    rm -rf "$DATA_DIR"
    echo "✔ Removed app data & history: $DATA_DIR"
fi

echo "✅ Uninstallation complete! (Your downloaded media in ~/Downloads/MediaDownloader was kept safe)."