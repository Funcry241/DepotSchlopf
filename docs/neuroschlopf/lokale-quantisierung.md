# NeuroSchlopf · Quantisierung auf einer lokalen RTX 3070

Die [RTX 3070](https://www.nvidia.com/de-de/geforce/graphics-cards/30-series/rtx-3070-3070ti/) hat 8 GB Grafikspeicher, die sich Windows, Anzeige und weitere Programme mit NeuroSchlopf teilen. Der Versuch misst, welche Genauigkeit und Ausführungsweise die Zeit bis zum fertigen Ergebnis senkt. Trainingslauf, laufende Textverarbeitung und Auswertung haben unterschiedliche Anforderungen.

## Messreihenfolge

| Schritt | Fester Vergleich | Entscheidung |
|---|---|---|
| 1 · Gebündeltes FP32 | CPU gegen WGPU auf denselben Fallblöcken. Der bereits vorbereitete Stapeltest enthält den Einzelabruf als Kontrolle. | Ausgangswert für Laufzeit und Rangwerte. |
| 2 · Kompaktere Textvektoren | Beim selben eingefrorenen Qwen3-Embedding-Modell die Ausgabedimension von 1.024 auf 512 und 256 kürzen und erneut normalisieren. Fallbestand und Rechenpräzision bleiben für diesen Versuch gleich. | Prüft, ob kleinere Vektoren für Cache, Übertragung und nachgelagerte Bewertung ausreichen. |
| 3 · Quantisierung des Textmodells | FP16 oder BF16 als Referenz innerhalb derselben Laufzeitumgebung, dann 8-Bit-Gewichte und bei Bedarf 4-Bit-Gewichte. Tokenizer, Pooling, Eingabe, Ausgabedimension und Startbedingungen bleiben gleich. | Prüft tatsächlich genutzten VRAM und Geschwindigkeit sowie die Auswirkungen auf relevante Top-3-Belege. |
| 4 · Präzision des Marktmodells | Identisches trainiertes Modell auf WGPU zuerst mit FP32, dann mit FP16 bei bestätigter Unterstützung. Danach optional 8-Bit-Gewichte nach dem Training. | Prüft, ob der ausgeführte Matrixpfad schneller ist und Ausgaben nahe an einer Entscheidungsschwelle stabil bleiben. |

[Qwen3-Embedding-0.6B](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B) unterstützt kürzere Matryoshka-Vektoren. Ein kürzerer Ausgabevektor spart nach der Kodierung Speicher und Rechenarbeit. Die Zeit für den Durchlauf des Textmodells wird separat gemessen. Bei einer anderen Laufzeitumgebung gilt zunächst ein eigener FP16- oder BF16-Lauf als Ausgangswert. Damit lassen sich Änderungen durch die Laufzeitumgebung und die Quantisierung unterscheiden.

Für WGPU unter Windows werden Adapter, Grafik-API und Datentyp-Unterstützung ausgegeben. Eine kleinere Darstellung der Gewichte spart Speicher. Geschwindigkeit entsteht erst, wenn die tatsächlich verwendeten Kernel die Darstellung effizient verarbeiten oder weniger Daten übertragen müssen. Die [Burn-Dokumentation](https://burn.dev/books/burn/performance/quantization.html) beschreibt Quantisierung nach dem Training mit vom Backend abhängigen Formaten. Ob die RTX, der Treiber und der eingesetzte WGPU-Pfad den gewünschten Datentyp beschleunigen, entscheidet der Lauf. Trainingsparameter, Optimiererzustand, Fehleraggregation und Schwellwertrechnung bleiben in der ersten Implementierung in FP32. FP16 für das Training wäre ein eigener Versuch.

## Betrieb und Messgrößen

- Neue Texte werden einmal kodiert. Die Vektoren werden mit Textabdruck, Modellrevision, Präzision und Dimension gespeichert. Änderungen an einer dieser Angaben erzeugen einen neuen Cache-Eintrag.
- Pro Entscheidungszeitpunkt landen Instrumente und Horizonte zusammen in einem Batch. Für den Test werden Einzelereignisse, kleine Blöcke und ein voller Aktualisierungslauf getrennt gemessen. Der lokale Worker bündelt ohne eine feste Wartezeit, die kurzlebige Hinweise verzögert. Der GPU-Pfad hält gemeinsame Zwischenergebnisse bis zur Ausgabe auf der Karte.
- Der Bericht trennt Modellstart, Kodierung, Rangberechnung und Antwortzeit. Er nennt den VRAM-Höchststand, bei Wiederholungen die 95. Perzentile der Antwortzeit, die erreichte Präzision und den vermessenen Backend-Pfad. CPU-Baseline und GPU-Varianten laufen auf demselben Rechner.
- Für die Entscheidung zählt die Zeit bis zur Papierchance bei erhaltener Qualität. Die Auswertung prüft dieselben späteren Belege, gleiche Top-3-Metriken und die Wirkung auf Entscheidungen nach Kosten. Fälle nahe an Schwellenwerten werden gesondert geprüft. Eine schnellere Variante mit unklaren Grenzfällen kann dort eine zweite Bewertung in FP32 auslösen.
- Die 10 % GPU-Hintergrundlast werden auf dem Zielrechner gesondert gemessen. Quantisierung verändert Rechenarbeit und Speicherbedarf. Die Hintergrundlast hängt zusätzlich von Batchgröße, Taktung und Konkurrenz mit anderen Programmen ab.

Die [Topologien für die Kapitalrotation](topologien-kapitalrotation.md) verwenden dieselben Zeit- und Qualitätsmessgrößen.
