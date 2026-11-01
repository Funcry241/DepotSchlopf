# GitHub-Einstellungen

Diese Einstellungen lassen sich nicht per Commit setzen. Der Eigentümer trägt sie einmal auf GitHub ein.

## Beschreibung und Suchwörter

Auf der Startseite des Repositorys rechts neben **About** auf das Zahnrad klicken.

**Description:**

```text
Rust-Rahmen: Zwei KIs (Claude und GPT) beraten ein Aktiendepot, ein getestetes Regelwerk entscheidet. Harte Grenzen für Risiko, Gebühren und KI-Kosten. Experimentell, keine Anlageberatung.
```

**Topics:**

```text
rust trading algorithmic-trading portfolio-management risk-management interactive-brokers dax aktien boerse depot ki kuenstliche-intelligenz llm claude gpt paper-trading fail-closed deutsch
```

Haken bei „Releases“ und „Packages“ entfernen, solange es keine gibt.

## Private Sicherheitsmeldungen

**Settings → Code security → Private vulnerability reporting → Enable.** Darauf verweist `docs/SECURITY.md`.

## Schutz für main

**Settings → Branches → Add branch ruleset** (oder „Add rule“) für `main`:

- Require a pull request before merging
- Require status checks to pass: **kodex**
- Block force pushes

Damit kommt nichts nach `main`, was das Gate nicht bestanden hat.

## Wiki

Das Wiki bleibt am besten aus (**Settings → General → Features → Wikis**). Alles Wissen steht im Repository (Essenz D00 und D16).
