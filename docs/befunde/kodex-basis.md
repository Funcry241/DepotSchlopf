# Kodex-Prüfung aus dem Basisstand

Der Workflow `kodex-basis.yml` baut den Kodex von `main`. Der Prüfer liest die exakte
Kandidaten-SHA als Git-Objekte und prüft die Pfade, die Deklaration und bei geschützten
Dateien die gebundene Freigabe. Der Status `kodex-basis` gehört zur Kandidaten-SHA.
Der normale Workflow führt weiterhin Gate und Tests des Kandidaten aus.

Die Festschreibung bindet die Dateien unter `crates/depot_kodex`, `.github` und
`kodex`. Deklaration, Festschreibung und Freigabedatei sind vom Fingerabdruck
ausgenommen, damit der Fingerabdruck erzeugt und freigegeben werden kann.

Der Quellscan erfasst direkte Prozessstarts und Importe von `Command`, auch mit
Alias. Dynamische Starts sind je Datei und Anzahl begrenzt. Der Scan wertet
keine codeerzeugenden Makros oder FFI aus. Die übrigen Kodex-Regeln begrenzen
diese Wege bereits.

Nach dem Einführungsmerge ist `kodex-basis` in GitHub als Pflichtprüfung für
`main` einzustellen. Den Einführungsnachweis liefern die Windows-Update-Prüfung
und der danach angelegte Folge-Entwurf mit rotem und grünem Kandidatenstatus.
