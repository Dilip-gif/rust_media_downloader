$ErrorActionPreference = "Stop"
$GithubUser = "Dilip-gif"
$RepoName = "rust_media_downloader"
$InstallDir = "$env:LOCALAPPDATA\Programs\MediaDownloader"

Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host " 🚀 Installing Rust Media Downloader (media-tui + media-popup)" -ForegroundColor Cyan
Write-Host "=================================================================" -ForegroundColor Cyan

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

# 1. Install FFmpeg and Python via Winget if missing
if (!(Get-Command ffmpeg -ErrorAction SilentlyContinue)) {
    Write-Host "📦 Installing FFmpeg via Winget..." -ForegroundColor Yellow
    winget install --id Gyan.FFmpeg -e --silent --accept-package-agreements --accept-source-agreements
}
if (!(Get-Command python -ErrorAction SilentlyContinue)) {
    Write-Host "📦 Installing Python3 via Winget..." -ForegroundColor Yellow
    winget install --id Python.Python.3.12 -e --silent --accept-package-agreements --accept-source-agreements
}

# 2. Install yt-dlp, gallery-dl, and curl_cffi
Write-Host "🐍 Setting up yt-dlp, gallery-dl, and curl_cffi..." -ForegroundColor Yellow
pip install -U "yt-dlp[default,curl-cffi]" gallery-dl curl_cffi

# 3. Download prebuilt Windows binaries (TUI + Quick-Popup GUI)
$BaseDl = "https://github.com/$GithubUser/$RepoName/releases/latest/download"
$TuiUrl = "$BaseDl/rust_media_downloader-windows-x86_64.exe"
$PopupUrl = "$BaseDl/media_popup-windows-x86_64.exe"
$TuiPath = "$InstallDir\media-tui.exe"
$PopupPath = "$InstallDir\media-popup.exe"

Write-Host "⬇️ Downloading prebuilt TUI binary (media-tui.exe)..." -ForegroundColor Yellow
Invoke-WebRequest -Uri $TuiUrl -OutFile $TuiPath

Write-Host "⬇️ Downloading prebuilt Quick-Popup GUI (media-popup.exe)..." -ForegroundColor Yellow
Invoke-WebRequest -Uri $PopupUrl -OutFile $PopupPath

# 4. Bind Global Shortcut (Ctrl + Alt + M) via Windows Start Menu Shortcut
$StartMenuDir = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs"
$ShortcutPath = "$StartMenuDir\Quick Media Downloader.lnk"
$WshShell = New-Object -ComObject WScript.Shell
$Shortcut = $WshShell.CreateShortcut($ShortcutPath)
$Shortcut.TargetPath = $PopupPath
$Shortcut.WorkingDirectory = $InstallDir
$Shortcut.Hotkey = "CTRL+ALT+M"
$Shortcut.Description = "Launch Quick Media Downloader Popup"
$Shortcut.Save()
Write-Host "⌨️ Bound global shortcut [Ctrl + Alt + M] to launch media-popup!" -ForegroundColor Green

# 5. Add to User PATH if not already present
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path += ";$InstallDir"
}

Write-Host ""
Write-Host "✅ Installation Complete!" -ForegroundColor Green
Write-Host "👉 Press [Ctrl + Alt + M] anywhere for the Quick Popup, or type 'media-tui' / 'media-popup' in terminal!" -ForegroundColor Cyan