
# 🚀 Rust Media Downloader (\`media-tui\` & \`media-popup\`)

An ultra-fast, cross-platform Media Downloader suite built in **Rust** featuring two interfaces:
1. **⚡ \`media-popup\` (OS-Adaptive Quick-Popup GUI):** Summon anywhere with **\`Super + M\`** (Linux) or **\`Ctrl + Alt + M\`** (Windows) to auto-detect copied links, inspect exact available video resolutions (\`8K\`, \`4K\`, \`2K\`, \`1080p\` default, \`720p\`, \`480p\`, \`MP3\`, \`Gallery\`) with estimated file sizes, and download in one click.
2. **🖥️ \`media-tui\` (4-Worker Terminal UI):** Full-featured asynchronous terminal queue manager built with **Tokio** and **Ratatui** with persistent history (\`[F4]\`), smart browser authentication (\`[F8]\`), and built-in auto-updater (\`[F3]\`).

Supports **2,000+ websites** including YouTube, Instagram, Reddit, Pinterest, X (Twitter), and TikTok.

---

## ⚡ One-Command Installation

Automatically installs all required dependencies (\`yt-dlp\`, \`gallery-dl\`, \`ffmpeg\`, \`curl_cffi\`), installs both \`media-tui\` and \`media-popup\`, and registers the global popup shortcut:

### 🐧 Linux & 🍎 macOS (Terminal)
```
curl -fsSL https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/install.sh | bash
```

### 🪟 Windows (PowerShell)
```
irm https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/install.ps1 | iex
```


---

## 🚀 How to Launch

* **Quick-Popup GUI:** Press **\`Super + M\`** (Linux), **\`Ctrl + Alt + M\`** (Windows), or run:
```
media-popup
 ```
* **Full Terminal UI (4-Worker Queue & History Manager):**
```
media-tui
```

---

## 🗑️ One-Command Uninstall

Cleanly removes \`media-tui\`, \`media-popup\`, shortcuts, and history cache while keeping your downloaded media files safe:

**Linux & macOS:**
```
curl -fsSL https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/uninstall.sh | bash
```

**Windows (PowerShell):**
```
irm https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/uninstall.ps1 | iex
```

---

## ✨ Key Engineering Features

* **🎨 OS-Adaptive Native Themes:** Automatically adapts to **Ubuntu GNOME Libadwaita Dark** (Lavender), **Windows 11 Fluent Dark** (Azure Blue), and **macOS Cupertino Dark** (System Blue).
* **🔍 Dynamic Resolution & File-Size Inspector:** Inspects links before downloading to show every exact available resolution (\`4320p 8K\` down to \`144p\`, \`Audio MP3\`, and \`Original Gallery\`) with estimated MB sizes and \`1080p Full HD\` pre-selected by default.
* **🔄 Built-In GitHub Auto-Updater:** Automatically checks GitHub Releases on launch and offers a 1-click update pill in \`media-popup\` and \`[F3]\` self-updater in \`media-tui\`.
* **⚡ 4-Worker Async Concurrency (\`media-tui\`):** Non-blocking metadata inspection and parallel downloads with real-time smoothed speed (\`EMA\`) and ETA tracking.
* **🛡️ Smart On-Demand Authentication (\`[F8]\`):** Runs in safe anonymous Guest Mode by default; automatically escalates through \`cookies.txt\` → \`Chrome\` → \`Brave\` → \`Firefox\` → \`Edge\` only when hitting age-gates or login walls.
* **🌐 Headless Browser DOM & TLS Impersonation Engine:** Bypasses network/API firewalls on Reddit and social platforms using Chrome JA3/JA4 TLS impersonation (\`curl_cffi\`), \`--headless=new --dump-dom\` extraction, and binary magic-byte image verification.
* **🗂️ Shared Persistent Dual History (\`[F4]\`):** Downloads from both \`media-popup\` and \`media-tui\` are logged into a shared history database across restarts.
* **🧹 Zero-Garbage Cleanup:** Skipping or canceling immediately sweeps leftover \`.part\`, \`.ytdl\`, \`.f137.mp4\`, \`.webp\`, and \`.srt\` fragments.

---

## ⌨️ Keyboard Shortcuts

### Quick-Popup GUI (\`media-popup\`)
| Key | Action |
| :--- | :--- |
| **\`Super + M\` / \`Ctrl + Alt + M\`** | Summon Quick-Popup GUI anywhere |
| **\`Enter\`** | Inspect link / Confirm selected quality preset |
| **\`↑ / ↓\`** | Choose video resolution or Audio MP3 in Quality Picker |
| **\`Spacebar\`** | Pause / Resume active download |
| **\`Esc\`** | Go back / Cancel download (auto-cleans temp files) & close popup |

### Terminal UI (\`media-tui\`)
| Key | Action |
| :--- | :--- |
| **\`Enter\`** | Add URL(s) to queue / Inspect selected item / Confirm quality preset |
| **\`F3\`** | Auto-Install new update (when update banner is shown) |
| **\`F4\` / \`F12\`** | Open Persistent Dual History Manager (\`Tab\` to switch views) |
| **\`F5\`** | Pause / Resume highlighted download |
| **\`F6\`** | Skip & cancel highlighted download (auto-cleans temp files) |
| **\`F7\`** | Nuke entire queue & terminate all background workers |
| **\`F8\`** | Open Smart Authentication Engine Switcher |
| **\`F9\`** | Open selected item's save folder in native OS File Explorer |
| **\`Ctrl + U\`** | Clear input box |
| **\`Del\`** | Remove item from queue or history |
| **\`Esc\`** | Close modal / Exit application |
EOF