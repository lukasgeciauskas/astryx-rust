# Shared by capture-window.ps1 and capture-running.ps1: finds a process's main
# window and captures it with PrintWindow (PW_RENDERFULLCONTENT), which reads
# the window's own content even when another window covers it. Never copies
# the screen, so other apps' windows are never captured.

Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing @"
using System;
using System.Drawing;
using System.Runtime.InteropServices;
public static class AstryxCapture {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc proc, IntPtr lParam);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr hWnd, System.Text.StringBuilder name, int max);

    // The process's largest visible top-level window: Windows can report a
    // small helper window (IME, tooltip) as the "main" one.
    public static IntPtr LargestWindow(uint pid) {
        IntPtr best = IntPtr.Zero;
        long bestArea = 0;
        EnumWindows((hWnd, _) => {
            uint owner;
            GetWindowThreadProcessId(hWnd, out owner);
            if (owner != pid || !IsWindowVisible(hWnd)) return true;
            // A console-subsystem binary's console window is not the app.
            var name = new System.Text.StringBuilder(64);
            GetClassName(hWnd, name, name.Capacity);
            if (name.ToString() == "ConsoleWindowClass") return true;
            RECT r;
            GetWindowRect(hWnd, out r);
            long area = (long)(r.Right - r.Left) * (r.Bottom - r.Top);
            if (area > bestArea) { bestArea = area; best = hWnd; }
            return true;
        }, IntPtr.Zero);
        return best;
    }

    public static string Capture(IntPtr hWnd, string path) {
        RECT r;
        GetWindowRect(hWnd, out r);
        int w = r.Right - r.Left, h = r.Bottom - r.Top;
        using (var bmp = new Bitmap(w, h)) {
            using (var g = Graphics.FromImage(bmp)) {
                IntPtr hdc = g.GetHdc();
                bool ok = PrintWindow(hWnd, hdc, 2);
                g.ReleaseHdc(hdc);
                if (!ok) return null;
            }
            bmp.Save(path, System.Drawing.Imaging.ImageFormat.Png);
        }
        return w + "x" + h;
    }
}
"@
[AstryxCapture]::SetProcessDPIAware() | Out-Null

# Waits up to $Seconds for a visible window of process $ProcessId.
function Wait-AstryxWindow([int]$ProcessId, [double]$Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    do {
        $hwnd = [AstryxCapture]::LargestWindow([uint32]$ProcessId)
        if ($hwnd -ne [IntPtr]::Zero) { return $hwnd }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $deadline)
    return [IntPtr]::Zero
}
