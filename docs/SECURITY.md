# Sicherheit

DepotSchlopf kann mit echtem Geld handeln. Sicherheitslücken nehmen wir deshalb besonders ernst.

## Was als Sicherheitslücke gilt

- Ein Weg, eine Order am Regelwerk vorbei zu senden oder einen `GenehmigterAuftrag` außerhalb von `depot_regelwerk` zu erzeugen.
- Ein Weg, KI-Aufrufe ohne `AufrufErlaubnis` oder über das Budget hinaus auszulösen.
- Ein Weg, harte Grenzen, Produktarten oder Lints zu ändern, ohne dass der Kodex ablehnt.
- Ein Weg, ein Produkt zu kaufen, dessen Verlust den Einsatz übersteigen kann.
- Zugangsdaten, die im Repository, in Logs oder Berichten landen.

## Melden

Bitte **nicht** als öffentliches Issue. Nutze die private Sicherheitsmeldung von GitHub: im Repository unter **Security → Report a vulnerability**.

Beschreibe den Weg so konkret wie möglich (betroffene Datei, Eingabe, beobachtetes Ergebnis). Bitte keine echten Kontodaten, Schlüssel oder Depotstände mitschicken.

## Was du erwarten kannst

Das Projekt wird privat gepflegt. Eine feste Antwortfrist ist nicht zugesagt. Eine bestätigte Lücke wird vorrangig behoben. Bis zur Behebung wird kein Echtgeld gehandelt.

## Grundsätze im Code

- Fail-closed: Jeder unklare Zustand sperrt.
- Zugangsdaten nur in der Windows-Anmeldeverwaltung, nie in Dateien im Repository.
- Der Kodex prüft jede Änderung; siehe `docs/kaltstart.md`.
