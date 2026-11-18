//! Absichtlich roter Test für die CI-Gegenprobe zu P38.
#[test]
fn p38_absichtlich_roter_rust_test() -> Result<(), std::fmt::Arguments<'static>> {
    Err(format_args!(
        "P38-Gegenprobe: erste Fehlerzeile\nZweite Zeile bleibt im Vollprotokoll"
    ))
}
