# Opens a GUI executable, captures its window to a PNG, and closes it.
# Usage: powershell -File tools/capture-window.ps1 -Exe target/debug/astryx-gallery.exe -Out shot.png
#   [-WaitSeconds 6] [-EnvName GALLERY_THEME -EnvValue butter]

param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$WaitSeconds = 6,
    [string]$EnvName = "",
    [string]$EnvValue = ""
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Win {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[Win]::SetProcessDPIAware() | Out-Null

if ($EnvName -ne "") { [Environment]::SetEnvironmentVariable($EnvName, $EnvValue, "Process") }
$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds $WaitSeconds
$p.Refresh()
if ($p.HasExited) { Write-Output "EXITED code=$($p.ExitCode)"; exit 1 }
$h = $p.MainWindowHandle
[Win]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 800
$r = New-Object Win+RECT
[Win]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $ht
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Stop-Process -Id $p.Id -Confirm:$false
Write-Output "captured ${w}x${ht} -> $Out"
