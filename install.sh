#!/usr/bin/env bash
set -e

GITHUB_USER="Dilip-gif"
REPO_NAME="rust_media_downloader"
TUI_BIN="media-tui"
POPUP_BIN="media-popup"
INSTALL_DIR="$HOME/.local/bin"

echo "==============================================================="
echo " 🚀 Installing Rust Media Downloader ($TUI_BIN + $POPUP_BIN)"
echo "==============================================================="
mkdir -p "$INSTALL_DIR"

# 1. Install system dependencies (ffmpeg, curl, python3-pip, nodejs, wl-clipboard/xclip)
if command -v apt-get >/dev/null 2>&1; then
    echo "📦 Installing system packages (ffmpeg, curl, python3-pip, xclip)..."
    sudo apt-get update -qq
    sudo apt-get install -y -qq ffmpeg curl python3-pip nodejs xclip wl-clipboard
elif command -v pacman >/dev/null 2>&1; then
    sudo pacman -Sy --noconfirm ffmpeg curl python-pip nodejs xclip wl-clipboard
elif command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y ffmpeg curl python3-pip nodejs xclip wl-clipboard
elif command -v brew >/dev/null 2>&1; then
    brew install ffmpeg curl python node
fi

# 2. Install Python extraction & Chrome TLS impersonation engines
echo "🐍 Setting up yt-dlp, gallery-dl, and curl_cffi..."
pip3 install -U "yt-dlp[default,curl-cffi]" gallery-dl curl_cffi --break-system-packages 2>/dev/null || \
pip3 install -U "yt-dlp[default,curl-cffi]" gallery-dl curl_cffi --user

# 3. Download pre-compiled binaries from GitHub Releases
OS_TYPE="$(uname -s)"
if [ "$OS_TYPE" = "Darwin" ]; then
    TUI_ASSET="rust_media_downloader-macos-arm64"
    POPUP_ASSET="media_popup-macos-arm64"
else
    TUI_ASSET="rust_media_downloader-linux-x86_64"
    POPUP_ASSET="media_popup-linux-x86_64"
fi

BASE_DL="https://github.com/$GITHUB_USER/$REPO_NAME/releases/latest/download"
TUI_URL="$BASE_DL/$TUI_ASSET"
POPUP_URL="$BASE_DL/$POPUP_ASSET"

if curl --output /dev/null --silent --head --fail -L "$TUI_URL"; then
    echo "⬇️ Downloading prebuilt TUI binary ($TUI_BIN)..."
    curl -fsSL "$TUI_URL" -o "$INSTALL_DIR/$TUI_BIN"
    chmod +x "$INSTALL_DIR/$TUI_BIN"

    if curl --output /dev/null --silent --head --fail -L "$POPUP_URL"; then
        echo "⬇️ Downloading prebuilt Quick-Popup GUI ($POPUP_BIN)..."
        curl -fsSL "$POPUP_URL" -o "$INSTALL_DIR/$POPUP_BIN"
        chmod +x "$INSTALL_DIR/$POPUP_BIN"
    fi
else
    echo "⚙️ Building binaries from source with Cargo..."
    if ! command -v cargo >/dev/null 2>&1; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi
    TMP_DIR="$(mktemp -d)"
    git clone "https://github.com/$GITHUB_USER/$REPO_NAME.git" "$TMP_DIR"
    cd "$TMP_DIR"
    cargo build --release --bins
    cp target/release/rust_media_downloader "$INSTALL_DIR/$TUI_BIN"
    cp target/release/media_popup "$INSTALL_DIR/$POPUP_BIN"
    chmod +x "$INSTALL_DIR/$TUI_BIN" "$INSTALL_DIR/$POPUP_BIN"
    rm -rf "$TMP_DIR"
fi

# 4. Auto-bind Super+M shortcut on Ubuntu / GNOME Desktop!
if command -v gsettings >/dev/null 2>&1 && [ -f "$INSTALL_DIR/$POPUP_BIN" ]; then
    KEY_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/rust-media-popup/"
    EXISTING=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings 2>/dev/null || echo "")
    if [[ "$EXISTING" != *"$KEY_PATH"* ]]; then
        if [ "$EXISTING" = "@as []" ] || [ -z "$EXISTING" ]; then
            NEW_LIST="['$KEY_PATH']"
        else
            NEW_LIST="${EXISTING%]*}, '$KEY_PATH']"
        fi
        gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$NEW_LIST" 2>/dev/null || true
    fi
    gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KEY_PATH name "Quick Media Downloader Popup" 2>/dev/null || true
    gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KEY_PATH command "$INSTALL_DIR/$POPUP_BIN" 2>/dev/null || true
    gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KEY_PATH binding "<Super>m" 2>/dev/null || true
    echo "⌨️ Bound global shortcut [Super + M] to launch $POPUP_BIN!"
fi

# 5. Add ~/.local/bin to PATH if not already present
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    for rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
        if [ -f "$rc" ]; then
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$rc"
        fi
    done
    export PATH="$INSTALL_DIR:$PATH"
fi

echo ""
echo "✅ Installation Complete!"
echo "👉 Press [Super + M] anywhere for the Quick Popup GUI, or type '$TUI_BIN' in your terminal!"