//! The stepped scene of `simulate`: built once from the pattern shown, then advanced a given
//! number of frames per call through [`ablauf::Anziehvorgang`] (sew, settle, hold).

use std::path::{Path, PathBuf};
use std::time::Instant;

use klon3d_pipeline::ablauf::{Anziehvorgang, Phase};
use klon3d_pipeline::{Koerperquelle, Musternetz, Szene};
use klon3d_sim::platzierung::Anziehen;
use klon3d_sim::{ausgabe, messung, ImplizitCg, ImplizitParameter, Material};
use serde_json::{json, Value};

/// What the scene is built from.
pub struct Vorgaben {
    pub muster: PathBuf,
    pub massdatei: PathBuf,
    pub nahtliste: PathBuf,
    pub lage: PathBuf,
    /// `.smis` to shape the body from, or a `.zprj` with a body.
    pub koerper: PathBuf,
    pub material: Material,
}

pub struct Simulation {
    pub szene: Szene,
    rw: ImplizitCg,
    vorgang: Anziehvorgang<ImplizitCg>,
    /// Points of the mesh before placement (all pieces) and of the placed pieces.
    netz_punkte: usize,
    platziert_punkte: usize,
    weggelassen: Vec<String>,
    herkunft: String,
    material: &'static str,
    aufbau_s: f64,
    rechenzeit_s: f64,
}

impl Simulation {
    /// Mesh, body, placement, scene; no frame yet.
    pub fn aufbauen(v: &Vorgaben) -> Result<Simulation, String> {
        let start = Instant::now();
        let ordner = klon3d_koerper::koerper::ordner();
        if !ordner.is_dir() {
            return Err(format!(
                "Körperordner fehlt: {} (Umgebungsvariable CLO_FEMALE)",
                ordner.display()
            ));
        }
        let fehler = |e: klon3d_pipeline::Fehler| e.to_string();
        let m = klon3d_pipeline::Muster::laden(&v.muster, Some(&v.massdatei)).map_err(fehler)?;
        let defs = klon3d_pipeline::nahtliste(&v.nahtliste).map_err(fehler)?;
        let netz: Musternetz = m
            .netz(&defs, &klon3d_mesh::Parameter::default())
            .map_err(fehler)?;
        let lage = klon3d_pipeline::lageliste(&v.lage).map_err(fehler)?;
        let quelle = if ist_projekt(&v.koerper) {
            Koerperquelle::CloProjekt(&v.koerper, ordner)
        } else {
            Koerperquelle::MassdateiMitOrdner(&v.koerper, ordner)
        };
        let k = klon3d_pipeline::koerper(quelle).map_err(fehler)?;
        // As `klon3d_pipeline::szene`, unrolled to keep the placement for the point count.
        let h = klon3d_pipeline::huellen(&k).map_err(fehler)?;
        let p = klon3d_pipeline::platzieren(&netz, &lage, &h, true)
            .map_err(|e| format!("{}: {e}", v.lage.display()))?;
        let mut szene = klon3d_pipeline::szene_aus(
            &p,
            &k,
            v.material,
            0.0,
            klon3d_pipeline::standard_simparameter(),
        );
        let zonen = p.klebungen(&szene).into_iter().map(|(z, _)| z).collect();
        szene.klebungen_setzen(zonen);
        let rw = ImplizitCg::neu(ImplizitParameter::default());
        let vorgang = Anziehvorgang::beginnen(&mut szene, &rw, &Anziehen::default());
        Ok(Simulation {
            netz_punkte: netz.teile.iter().map(|t| t.punkte.len()).sum(),
            platziert_punkte: p.netz.teile.iter().map(|t| t.punkte.len()).sum(),
            weggelassen: p.weggelassen.clone(),
            szene,
            rw,
            vorgang,
            herkunft: k.herkunft,
            material: v.material.name,
            aufbau_s: start.elapsed().as_secs_f64(),
            rechenzeit_s: 0.0,
        })
    }

    /// Up to `bilder` frames; fewer when the run ends. Returns the frames computed.
    pub fn schritte(&mut self, bilder: usize) -> usize {
        let start = Instant::now();
        let mut n = 0;
        while n < bilder {
            if self
                .vorgang
                .schritt(&mut self.szene, &mut self.rw)
                .is_none()
            {
                break;
            }
            n += 1;
        }
        self.rechenzeit_s += start.elapsed().as_secs_f64();
        n
    }

    pub fn obj(&self) -> String {
        ausgabe::obj(&self.szene)
    }

    pub fn status(&self) -> Value {
        let s = &self.szene;
        let bilder = self.vorgang.bild();
        json!({
            "szene": true,
            "bilder": bilder,
            "phase": phase_name(self.vorgang.phase()),
            "fertig": self.vorgang.phase() == Phase::Fertig,
            "zeit_s": s.zeit,
            "punkte": s.x.len(),
            "dreiecke": s.dreiecke.len(),
            "teile": s.teile.len(),
            "netz_punkte": self.netz_punkte,
            "platziert_punkte": self.platziert_punkte,
            "weggelassen": self.weggelassen,
            "nahtpaare": s.naht.len(),
            "groesster_nahtspalt_mm": messung::nahtspalt(s) * 1e3,
            "koerper": self.herkunft,
            "material": self.material,
            "aufbau_s": self.aufbau_s,
            "rechenzeit_s": self.rechenzeit_s,
            "ms_je_bild": if bilder > 0 { Some(self.rechenzeit_s * 1e3 / bilder as f64) } else { None },
        })
    }
}

fn ist_projekt(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zprj"))
}

fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Naehen => "naehen",
        Phase::Einschwingen => "einschwingen",
        Phase::Halt => "halt",
        Phase::Fertig => "fertig",
    }
}

/// The status of no scene.
pub fn keine() -> Value {
    json!({ "szene": false })
}
