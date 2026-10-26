$InstallDir = "$env:LOCALAPPDATA\Programs\MediaDownloader"
$DataDir = "$env:LOCALAPPDATA\MediaDownloader"
$ShortcutPath = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Quick Media Downloader.lnk"

Write-Host "🗑️ Uninstalling Rust Media Downloader (media-tui & media-popup)..." -ForegroundColor Yellow

if (Test-Path $ShortcutPath) {
    Remove-Item -Force $ShortcutPath
    Write-Host "✔ Removed Start Menu shortcut & hotkey" -ForegroundColor Green
}
if (Test-Path $InstallDir) {
    Remove-Item -Recurse -Force $InstallDir
    Write-Host "✔ Removed binary folder: $InstallDir" -ForegroundColor Green
}
if (Test-Path $DataDir) {
    Remove-Item -Recurse -Force $DataDir
    Write-Host "✔ Removed app data & history: $DataDir" -ForegroundColor Green
}

Write-Host "✅ Uninstallation complete! (Your downloaded media in Downloads\MediaDownloader was kept safe)." -ForegroundColor Cyan