# Sichtbare Texte mit Umlauten (#83)

Oberfläche, Laufprotokolle, Fehlermeldungen und neue Haltgründe verwenden ä, ö, ü und ß. Maschinenkennungen, Formularwerte, URLs, Formatparameter, Dateinamen, Journalarten und kodex/*.txt bleiben unverändert. Windows-Startdateien bleiben ASCII; Rust gibt die Unicode-Texte aus.

Die zwei historischen Notbremsentexte im Journal bleiben exakt erhalten, damit alte Sperren wirksam bleiben. Die fünf eingefrorenen Ereignisse des historischen Beispielszenarios bleiben ebenfalls unverändert, weil ihre Bytes zum Wiederabspielhash gehören. Originalbelege und fremde Nachrichtentexte werden nicht sprachlich umgeschrieben. Die bestehenden Format-/Wiederanlaufprüfungen laufen weiter. Eine zusätzliche Gegenprobe öffnet beide alten ASCII-Sperrtexte, prüft die Sperrwirkung und vergleicht Journal und Vermerk anschließend bytegleich.

Erwartungen an neue Fehlermeldungen verwenden die neue Schreibweise. Die Export-Gegenprobe akzeptiert für den unveränderten Fehlergrund „nicht prüfbar“ die alte und die neue Schreibweise, damit sie beim getrennten Umstellen der Kodex-Ausgaben nicht fälschlich scheitert. Ein eigener Kodex-Abschluss ergänzt die Ausgaben des Prüfprogramms und einen Quelltexttest gegen erneute Umschreibungen. Die normale Windows-CI prüft Ausgaben, Oberfläche und Startabläufe; eine persönliche Sichtprüfung des schwarzen Fensters am Benutzerrechner wird dadurch nicht behauptet.

## Rückfallprüfung im Kodex

Der vorhandene Rust-Leser kann zusätzlich Zeichenketteninhalte sammeln; die normale Tokenfolge und die Fehler bei unvollständigem Quelltext bleiben gleich. Die Sprachprüfung liest ausschließlich Rust-Quellen unter crates/*/src, ignoriert Kommentare und Bezeichner und meldet typische deutsche Umschreibungen in Meldungssätzen. Maschinenwerte, URLs, Dateipfade, Formatparameter und zitierte Feldnamen bleiben ASCII. Die ausdrückliche historische Ausnahmeliste enthält genau die zwei alten Sperrtexte und fünf eingefrorene Beispielsereignisse, jeweils an ihren Quellpfad gebunden.

Gegenproben zeigen erkannte fehlerhafte Meldungen, korrekte Umlaute, unveränderte Maschinenkennungen, ignorierte Kommentare und Zeichenliterale, normale und rohe Zeichenketten sowie abgelehnten unvollständigen Quelltext. Dieser Test läuft mit dem vollständigen Gate unter Linux und Windows. Die Prüfung erkennt typische Schreibweisen; sie übersetzt keine Originalnachrichten und ist keine allgemeine Grammatikprüfung.
