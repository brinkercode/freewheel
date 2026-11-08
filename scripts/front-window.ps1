# Brings the main window of process -Id (or the newest process called
# -Name, for an app that hands its window to another process, or the window
# titled -Title) to the front for real, as a click
# would (the Windows e2e: profiles per app, scripts/e2e.mjs). Windows only
# lets the foreground app hand the foreground on, so this presses and
# releases Alt first (the usual way around that lock), then checks that the
# window really is in front. Prints "ok" or what is in front instead.
param([int]$Id = 0, [string]$Name = "", [string]$Title = "")

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class FwFront {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
  // The visible top-level window titled exactly `title` (zero: none yet).
  public static IntPtr ByTitle(string title) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      if (!IsWindowVisible(h)) return true;
      var s = new System.Text.StringBuilder(512);
      GetWindowText(h, s, 512);
      if (s.ToString() == title) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@

$deadline = (Get-Date).AddSeconds(30)
$hwnd = [IntPtr]::Zero
while ((Get-Date) -lt $deadline) {
  if ($Title) {
    $hwnd = [FwFront]::ByTitle($Title)
    if ($hwnd -ne [IntPtr]::Zero) { break }
    Start-Sleep -Milliseconds 200
    continue
  }
  if ($Name) {
    $p = Get-Process -Name $Name -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } | Sort-Object StartTime -Descending | Select-Object -First 1
    if ($p) { $Id = $p.Id }
  } else {
    $p = Get-Process -Id $Id -ErrorAction SilentlyContinue
  }
  if ($p) { $p.Refresh(); $hwnd = $p.MainWindowHandle }
  if ($hwnd -ne [IntPtr]::Zero) { break }
  Start-Sleep -Milliseconds 200
}
if ($hwnd -eq [IntPtr]::Zero) { Write-Output "no window for process $Id $Name $Title"; exit 1 }

for ($i = 0; $i -lt 10; $i++) {
  [FwFront]::ShowWindow($hwnd, 9) | Out-Null # SW_RESTORE
  [FwFront]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero) # Alt down
  [FwFront]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero) # Alt up
  [FwFront]::SetForegroundWindow($hwnd) | Out-Null
  Start-Sleep -Milliseconds 300
  if ([FwFront]::GetForegroundWindow() -eq $hwnd) { Write-Output "ok"; exit 0 }
}
$front = [FwFront]::GetForegroundWindow()
$owner = 0
[FwFront]::GetWindowThreadProcessId($front, [ref]$owner) | Out-Null
$name = (Get-Process -Id $owner -ErrorAction SilentlyContinue).ProcessName
Write-Output "process $Id is not in front: $name ($owner) is"
exit 1
