# UI test: click (or scroll the mouse wheel) at a point given in screenshot pixels, in the running
# Fizz control window, then screenshot. Refuses to act unless the app is the foreground window.
# Usage: click.ps1 -X 907 -Y 434 -Out shot.png   or   click.ps1 -X 760 -Y 600 -Scroll -1500 -Out shot.png
param([int]$X, [int]$Y, [int]$Scroll = 0, [string]$Out)

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Mouse {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, int data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
}
"@

$proc = Get-Process fizz-control -ErrorAction Stop | Select-Object -First 1
$h = $proc.MainWindowHandle
$r = New-Object Mouse+RECT
[void][Mouse]::DwmGetWindowAttribute($h, 9, [ref]$r, 16)

$sb = New-Object System.Text.StringBuilder 256
[void][Mouse]::GetWindowText([Mouse]::GetForegroundWindow(), $sb, 256)
if ($sb.ToString() -ne 'Fizz control') { Write-Output "ABORT: foreground is '$($sb.ToString())'"; return }

[void][Mouse]::SetCursorPos($r.L + $X, $r.T + $Y)
Start-Sleep -Milliseconds 200
if ($Scroll -ne 0) {
  [Mouse]::mouse_event(0x0800, 0, 0, $Scroll, [UIntPtr]::Zero)   # wheel
} else {
  [Mouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)          # left down
  Start-Sleep -Milliseconds 80
  [Mouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)          # left up
}
Start-Sleep -Milliseconds 700
& 'D:\Projects\keyboard-control\tools\screenshot.ps1' -Title 'Fizz control' -Out $Out
