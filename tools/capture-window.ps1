# Opens a GUI executable, captures its window to a PNG, and closes it.
# Usage: powershell -File tools/capture-window.ps1 -Exe target/debug/rustryx-gallery.exe -Out shot.png
#   [-WaitSeconds 6] [-EnvName GALLERY_THEME -EnvValue butter]
#
# The capture reads the window's own content (see capture-common.ps1), so it
# works even if the window opens behind others.

param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$WaitSeconds = 6,
    [string]$EnvName = "",
    [string]$EnvValue = ""
)

. (Join-Path $PSScriptRoot "capture-common.ps1")

if ($EnvName -ne "") { [Environment]::SetEnvironmentVariable($EnvName, $EnvValue, "Process") }
$p = Start-Process -FilePath $Exe -PassThru
$hwnd = Wait-RustryxWindow $p.Id $WaitSeconds
if ($hwnd -eq [IntPtr]::Zero) {
    $p.Refresh()
    if ($p.HasExited) { Write-Output "EXITED code=$($p.ExitCode)" } else { Write-Output "no window after ${WaitSeconds}s" }
    Stop-Process -Id $p.Id -Confirm:$false -ErrorAction SilentlyContinue
    exit 1
}
# Let the first frames settle (fonts, images, restored scroll), then take the
# window again in case a larger one appeared.
Start-Sleep -Seconds 2
$hwnd = Wait-RustryxWindow $p.Id 1
$size = [RustryxCapture]::Capture($hwnd, (Resolve-Path -LiteralPath (Split-Path -Parent $Out)).Path + "\" + (Split-Path -Leaf $Out))
Stop-Process -Id $p.Id -Confirm:$false
if (-not $size) { Write-Output "PrintWindow failed"; exit 1 }
Write-Output "captured $size -> $Out"
