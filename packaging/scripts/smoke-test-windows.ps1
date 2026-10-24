# Installs Cantara with one of the Windows installers, starts it, checks that it
# is still running afterwards with its WebView2 window up, and uninstalls it.
#
#   packaging/scripts/smoke-test-windows.ps1 <installer.exe> <output dir>
#
# Silent (`/S`), into the per-user location the installer uses by default
# (`install_mode = "CurrentUser"` in Dioxus.toml). Uninstalls at the end, so
# that the next installer is tested on a machine without Cantara.
param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$OutDir
)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$installDir = Join-Path $env:LOCALAPPDATA "Programs\Cantara"
$exe = Join-Path $installDir "cantara.exe"

Write-Host "Installing $Installer"
$setup = Start-Process -FilePath $Installer -ArgumentList "/S" -Wait -PassThru
if ($setup.ExitCode -ne 0) {
    throw "The installer exited with $($setup.ExitCode)."
}
if (-not (Test-Path $exe)) {
    throw "The installer reported success, but $exe is not there."
}

# Whatever WebView2 processes were running before, so that only the ones
# Cantara starts count.
$before = @(Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })

Write-Host "Starting $exe"
$app = Start-Process -FilePath $exe -PassThru `
    -RedirectStandardOutput (Join-Path $OutDir "stdout.log") `
    -RedirectStandardError (Join-Path $OutDir "stderr.log")
Start-Sleep -Seconds 25

$failed = $false
if ($app.HasExited) {
    Write-Host "::error::Cantara exited (code $($app.ExitCode)) within 25 seconds of being started."
    $failed = $true
} else {
    $webviews = @(Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Id })
    if ($webviews.Count -eq 0) {
        Write-Host "::error::Cantara is running, but WebView2 never started: its window has nothing in it."
        $failed = $true
    } else {
        Write-Host "Cantara is running, with $($webviews.Count) WebView2 processes."
    }
}

if (-not $app.HasExited) {
    # The WebView2 processes go with it.
    & taskkill /PID $app.Id /T /F | Out-Null
}
Start-Sleep -Seconds 3

# In place (`_?=`), so that the uninstaller does not return before it is done.
$uninstaller = Join-Path $installDir "uninstall.exe"
if (Test-Path $uninstaller) {
    Start-Process -FilePath $uninstaller -ArgumentList "/S", "_?=$installDir" -Wait | Out-Null
}
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $installDir

if ($failed) {
    Get-Content -ErrorAction SilentlyContinue (Join-Path $OutDir "stderr.log") | Select-Object -Last 100
    exit 1
}
