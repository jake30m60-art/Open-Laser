# SPDX-License-Identifier: GPL-3.0-or-later
# Installs a freshly built OpenLaser over the one in the app folder.
#
#   powershell -ExecutionPolicy Bypass -File scripts\update-windows.ps1 [-Build] [-Start Maschine|Simulator|Nein]
#
# -Build builds the interface and the release first. The running OpenLaser
# is asked to close (never killed); the old exe moves to alt\ with a date,
# the new one is copied, compared by hash and started like the desktop
# shortcut starts it.
param(
    [switch]$Build,
    [ValidateSet('Maschine', 'Simulator', 'Nein')][string]$Start = 'Maschine',
    [string]$App = "$env:USERPROFILE\OpenLaser-App"
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

if ($Build) {
    Write-Host '== Oberfläche bauen'
    Push-Location "$repo\ui"
    try { npm ci; if ($LASTEXITCODE) { throw 'npm ci fehlgeschlagen' }; npm run build; if ($LASTEXITCODE) { throw 'Oberfläche bauen fehlgeschlagen' } }
    finally { Pop-Location }
    Write-Host '== Programm bauen'
    cargo build --release -p openlaser --features desktop --locked --manifest-path "$repo\Cargo.toml" --target-dir "$repo\target\neu"
    if ($LASTEXITCODE) { throw 'cargo build fehlgeschlagen' }
}

# The newest release build wins.
$candidates = @("$repo\target\neu\release\openlaser.exe", "$repo\target\release\openlaser.exe") | Where-Object { Test-Path $_ }
if (-not $candidates) { throw 'Kein gebautes openlaser.exe gefunden – mit -Build aufrufen.' }
$source = $candidates | Get-Item | Sort-Object LastWriteTime -Descending | Select-Object -First 1
$target = Join-Path $App 'OpenLaser.exe'
Write-Host "Neu:  $($source.FullName) ($($source.LastWriteTime))"

if ((Test-Path $target) -and (Get-FileHash $target).Hash -eq (Get-FileHash $source.FullName).Hash) {
    Write-Host 'Diese Version ist schon installiert.'
} else {
    $running = Get-Process openlaser -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $target }
    if ($running) {
        Write-Host 'OpenLaser läuft noch und wird gebeten, sich zu schließen …'
        $running | ForEach-Object { [void]$_.CloseMainWindow() }
        $deadline = (Get-Date).AddSeconds(30)
        while ((Get-Process -Id $running.Id -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
        if (Get-Process -Id $running.Id -ErrorAction SilentlyContinue) {
            Read-Host 'Bitte OpenLaser jetzt selbst beenden (Maschine vorher in Ruhe) und dann Enter drücken'
            if (Get-Process -Id $running.Id -ErrorAction SilentlyContinue) { throw 'OpenLaser läuft noch – nichts geändert.' }
        }
    }
    $old = Join-Path $App 'alt'
    New-Item -ItemType Directory -Force $old | Out-Null
    if (Test-Path $target) {
        $backup = Join-Path $old ("OpenLaser-{0:yyyyMMdd-HHmm}.exe" -f (Get-Item $target).LastWriteTime)
        Move-Item $target $backup -Force
        Write-Host "Alte Version gesichert: $backup"
    }
    Copy-Item $source.FullName $target
    if ((Get-FileHash $target).Hash -ne (Get-FileHash $source.FullName).Hash) { throw 'Kopie weicht ab – bitte erneut versuchen.' }
    Write-Host 'Neue Version installiert.'
}

switch ($Start) {
    'Maschine' { Start-Process $target -ArgumentList '--config', 'machine.toml' -WorkingDirectory $App }
    'Simulator' { Start-Process $target -ArgumentList '--simulate', '--config', 'simulator\machine.toml' -WorkingDirectory $App }
}
