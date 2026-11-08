//! Private Umgebung des vollständigen Testlaufs. Der echte Dienst und die persönliche
//! Git-Konfiguration gehören nicht zur Testfixture. Keine globale Prozessumgebung ändern.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub(super) struct Testumgebung {
    ordner: PathBuf,
}

impl Testumgebung {
    pub(super) fn neu() -> Result<Self, String> {
        for nr in 0..100_u32 {
            let ordner = std::env::temp_dir().join(format!("ds-test-{}-{nr}", std::process::id()));
            match fs::create_dir(&ordner) {
                Ok(()) => {
                    let umgebung = Self { ordner };
                    fs::write(
                        umgebung.ordner.join("gitconfig"),
                        concat!(
                            "[user]\nname = DepotSchlopf-Test\nemail = test@depotschlopf.invalid\n",
                            "[core]\nautocrlf = false\neol = lf\n",
                            "[protocol \"file\"]\nallow = always\n"
                        ),
                    )
                    .map_err(|e| format!("Git-Testkonfiguration nicht schreibbar: {e}"))?;
                    return Ok(umgebung);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("Private Testumgebung nicht anlegbar: {e}")),
            }
        }
        Err("Kein freier Ordner für die private Testumgebung.".into())
    }

    pub(super) fn anwenden(&self, befehl: &mut Command) {
        // Auch schon am Command gesetzte Werte entfernen; keine ungewollten Zusatzparameter.
        let git_variablen: Vec<_> = std::env::vars_os()
            .map(|(k, _)| k)
            .chain(befehl.get_envs().map(|(k, _)| k.to_os_string()))
            .filter(|k| k.to_string_lossy().to_ascii_uppercase().starts_with("GIT_"))
            .collect();
        for k in git_variablen {
            befehl.env_remove(k);
        }
        for k in ["TEMP", "TMP", "TMPDIR"] {
            befehl.env(k, &self.ordner);
        }
        befehl
            .env("GIT_CONFIG_GLOBAL", self.ordner.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_COUNT", "0")
            .env("GIT_ALLOW_PROTOCOL", "file");
    }
}

impl Drop for Testumgebung {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_dir_all(&self.ordner) {
            eprintln!(
                "[Kodex] Temporäre Testumgebung konnte nicht vollständig entfernt werden: {e}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ausgabe(mut befehl: Command) -> Result<String, String> {
        let aus = befehl.output().map_err(|e| e.to_string())?;
        if !aus.status.success() {
            return Err(String::from_utf8_lossy(&aus.stderr).into_owned());
        }
        Ok(String::from_utf8_lossy(&aus.stdout).trim().to_owned())
    }

    #[test]
    fn persoenliche_git_einstellungen_und_umleitungen_gelten_nicht_im_test() -> Result<(), String> {
        let umgebung = Testumgebung::neu()?;
        let fremd = umgebung.ordner.join("fremd.gitconfig");
        fs::write(
            &fremd,
            "[core]\neol = crlf\n[url \"ssh://ungueltig.invalid/\"]\ninsteadOf = ../\n",
        )
        .map_err(|e| e.to_string())?;
        let mut git = Command::new("git");
        git.args(["config", "--global", "--get", "core.eol"])
            .env("GIT_CONFIG_GLOBAL", &fremd)
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "core.eol")
            .env("GIT_CONFIG_VALUE_0", "crlf")
            .env("GIT_DIR", "ungueltiger-ordner");
        umgebung.anwenden(&mut git);
        assert_eq!(ausgabe(git)?, "lf");
        assert_eq!(
            fs::read_to_string(fremd).map_err(|e| e.to_string())?,
            "[core]\neol = crlf\n[url \"ssh://ungueltig.invalid/\"]\ninsteadOf = ../\n"
        );
        Ok(())
    }

    #[test]
    fn testordner_sind_eigenstaendig_und_werden_aufgeraeumt() -> Result<(), String> {
        let erste = Testumgebung::neu()?;
        let zweite = Testumgebung::neu()?;
        assert_ne!(erste.ordner, zweite.ordner);
        assert_ne!(erste.ordner, std::env::temp_dir());
        let pfad = erste.ordner.clone();
        let mut cargo = Command::new("cargo");
        erste.anwenden(&mut cargo);
        let env: std::collections::BTreeMap<_, _> = cargo
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|s| s.to_string_lossy().into_owned()),
                )
            })
            .collect();
        for k in ["TEMP", "TMP", "TMPDIR"] {
            assert_eq!(env.get(k).and_then(Option::as_deref), pfad.to_str());
        }
        assert_eq!(
            env.get("GIT_ALLOW_PROTOCOL").and_then(Option::as_deref),
            Some("file")
        );
        drop(erste);
        assert!(!pfad.exists());
        assert!(zweite.ordner.exists());
        Ok(())
    }

    #[test]
    fn unbeabsichtigte_git_netzverbindung_ist_im_test_gesperrt() -> Result<(), String> {
        let umgebung = Testumgebung::neu()?;
        let mut git = Command::new("git");
        git.args(["ls-remote", "https://ungueltig.invalid/nicht-abrufen.git"]);
        umgebung.anwenden(&mut git);
        let aus = git.output().map_err(|e| e.to_string())?;
        assert!(!aus.status.success());
        assert!(String::from_utf8_lossy(&aus.stderr).contains("transport 'https' not allowed"));
        Ok(())
    }
}
