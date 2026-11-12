//! ZIP-Schreiber und Ausgabegrenzen für lokale Exporte.

#![forbid(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::float_arithmetic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

/// CRC-32 nach ISO 3309, wie ZIP sie verlangt.
pub fn crc32(daten: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in daten {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let maske = (crc & 1).wrapping_neg();
            crc = crc.wrapping_shr(1) ^ (0xEDB8_8320 & maske);
        }
    }
    !crc
}

#[derive(Debug, Default)]
pub struct Zip {
    puffer: Vec<u8>,
    verzeichnis: Vec<u8>,
    anzahl: u16,
}

const DATUM_1980: u16 = 0x0021;
const UTF8: u16 = 0x0800;

fn u16_le(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn u32_le(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

impl Zip {
    /// # Errors
    /// Datei oder Archiv zu gross (ueber 4 GB) oder zu viele Einträge.
    pub fn datei(&mut self, name: &str, daten: &[u8]) -> Result<(), String> {
        self.eintrag(name, daten, daten, 0)
    }

    /// Schreibt einen Eintrag mit Deflate (ZIP-Methode 8).
    pub fn datei_gepackt(&mut self, name: &str, daten: &[u8]) -> Result<(), String> {
        let gepackt = miniz_oxide::deflate::compress_to_vec(daten, 6);
        self.eintrag(name, daten, &gepackt, 8)
    }

    fn eintrag(
        &mut self,
        name: &str,
        daten: &[u8],
        gepackt: &[u8],
        methode: u16,
    ) -> Result<(), String> {
        let zu_gross = || format!("{name}: zu groß für ZIP ohne ZIP64");
        let groesse = u32::try_from(daten.len()).map_err(|_| zu_gross())?;
        let gepackte_groesse = u32::try_from(gepackt.len()).map_err(|_| zu_gross())?;
        let name_len = u16::try_from(name.len()).map_err(|_| zu_gross())?;
        let versatz = u32::try_from(self.puffer.len()).map_err(|_| zu_gross())?;
        let crc = crc32(daten);

        let p = &mut self.puffer;
        u32_le(p, 0x0403_4b50);
        u16_le(p, 20);
        u16_le(p, UTF8);
        u16_le(p, methode);
        u16_le(p, 0);
        u16_le(p, DATUM_1980);
        u32_le(p, crc);
        u32_le(p, gepackte_groesse);
        u32_le(p, groesse);
        u16_le(p, name_len);
        u16_le(p, 0);
        p.extend_from_slice(name.as_bytes());
        p.extend_from_slice(gepackt);

        let c = &mut self.verzeichnis;
        u32_le(c, 0x0201_4b50);
        u16_le(c, 20);
        u16_le(c, 20);
        u16_le(c, UTF8);
        u16_le(c, methode);
        u16_le(c, 0);
        u16_le(c, DATUM_1980);
        u32_le(c, crc);
        u32_le(c, gepackte_groesse);
        u32_le(c, groesse);
        u16_le(c, name_len);
        u16_le(c, 0);
        u16_le(c, 0);
        u16_le(c, 0);
        u16_le(c, 0);
        u32_le(c, 0);
        u32_le(c, versatz);
        c.extend_from_slice(name.as_bytes());

        self.anzahl = self.anzahl.checked_add(1).ok_or("zu viele Einträge")?;
        Ok(())
    }

    /// # Errors
    /// Archiv zu gross.
    pub fn fertig(mut self) -> Result<Vec<u8>, String> {
        let versatz = u32::try_from(self.puffer.len()).map_err(|_| "Archiv zu groß")?;
        let laenge = u32::try_from(self.verzeichnis.len()).map_err(|_| "Archiv zu groß")?;
        self.puffer.append(&mut self.verzeichnis);
        let p = &mut self.puffer;
        u32_le(p, 0x0605_4b50);
        u16_le(p, 0);
        u16_le(p, 0);
        u16_le(p, self.anzahl);
        u16_le(p, self.anzahl);
        u32_le(p, laenge);
        u32_le(p, versatz);
        u16_le(p, 0);
        Ok(self.puffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_bekannter_wert() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn deflate_eintrag_hat_gueltige_zip_felder() -> Result<(), String> {
        let inhalt = b"DepotSchlopf Papierdepot";
        let name = "Übersicht.md";
        let mut zip = Zip::default();
        zip.datei_gepackt(name, inhalt)?;
        let bytes = zip.fertig()?;
        assert_eq!(bytes.get(8..10), Some([8, 0].as_slice()));
        assert_eq!(
            bytes.get(14..18),
            Some(crc32(inhalt).to_le_bytes().as_slice())
        );
        let laenge = bytes.get(18..22).ok_or("Länge fehlt")?;
        let laenge = u32::from_le_bytes(laenge.try_into().map_err(|_| "Länge unlesbar")?);
        let start = 30_usize.saturating_add(name.len());
        let ende = start.saturating_add(usize::try_from(laenge).map_err(|_| "Länge zu groß")?);
        let daten = bytes.get(start..ende).ok_or("Deflate-Daten fehlen")?;
        let entpackt = miniz_oxide::inflate::decompress_to_vec(daten)
            .map_err(|e| format!("Deflate: {e:?}"))?;
        assert_eq!(entpackt, inhalt);
        Ok(())
    }
}
