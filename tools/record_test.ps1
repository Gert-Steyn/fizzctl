# UI test: launch the app with a recording running (FIZZ_RECORD), bring it to the front, send real
# key presses with SendKeys and screenshot the result. Refuses to type unless "Fizz control" is the
# foreground window. Set $env:FIZZ_KEEP = 1 to leave the app open for tools/click.ps1.
# Note: SendKeys arrows lack the extended-key flag, so Windows sees them as numpad 4/6/8/2.
param([string]$Out, [string]$Theme = 'dark')

Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Win {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hgt, bool repaint);
}
"@

function Get-ForegroundTitle {
  $sb = New-Object System.Text.StringBuilder 256
  [void][Win]::GetWindowText([Win]::GetForegroundWindow(), $sb, 256)
  return $sb.ToString()
}

$env:FIZZ_PAGE = 'macros'
$env:FIZZ_THEME = $Theme
$env:FIZZ_RECORD = '1'
$exe = 'D:\Projects\keyboard-control\target\debug\fizz-control.exe'
$proc = Start-Process -FilePath $exe -WorkingDirectory 'D:\Projects\keyboard-control' -PassThru
Remove-Item Env:FIZZ_PAGE, Env:FIZZ_THEME, Env:FIZZ_RECORD
Start-Sleep -Seconds 9

try {
  $proc.Refresh()
  $h = $proc.MainWindowHandle
  [void][Win]::MoveWindow($h, 20, 20, 1000, 1040, $true)
  Start-Sleep -Milliseconds 600
  $shell = New-Object -ComObject WScript.Shell
  [void]$shell.AppActivate($proc.Id)
  [void][Win]::SetForegroundWindow($h)
  Start-Sleep -Milliseconds 700
  $title = Get-ForegroundTitle
  if ($title -ne 'Fizz control') {
    Write-Output "ABORT: foreground window is '$title', not sending any keys"
    return
  }
  Write-Output "foreground ok: $title"

  $send = { param($keys, $pause) [System.Windows.Forms.SendKeys]::SendWait($keys); Start-Sleep -Milliseconds $pause }
  & $send 'h' 150
  & $send 'i' 400
  & $send '+a' 250
  & $send '^c' 250
  & $send ' ' 150
  & $send '{F5}' 150
  & $send '{TAB}' 150
  & $send '{LEFT}' 150
  & $send '{ENTER}' 300
  Start-Sleep -Milliseconds 500

  $still = Get-ForegroundTitle
  Write-Output "foreground after typing: $still"
  & 'D:\Projects\keyboard-control\tools\screenshot.ps1' -Title 'Fizz control' -Out $Out
} finally {
  # Leave the app running for a follow-up click test only when asked.
  if (-not $env:FIZZ_KEEP) { Stop-Process -Id $proc.Id -ErrorAction SilentlyContinue }
}
