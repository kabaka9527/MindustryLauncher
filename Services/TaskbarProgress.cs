using System.Runtime.InteropServices;

namespace MindustryLauncher.Services;

internal enum TaskbarProgressState
{
    NoProgress = 0,
    Indeterminate = 0x1,
    Normal = 0x2,
    Error = 0x4,
    Paused = 0x8
}

[ComImport]
[Guid("ea1afb91-9e28-4b86-90e9-9e9f8a5eefaf")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface ITaskbarList3
{
    void HrInit();
    void AddTab(IntPtr hwnd);
    void DeleteTab(IntPtr hwnd);
    void ActivateTab(IntPtr hwnd);
    void SetActiveAlt(IntPtr hwnd);
    void MarkFullscreenWindow(IntPtr hwnd, [MarshalAs(UnmanagedType.Bool)] bool fFullscreen);
    void SetProgressValue(IntPtr hwnd, ulong ullCompleted, ulong ullTotal);
    void SetProgressState(IntPtr hwnd, TaskbarProgressState state);
}

[ComImport]
[Guid("56FDF344-FD6D-11d0-958A-006097C9A090")]
class TaskbarList { }

internal static class TaskbarProgress
{
    private static readonly ITaskbarList3? _taskbar;

    static TaskbarProgress()
    {
        try
        {
            _taskbar = (ITaskbarList3)new TaskbarList();
            _taskbar.HrInit();
        }
        catch
        {
            _taskbar = null;
        }
    }

    public static void Normal(IntPtr hwnd, ulong completed, ulong total)
    {
        _taskbar?.SetProgressValue(hwnd, completed, total);
        _taskbar?.SetProgressState(hwnd, TaskbarProgressState.Normal);
    }

    public static void Indeterminate(IntPtr hwnd)
    {
        _taskbar?.SetProgressState(hwnd, TaskbarProgressState.Indeterminate);
    }

    public static void Paused(IntPtr hwnd)
    {
        _taskbar?.SetProgressState(hwnd, TaskbarProgressState.Paused);
    }

    public static void Error(IntPtr hwnd)
    {
        _taskbar?.SetProgressState(hwnd, TaskbarProgressState.Error);
    }

    public static void Clear(IntPtr hwnd)
    {
        _taskbar?.SetProgressState(hwnd, TaskbarProgressState.NoProgress);
    }
}
