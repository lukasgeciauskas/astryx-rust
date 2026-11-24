# Captures the window of an already running process to a PNG.
# Usage: powershell -File tools/capture-running.ps1 -Name astryx-gallery -Out shot.png
#
# The capture reads the window's own content (see capture-common.ps1), so it
# works even if the window is behind others.

param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][string]$Out
)

. (Join-Path $PSScriptRoot "capture-common.ps1")

$p = Get-Process -Name $Name -ErrorAction SilentlyContinue | Sort-Object StartTime -Descending | Select-Object -First 1
if (-not $p) { Write-Output "no process $Name"; exit 1 }
$hwnd = [AstryxCapture]::LargestWindow([uint32]$p.Id)
if ($hwnd -eq [IntPtr]::Zero) { Write-Output "no visible window for process $Name"; exit 1 }
$size = [AstryxCapture]::Capture($hwnd, (Resolve-Path -LiteralPath (Split-Path -Parent $Out)).Path + "\" + (Split-Path -Leaf $Out))
if (-not $size) { Write-Output "PrintWindow failed"; exit 1 }
Write-Output "captured $size -> $Out"
