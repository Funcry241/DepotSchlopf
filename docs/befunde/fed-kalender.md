# Fed-Kalender (#121)

`run.cmd termine` ruft nach EZB und Ergebniskalender die [FOMC-Kalenderseite](https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm) einmal je Frankfurter Tag ohne Schlüssel ab. Der Sammler legt die öffentliche HTML-Antwort unter `daten/roh/fed/` ab und prüft beide veröffentlichten Jahresblöcke vollständig, bevor er Termine belegt. Eine fehlerhafte Zeile führt zu einem Fed-Abruffehler. Termine der übrigen Quellen behalten ihre Belege.

Die Kennung `fed-JJJJ-MM-TT` und der Betroffene `usd-leitzins` beziehen sich auf den zweiten Sitzungstag. Ein Stern kennzeichnet veröffentlichte Projektionen. Die Terminquelle belegt den Kalendertag. Die Fed kennzeichnet künftige Termine als vorläufig. Eine Strategiesitzung wie der 26.–27. Oktober 2027 erscheint im Abrufbericht. Die Fed hat dafür keinen Zinsentscheid angekündigt. Fehlende künftige Termine erscheinen ebenfalls im Bericht und behalten ihren Journalbeleg.
