//! The headless session: the application inside an `egui_kittest` harness (as
//! `klon3d-app/examples/bildschirmfoto.rs` builds it), the file roots and the stepped scene.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use eframe::egui::{self, Theme};
use egui_kittest::Harness;
use klon3d_app::anordnung::Fenster;
use klon3d_app::app::Aktion;
use klon3d_app::bibliothek::Bibliothek;
use klon3d_app::muster::{
    delta4, naehte_vorschlagen, naht_vermerk, nahtliste_neben, Ladeauftrag, Nahtstand,
};
use klon3d_app::panele::Reiter;
use klon3d_app::App;
use serde_json::{json, Map, Value};

use crate::simulation::{self, Simulation, Vorgaben};
use crate::vorschau::{panik_text, Vorschau};
use crate::werkzeuge::{self, Befehl, Fehler};
use crate::wurzeln::{anzeigbar, Wurzeln};

/// The window as the app opens it.
pub const BREITE: f32 = 1400.0;
pub const HOEHE: f32 = 900.0;
/// Largest picture side in pixels.
const MAX_PIXEL: f32 = 8192.0;

/// Result of a tool: JSON (`structuredContent`, also as text) and maybe a PNG in base64.
pub struct Antwort {
    pub json: Value,
    pub png_base64: Option<String>,
}

impl From<Value> for Antwort {
    fn from(json: Value) -> Self {
        Antwort {
            json,
            png_base64: None,
        }
    }
}

pub struct Sitzung {
    fenster: Harness<'static, App>,
    wurzeln: Wurzeln,
    sim: Option<Simulation>,
}

impl Sitzung {
    pub fn neu(wurzeln: Wurzeln) -> Sitzung {
        let mut fenster: Harness<'static, App> = Harness::builder()
            .with_size([BREITE, HOEHE])
            .with_pixels_per_point(1.0)
            .renderer(Vorschau::default())
            .build_eframe(|cc| App::neu(cc));
        // The library starts in the working folder; show a root instead, so the pictures list
        // no folder outside the roots. Without a root the library is hidden.
        let app = fenster.state_mut();
        match wurzeln.lesen().chain(wurzeln.schreiben()).next() {
            Some(w) => {
                let w = anzeigbar(w);
                app.bibliothek = Bibliothek::neu(&w);
                app.letzter_ordner = w;
            }
            None => app.anordnung.bibliothek = false,
        }
        let _ = fenster.run_ok();
        Sitzung {
            fenster,
            wurzeln,
            sim: None,
        }
    }

    /// Run a tool. A panic inside becomes a tool error; the session goes on.
    pub fn aufrufen(&mut self, name: &str, args: &Map<String, Value>) -> Result<Antwort, Fehler> {
        if !werkzeuge::WERKZEUGE.iter().any(|w| w.name == name) {
            return Err(Fehler::Parameter(format!("unbekanntes Werkzeug {name:?}")));
        }
        match catch_unwind(AssertUnwindSafe(|| self.ausfuehren(name, args))) {
            Ok(r) => r,
            Err(p) => {
                if name == "simulate" {
                    self.sim = None;
                }
                Err(Fehler::Werkzeug(format!(
                    "{name}: interner Fehler: {}",
                    panik_text(&*p)
                )))
            }
        }
    }

    fn ausfuehren(&mut self, name: &str, args: &Map<String, Value>) -> Result<Antwort, Fehler> {
        match name {
            "open" => self.oeffnen(args),
            "doc_inspect" => Ok(self.zustand().into()),
            "command_list" => Ok(werkzeuge::kommandos().into()),
            "command_run" => self.kommando(args),
            "render_preview" => self.vorschau(args),
            "suggest_seams" => self.naehte_vorschlagen(args),
            "mesh_check" => self.netz_pruefen(),
            "simulate" => self.simulieren(args),
            "sim_status" => Ok(self.sim_status().into()),
            "sim_reset" => {
                let war = self.sim.take().is_some();
                Ok(json!({ "verworfen": war, "simulation": simulation::keine() }).into())
            }
            "export_obj" => self.obj_schreiben(args),
            _ => Err(Fehler::Parameter(format!("unbekanntes Werkzeug {name:?}"))),
        }
    }

