// Messer fuer #325 Fragen 73-78 (Dachs). Nur Messung, wird nie uebernommen.
// Argumente: name rate faeden sekunden enkel(0/1) zuordnen(0/1) verschachtelt(0/1) morgenlauf(0/1)
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

static class Messer {
    [StructLayout(LayoutKind.Sequential)] struct Rate { public uint Flags; public uint CpuRate; }
    [StructLayout(LayoutKind.Sequential)] struct Basis { public long PerProcessUserTimeLimit, PerJobUserTimeLimit; public uint LimitFlags; public UIntPtr MinWs, MaxWs; public uint ActiveProcessLimit; public UIntPtr Affinity; public uint PriorityClass, SchedulingClass; }
    [StructLayout(LayoutKind.Sequential)] struct Io { public ulong a, b, c, d, e, f; }
    [StructLayout(LayoutKind.Sequential)] struct Erw { public Basis B; public Io I; public UIntPtr ProcessMemoryLimit, JobMemoryLimit, PeakProcessMemoryUsed, PeakJobMemoryUsed; }
    [StructLayout(LayoutKind.Sequential)] struct Konto { public long TotalUserTime, TotalKernelTime, ThisPeriodUser, ThisPeriodKernel; public uint PageFaults, TotalProcesses, ActiveProcesses, Terminated; }
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr CreateJobObject(IntPtr sa, string name);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr job, int klasse, ref Rate info, int laenge);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr job, int klasse, ref Erw info, int laenge);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool QueryInformationJobObject(IntPtr job, int klasse, out Konto info, int laenge, IntPtr rueck);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool QueryInformationJobObject(IntPtr job, int klasse, out Erw info, int laenge, IntPtr rueck);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr p);
    [DllImport("kernel32.dll")] static extern bool IsProcessInJob(IntPtr p, IntPtr job, out bool drin);
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll")] static extern uint GetActiveProcessorCount(ushort gruppe);

    static double Arbeit() { var sw = Stopwatch.StartNew(); double x = 1; for (int k = 0; k < 150000000; k++) x = Math.Sqrt(x + k); if (x < 0) Console.WriteLine(x); return sw.Elapsed.TotalMilliseconds; }

    static int Main(string[] a) {
        string name = a[0]; uint rate = uint.Parse(a[1]); int faeden = int.Parse(a[2]); int sek = int.Parse(a[3]);
        bool enkel = a[4] == "1", zuordnen = a[5] == "1", verschachtelt = a[6] == "1", morgen = a[7] == "1";
        uint lp = GetActiveProcessorCount(0xFFFF);
        if (faeden <= 0) faeden = (int)lp;
        bool selbst; IsProcessInJob(GetCurrentProcess(), IntPtr.Zero, out selbst);
        Console.WriteLine("== Fall " + name + " | logische_prozessoren=" + lp + " | Environment.ProcessorCount=" + Environment.ProcessorCount + " | messer_schon_im_job=" + selbst);
        if (selbst) {
            Erw aussen; bool ok = QueryInformationJobObject(IntPtr.Zero, 9, out aussen, Marshal.SizeOf(typeof(Erw)), IntPtr.Zero);
            Console.WriteLine("   umgebender Job: abfrage=" + ok + " LimitFlags=0x" + aussen.B.LimitFlags.ToString("X") + " (BREAKAWAY_OK=0x800, SILENT_BREAKAWAY_OK=0x1000, KILL_ON_CLOSE=0x2000)");
        }
        if (verschachtelt) {
            IntPtr aussenJob = CreateJobObject(IntPtr.Zero, null);
            Erw ae = new Erw(); ae.B.LimitFlags = 0x2000;
            SetInformationJobObject(aussenJob, 9, ref ae, Marshal.SizeOf(typeof(Erw)));
            Console.WriteLine("   Messer tritt aeusserem Job ohne CPU-Grenze bei (wie Aufgabenplanung): " + AssignProcessToJobObject(aussenJob, GetCurrentProcess()) + " err=" + Marshal.GetLastWin32Error());
        }
        double basis = 0;
        if (morgen) { Arbeit(); basis = Math.Min(Arbeit(), Arbeit()); Console.WriteLine("   Morgenlauf-Probe ohne Last: " + basis.ToString("F0") + " ms"); }

        IntPtr job = CreateJobObject(IntPtr.Zero, null);
        Rate r = new Rate(); r.Flags = 1 | 4; r.CpuRate = rate;
        bool rs = SetInformationJobObject(job, 15, ref r, 8); int rerr = Marshal.GetLastWin32Error();
        Erw e = new Erw(); e.B.LimitFlags = 0x2000;
        bool es = SetInformationJobObject(job, 9, ref e, Marshal.SizeOf(typeof(Erw)));
        Console.WriteLine("   Job: cpu_rate_gesetzt=" + rs + " (err " + rerr + ") kill_on_close=" + es + " rate=" + rate);

        var si = new ProcessStartInfo(System.IO.Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "kind.exe"), faeden + " " + sek + " " + (enkel ? "1" : "0"));
        si.UseShellExecute = false; si.RedirectStandardInput = true;
        var p = Process.Start(si);
        bool zu = zuordnen && AssignProcessToJobObject(job, p.Handle);
        Console.WriteLine("   Kind gestartet (wartet auf Startsignal), zugeordnet=" + zu);
        var start = Stopwatch.StartNew();
        p.StandardInput.WriteLine("los"); p.StandardInput.Close();

        var proben = new List<long>(); var zeiten = new List<double>();
        var morgenZeiten = new List<double>();
        Thread mt = null;
        if (morgen) { mt = new Thread(() => { Thread.Sleep(2000); for (int i = 0; i < 3; i++) morgenZeiten.Add(Arbeit()); }); mt.Start(); }
        Konto k = new Konto();
        while (!p.HasExited) {
            QueryInformationJobObject(job, 1, out k, Marshal.SizeOf(typeof(Konto)), IntPtr.Zero);
            proben.Add(k.TotalUserTime + k.TotalKernelTime); zeiten.Add(start.Elapsed.TotalMilliseconds);
            Thread.Sleep(1000);
        }
        QueryInformationJobObject(job, 1, out k, Marshal.SizeOf(typeof(Konto)), IntPtr.Zero);
        proben.Add(k.TotalUserTime + k.TotalKernelTime); zeiten.Add(start.Elapsed.TotalMilliseconds);
        if (mt != null) mt.Join();

        Console.WriteLine("   t_ms;cpu_100ns;last_promille_1s");
        double max5 = 0;
        for (int i = 1; i < proben.Count; i++) {
            double pm = (proben[i] - proben[i - 1]) / ((zeiten[i] - zeiten[i - 1]) * 10000.0 * lp) * 1000;
            if (i % 5 == 0 || i == proben.Count - 1) Console.WriteLine("   " + zeiten[i].ToString("F0") + ";" + proben[i] + ";" + pm.ToString("F1"));
            if (i >= 5) { double w = (proben[i] - proben[i - 5]) / ((zeiten[i] - zeiten[i - 5]) * 10000.0 * lp) * 1000; if (w > max5) max5 = w; }
        }
        double gesamt = proben[proben.Count - 1] / (zeiten[zeiten.Count - 1] * 10000.0 * lp) * 100;
        Console.WriteLine("ERGEBNIS " + name + ": exit=" + p.ExitCode + " prozesse_im_job=" + k.TotalProcesses + " last_gesamt=" + gesamt.ToString("F2") + " % max_5s=" + (max5 / 10).ToString("F2") + " % dauer=" + (zeiten[zeiten.Count - 1] / 1000).ToString("F1") + " s");
        if (morgen && morgenZeiten.Count > 0) { double m = 0; foreach (var z in morgenZeiten) m = Math.Max(m, z); Console.WriteLine("ERGEBNIS " + name + " Morgenlauf-Probe: ohne Last " + basis.ToString("F0") + " ms, unter Job-Last max " + m.ToString("F0") + " ms, Faktor " + (m / basis).ToString("F2")); }
        return 0;
    }
}
