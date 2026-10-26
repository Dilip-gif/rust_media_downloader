#!/usr/bin/env bash
set -e

INSTALL_DIR="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/MediaDownloader"

echo "🗑️ Uninstalling Rust Media Downloader (media-tui & media-popup)..."

rm -f "$INSTALL_DIR/media-tui" "$INSTALL_DIR/media-popup"
echo "✔ Removed binaries from $INSTALL_DIR"

if [ -d "$DATA_DIR" ]; then
    rm -rf "$DATA_DIR"
    echo "✔ Removed app data & history: $DATA_DIR"
fi

echo "✅ Uninstallation complete! (Your downloaded media in ~/Downloads/MediaDownloader was kept safe)."