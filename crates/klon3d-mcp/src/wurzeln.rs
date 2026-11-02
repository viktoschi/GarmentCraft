//! File access: every path in a tool call must lie under a root given at start.
//!
//! `--lesen <folder>` grants reading, `--schreiben <folder>` reading and writing. Roots and paths
//! are canonicalised (symbolic links and `..` resolved) before the comparison, so neither can
//! leave a root. A path to be written need not exist yet; its folder must.

use std::path::{Component, Path, PathBuf};

/// The roots of one server.
#[derive(Debug, Clone, Default)]
pub struct Wurzeln {
    lesen: Vec<PathBuf>,
    schreiben: Vec<PathBuf>,
}

impl Wurzeln {
    /// Canonicalise the roots; a root that does not exist is an error.
    pub fn neu(lesen: &[PathBuf], schreiben: &[PathBuf]) -> Result<Wurzeln, String> {
        let kanon = |p: &PathBuf| {
            std::fs::canonicalize(p)
                .map_err(|e| format!("Wurzel {}: {e}", p.display()))
                .and_then(|k| {
                    if k.is_dir() {
                        Ok(k)
                    } else {
                        Err(format!("Wurzel {}: kein Ordner", p.display()))
                    }
                })
        };
        Ok(Wurzeln {
            lesen: lesen.iter().map(kanon).collect::<Result<_, _>>()?,
            schreiben: schreiben.iter().map(kanon).collect::<Result<_, _>>()?,
        })
    }

    pub fn lesen(&self) -> impl Iterator<Item = &Path> {
        self.lesen.iter().map(|p| p.as_path())
    }

    pub fn schreiben(&self) -> impl Iterator<Item = &Path> {
        self.schreiben.iter().map(|p| p.as_path())
    }

    /// An existing file to read: under a read or a write root.
    pub fn zum_lesen(&self, pfad: &str) -> Result<PathBuf, String> {
        let p = Path::new(pfad);
        let k = std::fs::canonicalize(p).map_err(|e| format!("{pfad}: {e}"))?;
        if self
            .lesen
            .iter()
            .chain(&self.schreiben)
            .any(|w| k.starts_with(w))
        {
            Ok(anzeigbar(&k))
        } else {
            Err(format!(
                "{pfad}: liegt unter keiner Lese-Wurzel ({})",
                liste(self.lesen.iter().chain(&self.schreiben))
            ))
        }
    }

    /// A file to write: its folder must exist and lie under a write root; the file itself may
    /// be new. An existing file is canonicalised itself (a link pointing out of the root fails).
    pub fn zum_schreiben(&self, pfad: &str) -> Result<PathBuf, String> {
        let p = Path::new(pfad);
        let name = match p.components().next_back() {
            Some(Component::Normal(n)) => n.to_owned(),
            _ => return Err(format!("{pfad}: kein Dateiname")),
        };
        let k = if p.exists() {
            std::fs::canonicalize(p).map_err(|e| format!("{pfad}: {e}"))?
        } else {
            let ordner = match p.parent() {
                Some(o) if !o.as_os_str().is_empty() => o,
                _ => Path::new("."),
            };
            std::fs::canonicalize(ordner)
                .map_err(|e| format!("{pfad}: Ordner {}: {e}", ordner.display()))?
                .join(name)
        };
        if k.is_dir() {
            return Err(format!("{pfad}: ist ein Ordner"));
        }
        if self.schreiben.iter().any(|w| k.starts_with(w)) {
            Ok(anzeigbar(&k))
        } else {
            Err(format!(
                "{pfad}: liegt unter keiner Schreib-Wurzel ({})",
                liste(self.schreiben.iter())
            ))
        }
    }
}

fn liste<'a>(wurzeln: impl Iterator<Item = &'a PathBuf>) -> String {
    let v: Vec<String> = wurzeln
        .map(|w| anzeigbar(w).display().to_string())
        .collect();
    if v.is_empty() {
        "keine angegeben".into()
    } else {
        v.join(", ")
    }
}

/// Without Windows' verbatim prefix (`\\?\C:\…` → `C:\…`): the pattern reader joins relative
/// file names with `/`, which a verbatim path does not normalise. UNC paths keep their prefix.
pub fn anzeigbar(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => p.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordner(name: &str) -> PathBuf {
        let o = std::env::temp_dir().join(format!("klon3d-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        std::fs::create_dir_all(o.join("innen")).unwrap();
        std::fs::create_dir_all(o.join("aussen")).unwrap();
        std::fs::write(o.join("innen/a.txt"), "a").unwrap();
        std::fs::write(o.join("aussen/b.txt"), "b").unwrap();
        o
    }

    #[test]
    fn lesen_nur_unter_einer_wurzel() {
        let o = ordner("lesen");
        let w = Wurzeln::neu(&[o.join("innen")], &[]).unwrap();
        assert!(w.zum_lesen(o.join("innen/a.txt").to_str().unwrap()).is_ok());
        let e = w
            .zum_lesen(o.join("aussen/b.txt").to_str().unwrap())
            .unwrap_err();
        assert!(e.contains("keiner Lese-Wurzel"), "{e}");
        // `..` does not lead out of the root.
        let e = w
            .zum_lesen(o.join("innen/../aussen/b.txt").to_str().unwrap())
            .unwrap_err();
        assert!(e.contains("keiner Lese-Wurzel"), "{e}");
        assert!(w
            .zum_lesen(o.join("innen/fehlt.txt").to_str().unwrap())
            .is_err());
        let _ = std::fs::remove_dir_all(&o);
    }

    #[test]
    fn schreiben_neue_datei_unter_der_wurzel() {
        let o = ordner("schreiben");
        let w = Wurzeln::neu(&[], &[o.join("innen")]).unwrap();
        let p = w
            .zum_schreiben(o.join("innen/neu.obj").to_str().unwrap())
            .unwrap();
        assert!(p.ends_with("neu.obj"));
        assert!(!p.to_string_lossy().starts_with(r"\\?\"));
        for falsch in ["aussen/neu.obj", "innen/../aussen/neu.obj", "innen"] {
            assert!(
                w.zum_schreiben(o.join(falsch).to_str().unwrap()).is_err(),
                "{falsch}"
            );
        }
        // A write root is readable as well (a seam list written can be opened).
        assert!(w.zum_lesen(o.join("innen/a.txt").to_str().unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&o);
    }

    #[test]
    fn fehlende_wurzel_ist_ein_fehler() {
        let e = Wurzeln::neu(&[PathBuf::from("gibt/es/nicht/42")], &[]).unwrap_err();
        assert!(e.contains("Wurzel"), "{e}");
    }
}
