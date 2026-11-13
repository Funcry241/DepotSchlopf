# Externe Datensicherung

Trage unter **Einrichtung → Sicherung** einen vorhandenen absoluten Zielordner außerhalb des
Projektordners ein. Ein leeres Feld schaltet die automatische Sicherung aus. Unter
`<Ziel>/DepotSchlopf` bleiben die jüngsten 30 geprüften Sicherungen. Wähle einen zweiten
Datenträger, um deine Daten bei einem Plattenausfall zu schützen. Ein synchronisierter Ordner
überträgt dein Papierdepot und deine Belege zum Cloud-Anbieter.

**Jetzt sichern** und `run.cmd sichern` starten denselben Ablauf. Bei gesetztem Ziel startet die automatische Sicherung täglich ab 23:30 Frankfurter Zeit. Hat der
Dienst zu dieser Zeit pausiert, prüft er beim nächsten Start, ob eine Sicherung nachzuholen ist.

## Wiederherstellen

1. DepotSchlopf beenden und den beschädigten Datenordner beiseitelegen.
2. Aus dem jüngsten geprüften Ordner die enthaltenen Dateien nach `daten/` kopieren.
3. `DepotSchlopf.cmd` starten. Das Journal prüft seine Kette beim Öffnen.
