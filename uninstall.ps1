$InstallDir = "$env:LOCALAPPDATA\Programs\MediaDownloader"
$DataDir = "$env:LOCALAPPDATA\MediaDownloader"

Write-Host "🗑️ Uninstalling Rust Media Downloader (media-tui)..." -ForegroundColor Yellow

if (Test-Path $InstallDir) {
    Remove-Item -Recurse -Force $InstallDir
    Write-Host "✔ Removed binary folder: $InstallDir" -ForegroundColor Green
}
if (Test-Path $DataDir) {
    Remove-Item -Recurse -Force $DataDir
    Write-Host "✔ Removed app data & history: $DataDir" -ForegroundColor Green
}

Write-Host "✅ Uninstallation complete! (Your downloaded media in Downloads\MediaDownloader was kept safe)." -ForegroundColor Cyan