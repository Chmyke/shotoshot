# Runs a command while watching the foreground window, and fails if a program from the given
# folder (the build output) took it: tests must never take the keyboard from the user.
# The watch reads the real foreground (GetForegroundWindow). The foreground event alone is not
# enough: Windows raises it too when a background thread activates its own window.
param([Parameter(Mandatory)][string]$Under, [Parameter(Mandatory)][scriptblock]$Run)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
public static class ForegroundWatch {
    delegate void WinEventProc(IntPtr hook, uint ev, IntPtr hwnd, int obj, int child, uint thread, uint time);
    [StructLayout(LayoutKind.Sequential)]
    struct MSG { public IntPtr hwnd; public uint message; public IntPtr wParam; public IntPtr lParam; public uint time; public int x; public int y; }
    [DllImport("user32.dll")] static extern IntPtr SetWinEventHook(uint min, uint max, IntPtr module, WinEventProc proc, uint pid, uint tid, uint flags);
    [DllImport("user32.dll")] static extern bool UnhookWinEvent(IntPtr hook);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr hwnd, StringBuilder name, int size);
    [DllImport("user32.dll")] static extern bool PeekMessageW(out MSG msg, IntPtr hwnd, uint min, uint max, uint remove);
    [DllImport("user32.dll")] static extern IntPtr DispatchMessageW(ref MSG msg);
    [DllImport("user32.dll")] static extern bool PostThreadMessageW(uint thread, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] static extern uint MsgWaitForMultipleObjects(uint count, IntPtr handles, bool all, uint ms, uint mask);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] static extern IntPtr OpenThread(uint access, bool inherit, uint tid);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern bool QueryFullProcessImageNameW(IntPtr process, uint flags, StringBuilder name, ref int size);
    [DllImport("kernel32.dll")] static extern int GetThreadDescription(IntPtr thread, out IntPtr description);
    [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr memory);
    static string under;
    static List<string> found;
    static IntPtr last;
    static uint loopThread;
    static Thread loop;
    static WinEventProc proc;
    public static void Start(string folder) {
        under = Path.GetFullPath(folder).TrimEnd('\\') + "\\";
        found = new List<string>();
        last = IntPtr.Zero;
        ManualResetEvent ready = new ManualResetEvent(false);
        loop = new Thread(delegate () { Loop(ready); });
        loop.IsBackground = true;
        loop.Start();
        ready.WaitOne();
    }
    public static string[] Stop() {
        PostThreadMessageW(loopThread, 0x0012, IntPtr.Zero, IntPtr.Zero);
        loop.Join();
        return found.ToArray();
    }
    static void Loop(ManualResetEvent ready) {
        loopThread = GetCurrentThreadId();
        MSG msg;
        PeekMessageW(out msg, IntPtr.Zero, 0, 0, 0);
        // The event only triggers a reading: the window it names may be activated in the background.
        proc = delegate (IntPtr h, uint e, IntPtr w, int o, int c, uint t, uint ms) { Check(); };
        IntPtr hook = SetWinEventHook(3, 3, IntPtr.Zero, proc, 0, 0, 0);
        ready.Set();
        while (true) {
            MsgWaitForMultipleObjects(0, IntPtr.Zero, false, 5, 0x04FF);
            Check();
            while (PeekMessageW(out msg, IntPtr.Zero, 0, 0, 1)) {
                if (msg.message == 0x0012) { UnhookWinEvent(hook); return; }
                DispatchMessageW(ref msg);
            }
        }
    }
    static void Check() {
        IntPtr window = GetForegroundWindow();
        if (window == last) return;
        last = window;
        uint pid;
        uint tid = GetWindowThreadProcessId(window, out pid);
        string image = ProcessImage(pid);
        if (image == null || !image.StartsWith(under, StringComparison.OrdinalIgnoreCase)) return;
        StringBuilder name = new StringBuilder(256);
        GetClassNameW(window, name, name.Capacity);
        found.Add(string.Format("{0:HH:mm:ss.fff} {1}, thread '{2}', window class {3}",
            DateTime.Now, Path.GetFileName(image), ThreadName(tid), name));
    }
    static string ProcessImage(uint pid) {
        IntPtr process = OpenProcess(0x1000, false, pid);
        if (process == IntPtr.Zero) return null;
        StringBuilder name = new StringBuilder(1024);
        int size = name.Capacity;
        bool ok = QueryFullProcessImageNameW(process, 0, name, ref size);
        CloseHandle(process);
        return ok ? name.ToString() : null;
    }
    // libtest names each test's thread after the test.
    static string ThreadName(uint tid) {
        IntPtr thread = OpenThread(0x0800, false, tid);
        if (thread == IntPtr.Zero) return "?";
        string result = "?";
        IntPtr description;
        if (GetThreadDescription(thread, out description) >= 0 && description != IntPtr.Zero) {
            result = Marshal.PtrToStringUni(description);
            LocalFree(description);
        }
        CloseHandle(thread);
        return result;
    }
}
'@
[ForegroundWatch]::Start($Under)
try {
    & $Run
} finally {
    $taken = [ForegroundWatch]::Stop()
}
if ($taken) {
    throw "A program from $Under took the foreground (the keyboard) from the user:`n  $($taken -join "`n  ")"
}
