//! Kleiner ZIP-Schreiber ohne Kompression (Methode "stored"), ohne fremde Abhaengigkeit.

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
    /// Datei oder Archiv zu gross (ueber 4 GB) oder zu viele Eintraege.
    pub fn datei(&mut self, name: &str, daten: &[u8]) -> Result<(), String> {
        let zu_gross = || format!("{name}: zu gross fuer ZIP ohne ZIP64");
        let groesse = u32::try_from(daten.len()).map_err(|_| zu_gross())?;
        let name_len = u16::try_from(name.len()).map_err(|_| zu_gross())?;
        let versatz = u32::try_from(self.puffer.len()).map_err(|_| zu_gross())?;
        let crc = crc32(daten);

        let p = &mut self.puffer;
        u32_le(p, 0x0403_4b50);
        u16_le(p, 20);
        u16_le(p, UTF8);
        u16_le(p, 0);
        u16_le(p, 0);
        u16_le(p, DATUM_1980);
        u32_le(p, crc);
        u32_le(p, groesse);
        u32_le(p, groesse);
        u16_le(p, name_len);
        u16_le(p, 0);
        p.extend_from_slice(name.as_bytes());
        p.extend_from_slice(daten);

        let c = &mut self.verzeichnis;
        u32_le(c, 0x0201_4b50);
        u16_le(c, 20);
        u16_le(c, 20);
        u16_le(c, UTF8);
        u16_le(c, 0);
        u16_le(c, 0);
        u16_le(c, DATUM_1980);
        u32_le(c, crc);
        u32_le(c, groesse);
        u32_le(c, groesse);
        u16_le(c, name_len);
        u16_le(c, 0);
        u16_le(c, 0);
        u16_le(c, 0);
        u16_le(c, 0);
        u32_le(c, 0);
        u32_le(c, versatz);
        c.extend_from_slice(name.as_bytes());

        self.anzahl = self.anzahl.checked_add(1).ok_or("zu viele Eintraege")?;
        Ok(())
    }

    /// # Errors
    /// Archiv zu gross.
    pub fn fertig(mut self) -> Result<Vec<u8>, String> {
        let versatz = u32::try_from(self.puffer.len()).map_err(|_| "Archiv zu gross")?;
        let laenge = u32::try_from(self.verzeichnis.len()).map_err(|_| "Archiv zu gross")?;
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
}
