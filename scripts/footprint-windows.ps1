# Windows probes for scripts/footprint-windows.mjs (one call per probe):
#   procs              every process as JSON: pid, parent, name, working set,
#                      private bytes, CPU time (100 ns units)
#   windows -Pids a,b  the visible top-level windows of those processes
#   close   -Pids a,b  asks each of them to close (WM_CLOSE, as the title
#                      bar's Close button does), then lists what was closed
param(
  [Parameter(Mandatory = $true)][ValidateSet("procs", "windows", "close")][string]$Action,
  [string]$Pids = ""
)
$ErrorActionPreference = "Stop"

if ($Action -eq "procs") {
  Get-CimInstance Win32_Process |
    ForEach-Object {
      [pscustomobject]@{
        pid = [int]$_.ProcessId
        parent = [int]$_.ParentProcessId
        name = $_.Name
        workingSet = [int64]$_.WorkingSetSize
        privateBytes = [int64]$_.PrivatePageCount
        cpu100ns = [int64]$_.KernelModeTime + [int64]$_.UserModeTime
      }
    } | ConvertTo-Json -Compress -Depth 2
  exit 0
}

Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class FwWin {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static List<string> Visible(uint[] pids, bool close) {
    var found = new List<string>();
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (Array.IndexOf(pids, pid) >= 0 && IsWindowVisible(h)) {
        var t = new StringBuilder(256); GetWindowText(h, t, 256);
        found.Add(pid + ":" + t.ToString());
        if (close) PostMessage(h, 0x0010, IntPtr.Zero, IntPtr.Zero);
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@
[uint32[]]$ids = @($Pids -split "," | Where-Object { $_ } | ForEach-Object { [uint32]$_ })
$list = [FwWin]::Visible($ids, $Action -eq "close")
ConvertTo-Json -Compress -InputObject @($list)
