// Messkind fuer #325 Fragen 73-78 (Dachs). Nur Messung, wird nie uebernommen.
// Argumente: faeden sekunden enkel(0/1)
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

static class Kind {
    [StructLayout(LayoutKind.Sequential)] struct Rate { public uint Flags; public uint CpuRate; }
    [DllImport("kernel32.dll")] static extern bool IsProcessInJob(IntPtr p, IntPtr job, out bool drin);
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll")] static extern bool QueryInformationJobObject(IntPtr job, int klasse, out Rate info, int laenge, IntPtr rueck);

    static int Main(string[] a) {
        int faeden = int.Parse(a[0]); int sek = int.Parse(a[1]); bool enkel = a[2] == "1";
        string zeile = Console.ReadLine();
        if (zeile != "los") { Console.WriteLine("KIND kein Startsignal"); return 4; }
        bool drin; IsProcessInJob(GetCurrentProcess(), IntPtr.Zero, out drin);
        Rate r; bool ok = QueryInformationJobObject(IntPtr.Zero, 15, out r, 8, IntPtr.Zero);
        // ENABLE=1, HARD_CAP=4
        bool ungeprueft = a.Length > 3 && a[3] == "ungeprueft";
        if (!ungeprueft && (!drin || !ok || (r.Flags & 5) != 5 || r.CpuRate == 0 || r.CpuRate > 1000)) {
            Console.WriteLine("KIND Selbstpruefung FEHLT: im_job=" + drin + " abfrage=" + ok + " flags=" + r.Flags + " rate=" + r.CpuRate + " -> keine Rechenarbeit");
            return 3;
        }
        Console.WriteLine("KIND Selbstpruefung ok: im_job=True flags=" + r.Flags + " rate=" + r.CpuRate);
        Process e = null;
        if (enkel) {
            var si = new ProcessStartInfo(Process.GetCurrentProcess().MainModule.FileName, faeden + " " + sek + " 0");
            si.UseShellExecute = false; si.RedirectStandardInput = true;
            e = Process.Start(si); e.StandardInput.WriteLine("los"); e.StandardInput.Close();
        }
        var t = new Thread[faeden];
        for (int i = 0; i < faeden; i++) {
            t[i] = new Thread(() => { var sw = Stopwatch.StartNew(); double x = 1; while (sw.Elapsed.TotalSeconds < sek) { for (int k = 0; k < 100000; k++) x = Math.Sqrt(x + k); } if (x < 0) Console.WriteLine(x); });
            t[i].Start();
        }
        foreach (var th in t) th.Join();
        if (e != null) { e.WaitForExit(); Console.WriteLine("KIND Enkel Exit=" + e.ExitCode); }
        return 0;
    }
}
