using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Automation;

internal static class NativeUiSmoke
{
    private delegate bool WindowVisitor(IntPtr window, IntPtr state);
    [DllImport("user32.dll")] private static extern bool EnumDesktopWindows(IntPtr desktop, WindowVisitor callback, IntPtr state);
    [DllImport("user32.dll")] private static extern IntPtr GetThreadDesktop(uint thread);
    [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    private static readonly Stopwatch Clock = Stopwatch.StartNew();
    private static StreamWriter Log;

    private static void Event(string value)
    {
        Log.WriteLine("[{0} UTC] [INFO] [NATIVE-UIA] {1}", DateTime.UtcNow.ToString("yyyy-MM-dd HH:mm:ss"), value);
        Log.Flush();
    }

    private static AutomationElement FindWindow(int pid)
    {
        IntPtr found = IntPtr.Zero;
        WindowVisitor visit = delegate(IntPtr handle, IntPtr unused) {
            uint owner;
            GetWindowThreadProcessId(handle, out owner);
            if (owner == pid) { found = handle; return false; }
            return true;
        };
        EnumDesktopWindows(GetThreadDesktop(GetCurrentThreadId()), visit, IntPtr.Zero);
        GC.KeepAlive(visit);
        return found == IntPtr.Zero ? null : AutomationElement.FromHandle(found);
    }

    private static AutomationElement Find(AutomationElement root, string name, ControlType type)
    {
        return root.FindFirst(TreeScope.Descendants, new AndCondition(
            new PropertyCondition(AutomationElement.NameProperty, name),
            new PropertyCondition(AutomationElement.ControlTypeProperty, type)));
    }

    private static string Text(AutomationElement root)
    {
        var result = new StringBuilder();
        object pattern;
        if (root.TryGetCurrentPattern(TextPattern.Pattern, out pattern))
            result.Append(((TextPattern)pattern).DocumentRange.GetText(65536));
        var children = root.FindAll(TreeScope.Descendants, Condition.TrueCondition);
        if (children.Count > 2000) throw new InvalidOperationException("Native accessibility tree exceeded 2000 controls");
        foreach (AutomationElement child in children)
            if (!child.Current.IsOffscreen) result.AppendLine(child.Current.Name);
        return result.ToString();
    }

    private static void Wait(Func<bool> condition, string failure)
    {
        double until = Math.Min(120, Clock.Elapsed.TotalSeconds + 30);
        while (Clock.Elapsed.TotalSeconds < until)
        {
            try { if (condition()) return; }
            catch (ElementNotAvailableException) { }
            Thread.Sleep(100);
        }
        throw new TimeoutException(failure);
    }

    private static void Open(AutomationElement root, string workspace)
    {
        AutomationElement field = Find(root, "Workspace folder", ControlType.Edit);
        if (field == null) throw new InvalidOperationException("Workspace edit accessibility control missing");
        object value;
        if (!field.TryGetCurrentPattern(ValuePattern.Pattern, out value))
            throw new InvalidOperationException("Workspace edit does not support ValuePattern; no focus/keyboard fallback");
        ((ValuePattern)value).SetValue(workspace);
        AutomationElement button = Find(root, "Open workspace", ControlType.Button);
        if (button == null) throw new InvalidOperationException("Open workspace accessibility button missing");
        ((InvokePattern)button.GetCurrentPattern(InvokePattern.Pattern)).Invoke();
    }

    [MTAThread]
    private static int Main(string[] args)
    {
        if (args.Length != 3) return 2;
        string run = Path.GetFullPath(args[2]);
        string path = Path.Combine(run, "native-uia_" + DateTime.UtcNow.ToString("yyyy-MM-dd_HH-mm-ss") + "_UTC.log");
        try { Log = new StreamWriter(new FileStream(path, FileMode.CreateNew, FileAccess.Write), new UTF8Encoding(false)); }
        catch { return 3; }
        try
        {
            int pid = Int32.Parse(args[0]);
            string workspace = Path.GetFullPath(args[1]);
            AutomationElement root = null;
            Event("Inspecting only owned application pid=" + pid);
            Wait(delegate {
                root = FindWindow(pid);
                return root != null && Text(root).Contains("Native desktop bridge connected");
            }, "Original release accessibility content did not become ready");
            Event("Original release rendered native accessibility content");
            Open(root, workspace);
            Wait(delegate { return Text(root).Contains("Workspace opened"); }, "Native workspace status/sync did not complete");
            string rendered = Text(root);
            if (!rendered.Contains("sources") || !rendered.Contains("packages") || !rendered.Contains("installed"))
                throw new InvalidOperationException("Real workspace status response is missing from native UI");
            Event("Real workspace status displayed after native form invocation");
            string missing = Path.Combine(run, "missing-workspace");
            Open(root, missing);
            Wait(delegate { return Text(root).Contains("workspace: Select an absolute path to an existing workspace folder"); },
                 "Native invalid-workspace error was not rendered");
            if (Directory.Exists(missing)) throw new InvalidOperationException("Invalid workspace was created");
            Event("Real invalid workspace failure rendered without creating input");
            File.WriteAllText(Path.Combine(run, "uia-result.json"), "{\"status\":\"success\",\"surface\":\"native-accessibility\"}", new UTF8Encoding(false));
            Event("Native accessibility contract complete");
            return 0;
        }
        catch (Exception error)
        {
            Log.WriteLine("[{0} UTC] [ERROR] [NATIVE-UIA] {1}", DateTime.UtcNow.ToString("yyyy-MM-dd HH:mm:ss"), error);
            return 1;
        }
        finally { Log.Dispose(); }
    }
}