    fn app(&self) -> &App {
        self.fenster.state()
    }

    /// One frame for queued actions, then until the window is still.
    fn bilder(&mut self) {
        self.fenster.step();
        let _ = self.fenster.run_ok();
    }

    fn lesepfad(&self, args: &Map<String, Value>, name: &str) -> Result<Option<PathBuf>, Fehler> {
        match text(args, name)? {
            Some(p) => self
                .wurzeln
                .zum_lesen(p)
                .map(Some)
                .map_err(Fehler::Werkzeug),
            None => Ok(None),
        }
    }

    fn oeffnen(&mut self, args: &Map<String, Value>) -> Result<Antwort, Fehler> {
        let Some(muster) = text(args, "muster")? else {
            return Err(Fehler::Parameter("open: 'muster' fehlt".into()));
        };
        let muster = self.wurzeln.zum_lesen(muster).map_err(Fehler::Werkzeug)?;
        let auftrag = Ladeauftrag {
            muster,
            massdatei: self.lesepfad(args, "massdatei")?,
            nahtliste: self.lesepfad(args, "nahtliste")?,
        };
        let geladen = self.fenster.state_mut().laden(auftrag);
        self.bilder();
        if !geladen {
            return Err(Fehler::Werkzeug(self.app().meldung.clone()));
        }
        // Another pattern: the scene of the previous one no longer fits.
        self.sim = None;
        Ok(self.zustand().into())
    }

    fn kommando(&mut self, args: &Map<String, Value>) -> Result<Antwort, Fehler> {
        let Some(id) = text(args, "id")? else {
            return Err(Fehler::Parameter("command_run: 'id' fehlt".into()));
        };
        let leer = Map::new();
        let params = match args.get("params") {
            None | Some(Value::Null) => &leer,
            Some(Value::Object(p)) => p,
            Some(_) => {
                return Err(Fehler::Parameter(
                    "command_run: 'params' muss ein Objekt sein".into(),
                ))
            }
        };
        let befehl = werkzeuge::befehl(id, params)?;
        let app = self.fenster.state_mut();
        let teile = app.muster.as_ref().map_or(0, |m| m.teile.len());
        let naehte = app
            .muster
            .as_ref()
            .and_then(|m| m.naehte.bericht())
            .map_or(0, |b| b.naehte.len());
        match befehl {
            Befehl::Aktion(Aktion::TeilWaehlen(i)) if i >= teile => {
                return Err(Fehler::Werkzeug(format!(
                    "teil_waehlen: Index {i} außerhalb 0..{teile}"
                )))
            }
            Befehl::Aktion(Aktion::NahtWaehlen(i)) if i >= naehte => {
                return Err(Fehler::Werkzeug(format!(
                    "naht_waehlen: Index {i} außerhalb 0..{naehte}"
                )))
            }
            Befehl::Aktion(a) => app.aktion(a),
            Befehl::DialogeSchliessen => {
                app.dateiwahl = None;
                app.fehlerfenster = None;
                app.hilfe = None;
                app.informationen = false;
            }
        }
        self.bilder();
        Ok(self.zustand().into())
    }

