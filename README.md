# 🚀 Rust Media Downloader (`media-tui`)

An ultra-fast, asynchronous Terminal UI (TUI) Media Downloader and Multimodal Ingestion Engine built with **Rust**, **Tokio**, and **Ratatui**. Supports **2,000+ websites** including YouTube, Instagram, Reddit, Pinterest, X (Twitter), and TikTok.

---

## ⚡ One-Command Installation

Automatically installs all required dependencies (`yt-dlp`, `gallery-dl`, `ffmpeg`, `curl_cffi`) and sets up the global `media-tui` command:

### 🐧 Linux & 🍎 macOS (Terminal)
```bash
curl -fsSL [https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/install.sh](https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/install.sh) | bash
```

### 🪟 Windows (PowerShell)
```powershell
irm [https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/install.ps1](https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/install.ps1) | iex
```

### 🚀 Launch Anywhere
Once installed, simply type this in any terminal window:
```bash
media-tui
```

---

## 🗑️ One-Command Uninstall

Cleanly removes the `media-tui` binary and history cache while keeping your downloaded media files safe:

**Linux & macOS:**
```bash
curl -fsSL [https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/uninstall.sh](https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/uninstall.sh) | bash
```

**Windows (PowerShell):**
```powershell
irm [https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/uninstall.ps1](https://raw.githubusercontent.com/Dilip-gif/rust_media_downloader/main/uninstall.ps1) | iex
```

---

## ✨ Key Engineering Features

* **⚡ 4-Worker Async Concurrency (`Tokio`):** Non-blocking metadata inspection and parallel downloads with real-time smoothed speed (`EMA`) and ETA tracking.
* **🛡️ Smart On-Demand Authentication (`[F8]`):** Runs in safe anonymous Guest Mode by default; automatically escalates through `cookies.txt` → `Chrome` → `Brave` → `Firefox` → `Edge` only when hitting age-gates or login walls.
* **🌐 Headless Browser DOM & TLS Impersonation Engine:** Bypasses network/API firewalls on Reddit and social platforms using Chrome JA3/JA4 TLS impersonation (`curl_cffi`), `--headless=new --dump-dom` extraction, and binary magic-byte image verification.
* **🧠 Smart Input Shield (`BracketedPaste`):** Automatically rejects accidental non-URL code pastes and extracts clean URLs from mixed text.
* **🗂️ Persistent Dual History Manager (`[F4]`):** Browser-style manageable history for both **Completed Downloads** and **Search/Link History** across restarts.
* **🧹 Zero-Garbage Cleanup:** Canceling (`[F6]`) or nuking (`[F7]`) a download immediately sweeps leftover `.part`, `.ytdl`, `.webp`, and `.srt` fragments.

---

## ⌨️ Keyboard Shortcuts

| Key | Action |
| :--- | :--- |
| **`Enter`** | Add URL(s) to queue / Inspect selected item / Confirm quality preset |
| **`F4` / `F12`** | Open Persistent Dual History Manager (`Tab` to switch views) |
| **`F5`** | Pause / Resume active download |
| **`F6`** | Skip & cancel current download (auto-cleans temp files) |
| **`F7`** | Nuke entire queue & terminate all background workers |
| **`F8`** | Open Smart Authentication Engine Switcher |
| **`F9`** | Open selected item's save folder in native OS File Explorer |
| **`Ctrl + U`** | Clear input box |
| **`Del`** | Remove item from queue or history |
| **`Esc`** | Close modal / Exit application |