$ErrorActionPreference = "Stop"
$GithubUser = "Dilip-gif"
$RepoName = "rust_media_downloader"
$InstallDir = "$env:LOCALAPPDATA\Programs\MediaDownloader"

Write-Host "=====================================================" -ForegroundColor Cyan
Write-Host " 🚀 Installing Rust Media Downloader (media-tui) v1.0" -ForegroundColor Cyan
Write-Host "=====================================================" -ForegroundColor Cyan

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

# 3. Download prebuilt Windows .exe binary from GitHub Releases
$AssetUrl = "https://github.com/$GithubUser/$RepoName/releases/latest/download/rust_media_downloader-windows-x86_64.exe"
$ExePath = "$InstallDir\media-tui.exe"

Write-Host "⬇️ Downloading prebuilt Windows binary..." -ForegroundColor Yellow
Invoke-WebRequest -Uri $AssetUrl -OutFile $ExePath

# 4. Add to User PATH if not already present
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path += ";$InstallDir"
}

Write-Host ""
Write-Host "✅ Installation Complete!" -ForegroundColor Green
Write-Host "👉 Open a new terminal and type 'media-tui' to launch!" -ForegroundColor Cyan