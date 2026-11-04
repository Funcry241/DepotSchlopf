//! Ein echtes Journal aus PR #15 (Stand `dfee95c`): der erste Tag von `Szenario::beispiel()`,
//! erzeugt mit dem damaligen simulierten Lauf. Es hat noch keine Auftragszustaende: Jeder
//! Auftrag ist belegt und sein Ergebnis steht im naechsten Topf. Erfundene Daten (D12).

/// Das Journal, Byte fuer Byte.
pub const JOURNAL: &str = r#"10;f69313c0a5ad6b952cdb06af976e5843664719f6fb02f2460e2961739a9a8965;journal-v1
15;c7fd614811fee9a441f20e541a5399bc5ed7678dd9db6937374b452488798c68;topf;A;1000000;
15;9d5390013d86e00cb07dc4004691abba68774e3d77c786cb5e6966b4a48159e1;topf;B;1000000;
77;65015eb38a9d8333f8fc9a0656626b8cbaab79522f396ab6751491a8c9edccd4;beobachtung;2026-11-25;Alpha Werke kuendigt Quartalszahlen fuer den 26.11. an
74;a1bb968760c7358c198ed72a670b6e522073f305bd1d3cf36189f4d05ddb9efc;beobachtung;2026-11-25;Notenbank kuendigt Zinsentscheid fuer den 27.11. an
75;1291d9ff4575a03475784caf80a56d4e1c1246a1ba52779336388a98f4efab2c;ki-grenzen;5b19e4448c65093cec8296b784dbb5b5ab6b9ed58adbe4a1c2e2adfa5250712b
24;0d42c3bb16aca79d10d80349232fe9dcc1b54b2cdac801d27cbf0f27fbcd49bc;ki;Dachs;2026-11-25;1;48
103;d2389756a1b045a1713538b8d070c94ad1626354c8f79093db1a8e24a6e6edda;begruendung;2026-11-25;Dachs;Topf A: Zwei Termine stehen an; vor dem Ausgang breit und klein einsteigen
94;a46d79016c3b3f48c446a43553634974fbf82f6b53c471eafbea6fa0be2df217;begruendung;2026-11-25;Dachs;Topf A: XA0000BETA05 2000 bp: ruhiger Wert, kein Termin betroffen
94;aff1212209430510541ebc9b01a6f83ee9702acb56f4d924340b5a166da06802;begruendung;2026-11-25;Dachs;Topf A: XA000GAMMA06 1500 bp: ruhiger Wert, kein Termin betroffen
23;a7fecdac33aeed3fa23316e76f6e7122191997f3cf279a13b4ad833e63e67123;ki;Maus;2026-11-25;1;90
92;a67e3faf87e012b0e3bc6925e234ceb769d1b94e0ee7601d8f56748ce9f472ea;begruendung;2026-11-25;Maus;Topf A: Zustimmung: breit gestreut, keine Wette auf einen Termin
50;e5485e2c192917b0214375ace1b7be3bb1209bd3cdae5c7995c6e5ee26d1db75;auftrag;1;A;XA0000BETA05;Kauf;25;8060;Umschichtung
123;dcf944ac6129f26f14a5351fa398afb75e6b17744a057faa3855de96c95d8c9f;begruendung;2026-11-25;Simulation;Topf A: Auftrag 1 ausgefuehrt: Kauf 25 Stueck XA0000BETA05 zu 7980 Cent, Gebuehr 150 Cent
50;090829c67b57b7b1dbc25c1e857e6626193d29b093e2197ed7f4950a72189395;auftrag;2;A;XA000GAMMA06;Kauf;74;2026;Umschichtung
123;03967cce34b655240a9f5e2c875b3b4c058113a9a4e8b269d0694e9d12a85184;begruendung;2026-11-25;Simulation;Topf A: Auftrag 2 ausgefuehrt: Kauf 74 Stueck XA000GAMMA06 zu 2005 Cent, Gebuehr 150 Cent
45;d9bcc914395c4f308a2034f137596e09820d0097ef329820e4e5cca21f7789d6;topf;A;651830;XA0000BETA05:25,XA000GAMMA06:74
23;ade0796425172814fcab0488d580815fe26c83f6bcda12105eed326d08c699a6;ki;Maus;2026-11-25;1;90
87;63947916b912bcf14eddbe6dada11eb58c83e4df149c34ada9cabf08e90ad1e6;begruendung;2026-11-25;Maus;Topf B: Vor Zinsentscheid und Zahlen nur ein kleiner Anfang
79;d5e5c835f9708a7ffa1f46e27658b9082601e53f938cbe8d7f95f3d2f9024af7;begruendung;2026-11-25;Maus;Topf B: XA0000BETA05 1000 bp: kein Termin betroffen
24;4cb0a881c4143e6e0e7853e5f98488a3ec9c831a4c7b1756c1ac842df0e3b940;ki;Dachs;2026-11-25;1;48
84;efec5112f160160391846ea0390a85474fea950a136b24f947f7886b4afcba4a;begruendung;2026-11-25;Dachs;Topf B: Zustimmung: Streuung und kleine Schritte passen
50;9c15238c6187f861bfdfb3d83f925a3e3da3e4dc1b58416b2f5f6848a17c552c;auftrag;3;B;XA0000BETA05;Kauf;12;8060;Umschichtung
123;55ad7416f4c7a86d095049ea8852e7e41810a777087ffe1923c0da94d2c7d4b0;begruendung;2026-11-25;Simulation;Topf B: Auftrag 3 ausgefuehrt: Kauf 12 Stueck XA0000BETA05 zu 7980 Cent, Gebuehr 150 Cent
29;13ccba62c7c1cbaebdf821b6702d819c2ae8ecb2cb2ba9e1d4de80e7706a259c;topf;B;904090;XA0000BETA05:12
93;8c212fe4fc9171674c7072b133cac63481cd1583905b6f8ba66c437c16bb641e;abschluss;2026-11-25;2000000;4b25d119332e268513a68bfdea96f32e83ce195fe17d37b372056b6a3605a8a6
"#;

/// Der Anhangsvermerk dazu.
pub const VERMERK: &str = r"vermerk-v1;3477;8c212fe4fc9171674c7072b133cac63481cd1583905b6f8ba66c437c16bb641e;0
";
