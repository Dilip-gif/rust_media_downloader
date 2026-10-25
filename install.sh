#!/usr/bin/env bash
set -e

GITHUB_USER="Dilip-gif"
REPO_NAME="rust_media_downloader"
BIN_NAME="media-tui"
INSTALL_DIR="$HOME/.local/bin"

echo "====================================================="
echo " 🚀 Installing Rust Media Downloader ($BIN_NAME) v1.0"
echo "====================================================="
mkdir -p "$INSTALL_DIR"

# 1. Install system dependencies (ffmpeg, curl, python3-pip, nodejs)
if command -v apt-get >/dev/null 2>&1; then
    echo "📦 Installing system packages (ffmpeg, curl, python3-pip, nodejs)..."
    sudo apt-get update -qq
    sudo apt-get install -y -qq ffmpeg curl python3-pip nodejs
elif command -v pacman >/dev/null 2>&1; then
    echo "📦 Installing system packages via pacman..."
    sudo pacman -Sy --noconfirm ffmpeg curl python-pip nodejs
elif command -v dnf >/dev/null 2>&1; then
    echo "📦 Installing system packages via dnf..."
    sudo dnf install -y ffmpeg curl python3-pip nodejs
elif command -v brew >/dev/null 2>&1; then
    echo "📦 Installing system packages via Homebrew..."
    brew install ffmpeg curl python node
fi

# 2. Install Python extraction & Chrome TLS impersonation engines
echo "🐍 Setting up yt-dlp, gallery-dl, and curl_cffi..."
pip3 install -U "yt-dlp[default,curl-cffi]" gallery-dl curl_cffi --break-system-packages 2>/dev/null || \
pip3 install -U "yt-dlp[default,curl-cffi]" gallery-dl curl_cffi --user

# 3. Download pre-compiled binary from GitHub Releases (with source build fallback)
OS_TYPE="$(uname -s)"
if [ "$OS_TYPE" = "Darwin" ]; then
    ASSET_NAME="rust_media_downloader-macos-arm64"
else
    ASSET_NAME="rust_media_downloader-linux-x86_64"
fi

LATEST_URL="https://github.com/$GITHUB_USER/$REPO_NAME/releases/latest/download/$ASSET_NAME"

if curl --output /dev/null --silent --head --fail -L "$LATEST_URL"; then
    echo "⬇️ Downloading prebuilt release binary ($ASSET_NAME)..."
    curl -fsSL "$LATEST_URL" -o "$INSTALL_DIR/$BIN_NAME"
    chmod +x "$INSTALL_DIR/$BIN_NAME"
else
    echo "⚙️ Prebuilt release not found yet; building from source with Cargo..."
    if ! command -v cargo >/dev/null 2>&1; then
        echo "🦀 Installing Rust toolchain..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi
    TMP_DIR="$(mktemp -d)"
    git clone "https://github.com/$GITHUB_USER/$REPO_NAME.git" "$TMP_DIR"
    cd "$TMP_DIR"
    cargo build --release
    cp target/release/rust_media_downloader "$INSTALL_DIR/$BIN_NAME"
    chmod +x "$INSTALL_DIR/$BIN_NAME"
    rm -rf "$TMP_DIR"
fi

# 4. Add ~/.local/bin to PATH if not already present
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
echo "👉 Run '$BIN_NAME' from any terminal (or restart your terminal first)."