# Referenzlauf des Neuro2026-Kerns

**Live → NeuroSchlopf → Neuro2026** zeigt den abgeschlossenen Multiplikationsversuch vom 20.11.2026. Neuro2026 0.1.16 lief unter Linux mit Rust 1.92 und Burn Flex 0.21 auf der CPU. Der Versuch verwendete 81 Trainingspaare und 64 getrennte Testpaare auf den Zwischenwerten. Die Kurve zeigt die tatsächlich gemessenen Fehler je Epoche. Die Testausgabe stammt von den gespeicherten und erneut geladenen Gewichten. Ein zweiter Trainingslauf reproduzierte alle 240 Epochenwerte und alle 64 Vorhersagen. Der bestehende NeuroSchlopf-Schalter steuert derzeit den Journalvergleich. Die Relevanz von Belegen erfordert einen eigenen Modelllauf mit Depotdaten.

Die Oberfläche und der Download verwenden dieselbe [Originalmessung](lernlauf-v0.1.16.json). Ihr SHA-256 über die UTF-8-Datei mit LF lautet `025649b4417e88eb92f6484914ff60280a510a30b797045e8bb92dbe57a2ddb4`.