    fn vorschau(&mut self, args: &Map<String, Value>) -> Result<Antwort, Fehler> {
        let breite = zahl(args, "breite")?.unwrap_or(BREITE as f64) as f32;
        let hoehe = zahl(args, "hoehe")?.unwrap_or(HOEHE as f64) as f32;
        let massstab = zahl(args, "massstab")?.unwrap_or(1.0) as f32;
        if breite * massstab > MAX_PIXEL || hoehe * massstab > MAX_PIXEL {
            return Err(Fehler::Parameter(format!(
                "render_preview: höchstens {MAX_PIXEL} Pixel je Seite"
            )));
        }
        let ziel = match text(args, "pfad")? {
            Some(p) => Some(self.wurzeln.zum_schreiben(p).map_err(Fehler::Werkzeug)?),
            None => None,
        };
        self.fenster.set_size(egui::vec2(breite, hoehe));
        self.fenster.set_pixels_per_point(massstab);
        // Two passes: the first lays the panels out, the second draws with their sizes.
        self.bilder();
        self.bilder();
        let bild = self.fenster.render().map_err(Fehler::Werkzeug)?;
        let mut png = Vec::new();
        bild.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| Fehler::Werkzeug(format!("PNG: {e}")))?;
        if let Some(z) = &ziel {
            std::fs::write(z, &png)
                .map_err(|e| Fehler::Werkzeug(format!("{}: {e}", z.display())))?;
        }
        Ok(Antwort {
            json: json!({
                "breite_px": bild.width(),
                "hoehe_px": bild.height(),
                "bytes": png.len(),
                "pfad": ziel.map(|z| z.display().to_string()),
            }),
            png_base64: Some(crate::base64::kodieren(&png)),
        })
    }

    fn naehte_vorschlagen(&mut self, args: &Map<String, Value>) -> Result<Antwort, Fehler> {
        let ziel = match text(args, "schreiben")? {
            Some(p) => Some(self.wurzeln.zum_schreiben(p).map_err(Fehler::Werkzeug)?),
            None => None,
        };
        let Some(m) = &self.app().muster else {
            return Err(Fehler::Werkzeug("erst ein Muster öffnen (open)".into()));
        };
        let v = naehte_vorschlagen(m).map_err(Fehler::Werkzeug)?;
        let mut geladen = None;
        if let Some(z) = &ziel {
            std::fs::write(z, &v.text)
                .map_err(|e| Fehler::Werkzeug(format!("{}: {e}", z.display())))?;
            if let Some(mut auftrag) = self.app().auftrag.clone() {
                auftrag.nahtliste = Some(z.clone());
                geladen = Some(self.fenster.state_mut().laden(auftrag));
                self.bilder();
                self.sim = None;
            }
        }
        Ok(json!({
            "naehte": v.naehte,
            "text": v.text,
            "geschrieben": ziel.map(|z| z.display().to_string()),
            "geladen": geladen,
            "meldung": self.app().meldung,
        })
        .into())
    }

    /// Pattern, measurement file and checked seam list of the pattern shown.
    fn dateien(&self) -> Result<(PathBuf, PathBuf, PathBuf), Fehler> {
        let Some(m) = &self.app().muster else {
            return Err(Fehler::Werkzeug("erst ein Muster öffnen (open)".into()));
        };
        let naehte = match &m.naehte {
            Nahtstand::Geprueft { pfad, .. } => pfad.clone(),
            Nahtstand::Keine { gesucht } => {
                return Err(Fehler::Werkzeug(format!(
                    "keine Nahtliste ({} fehlt) – suggest_seams mit 'schreiben' oder open mit 'nahtliste'",
                    gesucht.display()
                )))
            }
            Nahtstand::Fehler { text, .. } => {
                return Err(Fehler::Werkzeug(format!("Nahtliste fehlerhaft: {text}")))
            }
        };
        Ok((m.pfad.clone(), m.massdatei.clone(), naehte))
    }

    fn netz_pruefen(&mut self) -> Result<Antwort, Fehler> {
        let (muster, mass, naehte) = self.dateien()?;
        let f = |e: klon3d_pipeline::Fehler| Fehler::Werkzeug(e.to_string());
        let m = klon3d_pipeline::Muster::laden(&muster, Some(&mass)).map_err(f)?;
        let defs = klon3d_pipeline::nahtliste(&naehte).map_err(f)?;
        let p = klon3d_mesh::Parameter::default();
        let netz = m.netz(&defs, &p).map_err(f)?;
        let bericht = klon3d_mesh::pruefung::pruefe(&netz);
        let teile: Vec<Value> = bericht
            .teile
            .iter()
            .map(|t| {
                json!({
                    "teil": t.teil,
                    "punkte": t.punkte,
                    "randpunkte": t.randpunkte,
                    "dreiecke": t.dreiecke,
                    "min_winkel_grad": t.min_winkel,
                    "unter_winkel": t.unter_winkel,
                    "kante_min_mm": t.kante_min,
                    "kante_median_mm": t.kante_median,
                    "kante_max_mm": t.kante_max,
                })
            })
            .collect();
        let versatz = bericht
            .naehte
            .iter()
            .map(|n| n.versatz_max)
            .fold(0.0, f64::max);
        Ok(json!({
            "teile": teile,
            "punkte": netz.teile.iter().map(|t| t.punkte.len()).sum::<usize>(),
            "dreiecke": netz.teile.iter().map(|t| t.dreiecke.len()).sum::<usize>(),
            "naehte": bericht.naehte.len(),
            "groesster_nahtversatz_mm": versatz,
            "nicht_vernetzt": netz.fehler,
            "verstoesse": bericht.verstoesse(&p),
        })
        .into())
    }

    fn simulieren(&mut self, args: &Map<String, Value>) -> Result<Antwort, Fehler> {
        let bilder = match args.get("bilder").and_then(Value::as_u64) {
            Some(b) if (1..=100_000).contains(&b) => b as usize,
            _ => {
                return Err(Fehler::Parameter(
                    "simulate: 'bilder' (ganze Zahl 1 bis 100000) fehlt".into(),
                ))
            }
        };
        let koerper = self.lesepfad(args, "koerper")?;
        let lage = self.lesepfad(args, "lage")?;
        let material = match text(args, "material")? {
            Some(n) => Some(klon3d_sim::Material::nach_namen(n).ok_or_else(|| {
                Fehler::Parameter(format!(
                    "simulate: unbekannter Stoff {n:?}, erlaubt: radical, retifist"
                ))
            })?),
            None => None,
        };
        if self.sim.is_none() {
            let (muster, massdatei, nahtliste) = self.dateien()?;
            let lage = match lage {
                Some(l) => l,
                None => {
                    let l = muster.with_extension("lage");
                    if !l.is_file() {
                        return Err(Fehler::Werkzeug(format!(
                            "keine Lageliste: {} fehlt – 'lage' angeben",
                            l.display()
                        )));
                    }
                    l
                }
            };
            let v = Vorgaben {
                koerper: koerper.unwrap_or_else(|| massdatei.clone()),
                muster,
                massdatei,
                nahtliste,
                lage,
                material: material.unwrap_or(klon3d_sim::RADICAL_RUBBER_04),
            };
            eprintln!("klon3d-mcp: Szene wird aufgebaut ({})", v.muster.display());
            self.sim = Some(Simulation::aufbauen(&v).map_err(Fehler::Werkzeug)?);
        } else if koerper.is_some() || lage.is_some() || material.is_some() {
            return Err(Fehler::Werkzeug(
                "die Szene besteht schon – 'koerper', 'lage' und 'material' gelten erst nach sim_reset"
                    .into(),
            ));
        }
        let sim = self.sim.as_mut().expect("built above");
        let n = sim.schritte(bilder);
        let mut s = sim.status();
        s["bilder_dieser_aufruf"] = json!(n);
        Ok(s.into())
    }

    fn sim_status(&self) -> Value {
        self.sim
            .as_ref()
            .map_or_else(simulation::keine, Simulation::status)
    }

    fn obj_schreiben(&mut self, args: &Map<String, Value>) -> Result<Antwort, Fehler> {
        let Some(pfad) = text(args, "pfad")? else {
            return Err(Fehler::Parameter("export_obj: 'pfad' fehlt".into()));
        };
        let ziel = self.wurzeln.zum_schreiben(pfad).map_err(Fehler::Werkzeug)?;
        let Some(sim) = &self.sim else {
            return Err(Fehler::Werkzeug("keine Szene – erst simulate".into()));
        };
        let obj = sim.obj();
        std::fs::write(&ziel, &obj)
            .map_err(|e| Fehler::Werkzeug(format!("{}: {e}", ziel.display())))?;
        Ok(json!({
            "pfad": ziel.display().to_string(),
            "bytes": obj.len(),
            "punkte": sim.szene.x.len(),
            "dreiecke": sim.szene.dreiecke.len(),
        })
        .into())
    }

    /// `doc_inspect`.
    pub fn zustand(&self) -> Value {
        let app = self.app();
        let muster = app.muster.as_ref().map(|m| {
            let teile: Vec<Value> = m
                .teile
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    json!({
                        "index": i,
                        "name": t.name,
                        "umfang_cm": t.umfang_cm,
                        "knoten": t.knoten,
                    })
                })
                .collect();
            let naehte = match &m.naehte {
                Nahtstand::Keine { gesucht } => json!({
                    "stand": "keine",
                    "gesucht": gesucht.display().to_string(),
                }),
                Nahtstand::Fehler { pfad, text } => json!({
                    "stand": "fehler",
                    "pfad": pfad.display().to_string(),
                    "text": text,
                }),
                Nahtstand::Geprueft { pfad, bericht } => {
                    let liste: Vec<Value> = bericht
                        .naehte
                        .iter()
                        .enumerate()
                        .map(|(i, n)| {
                            let vermerk = naht_vermerk(n);
                            json!({
                                "index": i,
                                "kennung": n.kennung,
                                "name": n.name,
                                "delta_cm": delta4(n.diff_cm),
                                "status": if vermerk.is_empty() { "ok" } else { vermerk },
                            })
                        })
                        .collect();
                    json!({
                        "stand": "geprueft",
                        "pfad": pfad.display().to_string(),
                        "naehte": liste,
                        "doppelt": bericht.doppelt.len(),
                        "zugabe_ohne_naht": bericht.zugabe_ohne_naht.len(),
                    })
                }
            };
            json!({
                "pfad": m.pfad.display().to_string(),
                "massdatei": m.massdatei.display().to_string(),
                "massdatei_von_hand": m.massdatei_von_hand,
                "einheit": m.einheit,
                "teile": teile,
                "ohne_umriss": m.ohne_umriss,
                "naehte": naehte,
                "groesste_abweichung": m.groesste_abweichung().map(|n| json!({
                    "kennung": n.kennung,
                    "delta_cm": delta4(n.diff_cm),
                })),
                "nahtliste_vorgabe": nahtliste_neben(&m.pfad).display().to_string(),
            })
        });
        let a = &app.ansicht.anzeige;
        let o = &app.anordnung;
        json!({
            "muster": muster,
            "auswahl": { "teil": app.gewaehlt, "naht": app.naht_gewaehlt },
            "reiter": match app.reiter {
                Reiter::Teile => "teile",
                Reiter::Naehte => "naehte",
                Reiter::Stoffe => "stoffe",
            },
            "thema": match app.thema { Theme::Dark => "dunkel", Theme::Light => "hell" },
            "ansicht2d": {
                "ausgelegt": app.ansicht.ausgelegt,
                "naehte": a.naehte,
                "laengen": a.laengen,
                "teilnamen": a.teilnamen,
                "massstab": a.massstab,
                "fadenlauf": a.fadenlauf,
                "passzeichen": a.passzeichen,
            },
            "anordnung": {
                "bibliothek": o.bibliothek,
                "browser": o.browser,
                "eigenschaften": o.eigenschaften,
                "fenster3d": o.fenster3d,
                "fenster2d": o.fenster2d,
                "leiste_datei": o.leiste_datei,
                "leiste_3d": o.leiste_3d,
                "leiste_2d": o.leiste_2d,
                "aktiv": match app.aktiv { Fenster::Drei => "3d", Fenster::Zwei => "2d" },
            },
            "dialoge": {
                "fehlerfenster": app.fehlerfenster,
                "dateiwahl": app.dateiwahl.is_some(),
                "hilfe": app.hilfe.is_some(),
                "informationen": app.informationen,
            },
            "meldung": app.meldung,
            "meldung_fehler": app.meldung_fehler,
            "simulation": self.sim_status(),
            "wurzeln": {
                "lesen": self.wurzeln.lesen().map(|p| anzeigbar(p).display().to_string()).collect::<Vec<_>>(),
                "schreiben": self.wurzeln.schreiben().map(|p| anzeigbar(p).display().to_string()).collect::<Vec<_>>(),
            },
        })
    }
}

/// An optional text argument; another type is a parameter error.
fn text<'a>(args: &'a Map<String, Value>, name: &str) -> Result<Option<&'a str>, Fehler> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(Fehler::Parameter(format!("'{name}' muss ein Text sein"))),
    }
}

/// An optional positive finite number.
fn zahl(args: &Map<String, Value>, name: &str) -> Result<Option<f64>, Fehler> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_f64() {
            Some(x) if x.is_finite() && x > 0.0 => Ok(Some(x)),
            _ => Err(Fehler::Parameter(format!(
                "'{name}' muss eine positive Zahl sein"
            ))),
        },
    }
}
