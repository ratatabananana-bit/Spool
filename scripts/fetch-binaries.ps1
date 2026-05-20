# scripts/fetch-binaries.ps1
# Pulls the sidecar binaries (yt-dlp + ffmpeg + ffprobe) into src-tauri/binaries/
# named with the Tauri target-triple suffix so `tauri build` picks them up.
#
# Run from the repo root:
#   powershell -ExecutionPolicy Bypass -File scripts\fetch-binaries.ps1

$ErrorActionPreference = 'Stop'
$ProgressPreference    = 'SilentlyContinue'

$root      = Split-Path -Parent $PSScriptRoot
$binDir    = Join-Path $root "src-tauri\binaries"
$ytDlpPath = Join-Path $binDir "yt-dlp-x86_64-pc-windows-msvc.exe"
$ffmpegPath = Join-Path $binDir "ffmpeg-x86_64-pc-windows-msvc.exe"
$ffprobePath = Join-Path $binDir "ffprobe-x86_64-pc-windows-msvc.exe"

New-Item -ItemType Directory -Force -Path $binDir | Out-Null

# yt-dlp — single exe, latest release
if (Test-Path $ytDlpPath) {
    Write-Host "[skip] $ytDlpPath already exists"
} else {
    Write-Host "[get ] yt-dlp.exe (latest release)"
    Invoke-WebRequest `
        -Uri 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe' `
        -OutFile $ytDlpPath
}

# ffmpeg + ffprobe — Gyan's essentials build, latest
if ((Test-Path $ffmpegPath) -and (Test-Path $ffprobePath)) {
    Write-Host "[skip] ffmpeg.exe and ffprobe.exe already exist"
} else {
    $tmp = Join-Path $env:TEMP "spool-ffmpeg-$(Get-Random).zip"
    $extractDir = Join-Path $env:TEMP "spool-ffmpeg-extract-$(Get-Random)"
    Write-Host "[get ] ffmpeg essentials build (latest from gyan.dev)"
    $release = Invoke-RestMethod 'https://api.github.com/repos/GyanD/codexffmpeg/releases/latest'
    $asset = $release.assets | Where-Object { $_.name -match 'essentials_build\.zip$' } | Select-Object -First 1
    if (-not $asset) { throw "could not find essentials_build.zip on Gyan's latest release" }
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $tmp
    Expand-Archive -Path $tmp -DestinationPath $extractDir -Force
    $ff  = Get-ChildItem $extractDir -Recurse -Filter 'ffmpeg.exe'  | Select-Object -First 1
    $fp  = Get-ChildItem $extractDir -Recurse -Filter 'ffprobe.exe' | Select-Object -First 1
    Copy-Item $ff.FullName $ffmpegPath  -Force
    Copy-Item $fp.FullName $ffprobePath -Force
    Remove-Item $tmp -Force
    Remove-Item $extractDir -Recurse -Force
}

Write-Host ""
Write-Host "binaries ready:" -ForegroundColor Green
Get-ChildItem $binDir | Select-Object Name, @{N='MB';E={[math]::Round($_.Length/1MB, 2)}} | Format-Table -AutoSize
