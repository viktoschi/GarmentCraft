//! Körper an Zielmaße anpassen — CLOs Schleife mit unseren Messwerten.
//!
//! CLO iteriert `c += (Ziel − Ist)` mit einer festen Schrittweite von 1
//! (Befund 22). Das ist ein Festpunkt-Schritt mit
//! diagonaler Näherung der Jacobi-Matrix. Hier steht dieselbe Schleife,
//! aber mit der vollständigen Jacobi-Matrix der **aktiven Felder**:
//!
//! ```text
//! c[0] = Zielhöhe                        (Höhe ist der Koeffizient, cm)
//! A[i][j] = ∂(Maß i)/∂(c_aktiv[j])       (je Runde neu, Schritt 1,0)
//! δ = (A + λI)⁻¹ (Ziel − Ist)            (gedämpftes Newton, m × m)
//! c_aktiv += δ                           (mit Rücknahme)
//! ```
//!
//! Aktiv sind die Felder, die zu den Messlinien gehören
//! ([`Messort::feld`]) — bei den fünf Rumpflinien also `CIRCUMFERENCE_Bust`
//! bis `CIRCUMFERENCE_LowHip`. Die übrigen Felder bleiben stehen; sie mit
//! zu optimieren hat sich als instabil erwiesen (sie zogen etwa Wade und
//! Fuß mit, um die Taille zu treffen).
//!
//! Gemessen wird entweder auf **mitwandernden Landmarkenlinien**
//! ([`crate::spur`]) oder auf **festen Schnitten** bei einer bekannten
//! Zielhöhe. Letzteres ist richtig, sobald die Maßdatei die Höhen liefert:
//! dann setzt der Aufrufer auch die `HEIGHT_*`-Felder auf dieselben Werte.
//!
//! Gemessen wird die Länge der Schnittkontur — oder, mit
//! [`Messlinien::mit_massband`], wie ein Maßband: konvexe Hülle des
//! waagerechten Rumpfschnitts auf derselben Höhe ([`crate::massband`]).
//!
//! Je Linie ist nicht nur der Umfang ein Maß: [`Messart`] nennt fünf weitere
//! Zahlen derselben Linie (vorderer und hinterer Bogen, Breite, Tiefe,
//! Brustpunktabstand), gemessen auf derselben Hülle von
//! [`crate::bogenmasse`]. Das Ziel ist deshalb nicht mehr quadratisch: es gibt
//! mehr Ziele als Stellschrauben, und [`anpassen`] löst gedämpfte kleinste
//! Quadrate statt eines quadratischen Newton-Schritts.

use crate::bogenmasse::{
    auf_umriss, boegen, ecken_richtungen, versatz_feld, Boegen, Querschnitt, STUETZEN,
    UMRISS_RUNDEN,
};
use crate::formung::Formung;
use crate::massband::{huelle, rumpf_huelle, zentriert_cm, Messdaten, WAAGERECHT};
use crate::spur::{spur, Spur};
use crate::{ringecken, rumpf, Netz, Punkt3};
use std::collections::HashMap;
use std::sync::Arc;

/// Höhe der CLO-Referenzavatarin in Zentimetern (Befund 22).
pub const HOEHE_REF: f64 = 163.95;
/// Höhe der Brust (Apex) an der Referenz, cm.
pub const BRUSTHOEHE_REF: f64 = 119.4119;
/// Höhe der Unterbrust an der Referenz, cm.
pub const UNTERBRUSTHOEHE_REF: f64 = 113.1081;
/// Höhe der Taille an der Referenz, cm.
pub const TAILLENHOEHE_REF: f64 = 105.7056;
/// Höhe der High Hip an der Referenz, cm.
pub const HIGHHIPHOEHE_REF: f64 = 93.8775;
/// Höhe der Hüfte (Low Hip, Sitzhöhe) an der Referenz, cm.
pub const HUEFTHOEHE_REF: f64 = 82.2780;

/// Eine waagerechte Rumpfmesslinie.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Messort {
    /// Brust / Apex.
    Brust,
    /// Unterbrust.
    Unterbrust,
    /// Taille.
    Taille,
    /// High Hip.
    HighHip,
    /// Hüfte / Low Hip.
    Huefte,
}

impl Messort {
    /// Alle Rumpflinien, von oben nach unten.
    pub const ALLE: [Messort; 5] = [
        Messort::Brust,
        Messort::Unterbrust,
        Messort::Taille,
        Messort::HighHip,
        Messort::Huefte,
    ];

    /// Name für Meldungen und Tabellen.
    pub fn name(self) -> &'static str {
        match self {
            Messort::Brust => "Brust",
            Messort::Unterbrust => "Unterbrust",
            Messort::Taille => "Taille",
            Messort::HighHip => "HighHip",
            Messort::Huefte => "Hüfte",
        }
    }

    /// Präfix der Namen in einer SeamlyMe-Maßdatei (`.smis`).
    ///
    /// Aus `bust` wird `bust_circ`, `bust_arc_f`, `bust_arc_b`. Die Unterbrust
    /// heisst dort `lowbust` und hat keine Teilbögen; [`formziele_aus_smis`]
    /// überspringt sie deshalb still.
    pub fn smis_praefix(self) -> &'static str {
        match self {
            Messort::Brust => "bust",
            Messort::Unterbrust => "lowbust",
            Messort::Taille => "waist",
            Messort::HighHip => "highhip",
            Messort::Huefte => "hip",
        }
    }

    /// CLO-Feld, das diese Linie steuert (Befund 22, ID-Tabelle).
    pub fn feld(self) -> &'static str {        match self {
            Messort::Brust => "CIRCUMFERENCE_Bust",
            Messort::Unterbrust => "CIRCUMFERENCE_UnderBust",
            Messort::Taille => "CIRCUMFERENCE_Waist",
            Messort::HighHip => "CIRCUMFERENCE_HighHip",
            Messort::Huefte => "CIRCUMFERENCE_LowHip",
        }
    }

    /// Höhe der Referenzlinie am ungeformten Grundkörper, cm.
    pub fn hoehe(self) -> f64 {
        match self {
            Messort::Brust => BRUSTHOEHE_REF,
            Messort::Unterbrust => UNTERBRUSTHOEHE_REF,
            Messort::Taille => TAILLENHOEHE_REF,
            Messort::HighHip => HIGHHIPHOEHE_REF,
            Messort::Huefte => HUEFTHOEHE_REF,
        }
    }
}

/// Welche Zahl einer Messlinie gemessen wird.
///
/// Der Umfang allein beschreibt keinen Querschnitt (finding 123, Issue #65):
/// dieselben 92,67 cm sind ein Kreis oder eine breite, flache Brust. Deshalb
/// trägt jede Linie neben dem Umfang fünf Formmaße; sie kommen aus
/// [`crate::bogenmasse`] und werden auf derselben Hülle gemessen.
///
/// Die Reihenfolge der Varianten ist die Reihenfolge der Linien einer Höhe:
/// Umfang zuerst, dann die Form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Messart {
    /// Geschlossene Hüllenlänge, cm.
    Umfang,
    /// Bogen vorn, von Seitenpunkt zu Seitenpunkt über die Brust, cm.
    Vorn,
    /// Der andere Bogen, über den Rücken, cm.
    Hinten,
    /// Ausdehnung in `x`, cm.
    Breite,
    /// Ausdehnung in `z`, cm.
    Tiefe,
    /// Gerader Abstand der beiden Brustpunkte, cm.
    Brustpunkt,
}

impl Messart {
    /// Alle Arten in der Reihenfolge der Linien.
    pub const ALLE: [Messart; 6] = [
        Messart::Umfang,
        Messart::Vorn,
        Messart::Hinten,
        Messart::Breite,
        Messart::Tiefe,
        Messart::Brustpunkt,
    ];

    /// Name für Meldungen und Tabellen.
    pub fn name(self) -> &'static str {
        match self {
            Messart::Umfang => "Umfang",
            Messart::Vorn => "Bogen vorn",
            Messart::Hinten => "Bogen hinten",
            Messart::Breite => "Breite",
            Messart::Tiefe => "Tiefe",
            Messart::Brustpunkt => "Brustpunktabstand",
        }
    }

    /// Abnahmegrenze in Zentimetern (Issue #65).
    ///
    /// Sie ist zugleich das Gewicht: der Löser rechnet mit
    /// `Fehler / Toleranz`, damit ein Zentimeter an der Brust nicht
    /// gleich viel wiegt wie ein Zentimeter am hinteren Bogen.
    pub fn toleranz(self) -> f64 {
        match self {
            Messart::Umfang => 0.2,
            Messart::Brustpunkt => 0.5,
            Messart::Vorn | Messart::Hinten => 1.0,
            Messart::Breite | Messart::Tiefe => 0.5,
        }
    }

    /// Der Wert dieser Art aus einem gemessenen Querschnitt.
    pub fn aus_bogen(self, b: &Boegen) -> f64 {
        match self {
            Messart::Umfang => b.umfang,
            Messart::Vorn => b.vorn,
            Messart::Hinten => b.hinten,
            Messart::Breite => b.breite,
            Messart::Tiefe => b.tiefe,
            Messart::Brustpunkt => b.brustpunkt,
        }
    }
}

/// Eine gewünschte Formzahl einer Linie: wo, was, wie viel (cm).
///
/// Eingabe für [`Messlinien::mit_form`]. Der Brustpunktabstand gehört zur
/// Brustlinie; an einer Taille misst er die breitesten Punkte und hat dort
/// keine Bedeutung.
#[derive(Clone, Copy, Debug)]
pub struct Formziel {
    pub ort: Messort,
    pub art: Messart,
    pub wert: f64,
}

/// Zielmaße in Zentimetern.
#[derive(Clone, Debug)]
pub struct Ziel {
    pub hoehe: f64,
    /// Zielumfänge in der Reihenfolge der Messlinien, cm.
    pub masse: Vec<f64>,
}

/// Gemessene Maße in Zentimetern.
#[derive(Clone, Debug)]
pub struct Ist {
    /// Umfänge in der Reihenfolge der Messlinien, cm.
    pub masse: Vec<f64>,
}

impl Ist {
    pub fn fehler(&self, ziel: &Ziel) -> Vec<f64> {
        assert_eq!(
            self.masse.len(),
            ziel.masse.len(),
            "Messlinien und Zielmaße passen nicht zusammen"
        );
        self.masse
            .iter()
            .zip(&ziel.masse)
            .map(|(ist, soll)| soll - ist)
            .collect()
    }

    pub fn groesster_fehler(&self, ziel: &Ziel) -> f64 {
        self.fehler(ziel)
            .iter()
            .fold(0.0f64, |m, f| m.max(f.abs()))
    }

    /// Quadratsumme der auf die Toleranz bezogenen Fehler.
    ///
    /// Das ist die Größe, die der Löser senkt: `Σ ((Ziel − Ist)/Toleranz)²`.
    /// Ohne die Gewichte zöge ein 4-cm-Fehler an der Brust die Taille mit,
    /// die auf 0,2 cm abgenommen wird.
    pub fn kosten(&self, ziel: &Ziel, gewicht: &[f64]) -> f64 {
        assert_eq!(self.masse.len(), ziel.masse.len());
        assert_eq!(self.masse.len(), gewicht.len());
        self.masse
            .iter()
            .zip(&ziel.masse)
            .zip(gewicht)
            .map(|((ist, soll), w)| {
                let f = (soll - ist) / w;
                f * f
            })
            .sum()
    }
}

/// Eine Messlinie.
///
/// Zwei Arten, dieselbe Anatomie zu treffen:
///
/// * `spur = Some(…)` — eine **mitwandernde Materiallinie** von der
///   Referenzavatarin. Sie folgt der Formung, auch wenn sich die
///   Höhenfelder verschieben.
/// * `spur = None` — ein **fester Schnitt** bei `hoehe`. Das ist richtig,
///   sobald die Zielhöhe aus der Maßdatei bekannt ist.
#[derive(Clone, Debug)]
pub struct Messlinie {
    pub ort: Messort,
    pub spur: Option<Spur>,
    /// Höhe der Linie, cm.
    pub hoehe: f64,
    /// Welche Zahl dieser Höhe gemessen wird.
    ///
    /// Dieselbe Höhe kommt mehrfach vor — einmal je [`Messart`]. Der Umfang
    /// ist die Linie, die ein Feld steuert; die Formmaße hängen an ihr.
    pub art: Messart,
}

impl Messlinie {
    /// Mittlere Höhe der Linie auf einem geformten Netz, cm.
    pub fn ist_hoehe(&self, netz: &Netz) -> f64 {
        match &self.spur {
            Some(s) => s.mittlere_hoehe(netz) / 10.0,
            // Ein fester Schnitt liegt per Definition auf dieser Höhe.
            None => self.hoehe,
        }
    }
}

/// Die Messlinien eines Körpers.
#[derive(Clone, Debug)]
pub struct Messlinien {
    pub linien: Vec<Messlinie>,
    /// Messdaten für das Maßband; `None` misst die Schnittkontur.
    pub massband: Option<Arc<Messdaten>>,
}

impl Messlinien {
    /// Misst künftig wie ein Maßband: konvexe Hülle des waagerechten
    /// Rumpfschnitts (Segment Rumpf) auf der Höhe der Linie, Ebene durch
    /// `(0, y, 0)`. Eine Materiallinie wird auf ihrer mittleren Höhe am
    /// jeweiligen Netz gemessen.
    pub fn mit_massband(mut self, d: Arc<Messdaten>) -> Messlinien {
        self.massband = Some(d);
        self
    }

    pub fn len(&self) -> usize {
        self.linien.len()
    }

    pub fn ist_leer(&self) -> bool {
        self.linien.is_empty()
    }

    pub fn orte(&self) -> Vec<Messort> {
        self.linien.iter().map(|l| l.ort).collect()
    }

    pub fn namen(&self) -> Vec<String> {
        self.linien
            .iter()
            .map(|l| {
                if l.art == Messart::Umfang {
                    l.ort.name().to_string()
                } else {
                    format!("{} {}", l.ort.name(), l.art.name())
                }
            })
            .collect()
    }

    pub fn hoehen(&self) -> Vec<f64> {
        self.linien.iter().map(|l| l.hoehe).collect()
    }

    /// Fügt Formziele hinzu: je Ziel eine weitere Linie auf derselben Höhe.
    ///
    /// Die Umfangslinie bleibt die Linie, die ein Feld steuert; die Formmaße
    /// hängen an ihr. Ein Ziel, dessen Ort keine Linie hat, ist ein Fehler.
    pub fn mit_form(mut self, ziele: &[Formziel]) -> Result<Messlinien, String> {
        for z in ziele {
            if z.art == Messart::Umfang {
                return Err("Umfang ist schon die Linie selbst".to_string());
            }
            if !z.wert.is_finite() {
                return Err(format!("{}: kein endlicher Wert", z.art.name()));
            }
            let quelle = self
                .linien
                .iter()
                .find(|l| l.ort == z.ort)
                .ok_or_else(|| format!("{} hat keine Messlinie", z.ort.name()))?;
            let spur = quelle.spur.clone();
            let hoehe = quelle.hoehe;
            if self
                .linien
                .iter()
                .any(|l| l.ort == z.ort && l.art == z.art)
            {
                return Err(format!(
                    "{}: {} doppelt",
                    z.ort.name(),
                    z.art.name()
                ));
            }
            self.linien.push(Messlinie {
                ort: z.ort,
                spur,
                hoehe,
                art: z.art,
            });
        }
        // Reihenfolge: erst die Umfangslinien von oben nach unten, dann je
        // Höhe die Formmaße in der Reihenfolge der Arten.
        self.linien.sort_by_key(|l| (l.ort, l.art));
        Ok(self)
    }

    /// Abnahmegrenze je Linie, cm (Issue #65).
    pub fn toleranzen(&self) -> Vec<f64> {
        self.linien.iter().map(|l| l.art.toleranz()).collect()
    }

    /// Linien, die nicht den Umfang messen, also keinen eigenen Feldindex
    /// haben.
    pub fn formlinien(&self) -> usize {
        self.linien
            .iter()
            .filter(|l| l.art != Messart::Umfang)
            .count()
    }

    /// Feldindex in `c` je Linie, aus der Namensliste des Modells.
    ///
    /// Das ist die Zuordnung „Linie steuert Feld“ für [`anpassen`]: die
    /// Umfangslinien, ohne Doppel. Die Formmaße haben kein Feld und keine
    /// eigene Spalte; sie hängen an der Umfangslinie ihrer Höhe.
    pub fn aktiv(&self, namen: &[String]) -> Result<Vec<usize>, String> {
        let mut felder = Vec::new();
        for l in &self.linien {
            if l.art != Messart::Umfang {
                continue;
            }
            let feld = l.ort.feld();
            let i = namen
                .iter()
                .position(|n| n == feld)
                .ok_or_else(|| format!("Feld {feld} fehlt im Modell"))?;
            if !felder.contains(&i) {
                felder.push(i);
            }
        }
        Ok(felder)
    }
}

/// Breite und Tiefe der vier Rumpfschnitte, cm (finding 123).
///
/// Die Maßdatei der Anwenderin führt Breite und Tiefe nicht — SeamlyMe hat
/// dafür keine Namen, und `bust_arc_f`/`bust_arc_b` legen sie auch nicht fest:
/// bei festem Umfang und festem vorderen Bogen bleibt die Breite frei. Für den
/// vorderen Bogen ist das anders (Seitenstück 17,00, Brustpunktabstand 22 —
/// beides gemessen), aber ohne Breite ist der Querschnitt nicht zu bauen
/// ([`crate::bogenmasse::Querschnitt`] braucht `W` und `T`).
///
/// Die vier Wertepaare sind an der Referenz gemessen, deren Umfänge auf
/// 0,01 cm mit ihren übereinstimmen (Issue #65, Tabelle „Breite × Tiefe“).
/// Sie sind damit die Form des Körpers, an dem die Abnahme hängt, und keine
/// CLO-Datei: Zahlen, keine Daten.
pub const BREITE_TIEFE: [(Messort, f64, f64); 4] = [
    (Messort::Brust, 28.16, 24.47),
    (Messort::Taille, 24.95, 18.43),
    (Messort::HighHip, 30.16, 21.45),
    (Messort::Huefte, 31.83, 20.81),
];

/// Formziele aus einer Maßdatei, mit dem, was beim Abgleich auffiel.
#[derive(Clone, Debug, Default)]
pub struct Formziele {
    pub ziele: Vec<Formziel>,
    /// Zeile je Auffälligkeit: Lücke zwischen den Bögen, fehlender Name.
    pub meldungen: Vec<String>,
}

/// Formziele aus einer SeamlyMe-Maßdatei, alle in Zentimetern.
///
/// Gelesen werden die Teilbögen (`bust_arc_f`, `bust_arc_b`, …) und der
/// Brustpunktabstand (`bustpoint_to_bustpoint`); Breite und Tiefe kommen aus
/// [`BREITE_TIEFE`], weil die Datei sie nicht kennt.
///
/// **Die Bögen schliessen nicht.** Das Band ist geschlossen, also gilt
/// `F + B = P`. Bei der Anwenderin fehlen 0,33 cm an der Brust, 1,00 an der
/// Taille und 2,00 an der Hochhüfte. Der Umfang hat Vorrang (Toleranz 0,2 gegen
/// 1,0), die Lücke wird deshalb **je zur Hälfte** auf vorn und hinten verteilt:
/// bei der Hochhüfte liegen dann beide am Toleranzrand (43,0 und 40,0), an der
/// Taille bleibt 0,5, an der Brust 0,165. Jede Verteilung wird als Meldung
/// mitgeführt, damit die Abnahme zeigt, woher die Zahl kommt.
pub fn formziele_aus_smis(wert: &dyn Fn(&str) -> Option<f64>) -> Formziele {
    let mut f = Formziele::default();
    for &(ort, breite, tiefe) in &BREITE_TIEFE {
        for (art, w) in [(Messart::Breite, breite), (Messart::Tiefe, tiefe)] {
            f.ziele.push(Formziel { ort, art, wert: w });
        }
    }
    for ort in Messort::ALLE {
        let praefix = ort.smis_praefix();
        let name = |suffix: &str| format!("{praefix}_{suffix}");
        let (Some(p), Some(v), Some(h)) = (
            wert(&name("circ")),
            wert(&name("arc_f")),
            wert(&name("arc_b")),
        ) else {
            continue;
        };
        if p <= 0.0 || v <= 0.0 || h <= 0.0 {
            f.meldungen
                .push(format!("{}: Bögen oder Umfang nicht positiv", ort.name()));
            continue;
        }
        // Lücke halbieren; der Umfang bleibt, wie er ist.
        let luecke = v + h - p;
        let v = v - luecke / 2.0;
        let h = p - v;
        if v <= 0.0 || h <= 0.0 {
            f.meldungen.push(format!(
                "{}: Bögen schliessen nicht zum Umfang {p:.2}",
                ort.name()
            ));
            continue;
        }
        if luecke.abs() > 0.001 {
            f.meldungen.push(format!(
                "{}: Bögen {:.2} + {:.2} gegen Umfang {p:.2} (Lücke {luecke:+.2}), geteilt vorn {v:.2} / hinten {h:.2}",
                ort.name(),
                v + luecke / 2.0,
                h + luecke / 2.0
            ));
        }
        f.ziele.push(Formziel {
            ort,
            art: Messart::Vorn,
            wert: v,
        });
        f.ziele.push(Formziel {
            ort,
            art: Messart::Hinten,
            wert: h,
        });
    }
    // Der Brustpunktabstand gehört zur Brust: an einer Taille misst er die
    // breitesten Punkte, an der Hüfte nichts.
    if let Some(d) = wert("bustpoint_to_bustpoint") {
        if d > 0.0 && d.is_finite() {
            f.ziele.push(Formziel {
                ort: Messort::Brust,
                art: Messart::Brustpunkt,
                wert: d,
            });
        } else {
            f.meldungen
                .push(format!("Brustpunktabstand {d} ist kein Maß"));
        }
    }
    f
}

/// Höhe der Brustlinie am geformten Körper, cm.
///
/// **Nicht `height_bustpoint`.** Die Kette misst die Brust bei 113, nicht bei
/// 111: dort ist die Hülle 90,57 cm, bei 113 sind es 92,73 (Issue #65,
/// PLAN „Body without CLO data“). Die 111 ist die Höhe des Punktes, die 113
/// die Höhe, in der das Band läuft.
pub const BRUSTLINIE_CM: f64 = 113.0;

/// Die Messhöhen der Formlinien, cm, von oben nach unten - die Höhen, in denen
/// die Bahnen der Kette liegen und in denen die Abnahme misst.
///
/// Die Unterbrust fehlt: ihre `.smis`-Namen `lowbust_arc_f`/`_b` gibt es nicht,
/// und die Breite-Tiefe-Tabelle führt sie auch nicht. Sie wird trotzdem
/// gemessen ([`anpassen`] kennt fünf Linien), nur nicht geformt.
pub const FORMLINIEN: [(Messort, f64); 4] = [
    (Messort::Brust, BRUSTLINIE_CM),
    (Messort::Taille, 98.0),
    (Messort::HighHip, 93.0),
    (Messort::Huefte, 77.0),
];

/// Die Messhöhen aus der Maßdatei, je Linie wie [`FORMLINIEN`].
///
/// Die Taille kommt aus `height_waist_side`, Hochhüfte und Hüfte aus
/// `height_highhip` und `height_hip`; die Brust bleibt bei
/// [`BRUSTLINIE_CM`], weil ihr Name `height_bustpoint` eine andere Höhe meint
/// (siehe dort). Fehlt ein Name, bleibt die Zahl aus [`FORMLINIEN`].
pub fn formlinien_aus_smis(wert: &dyn Fn(&str) -> Option<f64>) -> [(Messort, f64); 4] {
    let hoehe = |name: &str, vorgabe: f64| wert(name).filter(|h| *h > 0.0).unwrap_or(vorgabe);
    [
        (Messort::Brust, BRUSTLINIE_CM),
        (Messort::Taille, hoehe("height_waist_side", 98.0)),
        (Messort::HighHip, hoehe("height_highhip", 93.0)),
        (Messort::Huefte, hoehe("height_hip", 77.0)),
    ]
}

/// Die Formlinien als fertige Querschnitte, aus [`formziele_aus_smis`].
///
/// Je Höhe werden Umfang, Teilbögen, Breite, Tiefe und - an der Brust - der
/// Brustpunktabstand zu einem [`FormzielEbene`] zusammengesetzt. Was fehlt,
/// lässt die Linie aus: ohne Umfang gibt es nichts zu schliessen, ohne
/// Breite/Tiefe lässt sich der Querschnitt nicht bauen
/// ([`crate::bogenmasse::Querschnitt::form`] braucht beide). Beides landet als
/// Meldung im Ergebnis, damit die Abnahme nicht stillschweigend weniger misst.
pub fn formziel_ebenen(
    formziele: &Formziele,
    hoehen: &[(Messort, f64)],
    wert: &dyn Fn(&str) -> Option<f64>,
) -> (Vec<FormzielEbene>, Vec<String>) {
    let mut ebenen = Vec::new();
    let mut meldungen = Vec::new();
    for &(ort, hoehe) in hoehen {
        let wert_von = |art: Messart| {
            formziele
                .ziele
                .iter()
                .find(|z| z.ort == ort && z.art == art)
                .map(|z| z.wert)
        };
        let (Some(breite), Some(tiefe)) = (wert_von(Messart::Breite), wert_von(Messart::Tiefe))
        else {
            meldungen.push(format!(
                "{}: Breite oder Tiefe fehlt in BREITE_TIEFE",
                ort.name()
            ));
            continue;
        };
        // Der Umfang ist die Linie selbst; die Bögen kommen dazu, wenn es sie
        // gibt, sonst wird das Band aus dem Umfang geschlossen.
        let umfang = wert(&format!("{}_circ", ort.smis_praefix()));
        let (vorn, hinten) = match (wert_von(Messart::Vorn), wert_von(Messart::Hinten)) {
            (Some(v), Some(h)) => (v, h),
            _ => match wert_von(Messart::Umfang) {
                Some(p) => (p / 2.0, p / 2.0),
                None => {
                    meldungen.push(format!("{}: kein Umfang in der Maßdatei", ort.name()));
                    continue;
                }
            },
        };
        let Some(umfang) = umfang.filter(|p| *p > 0.0) else {
            meldungen.push(format!("{}: kein Umfang in der Maßdatei", ort.name()));
            continue;
        };
        let brustpunkt = if ort == Messort::Brust {
            wert_von(Messart::Brustpunkt).unwrap_or(0.0)
        } else {
            0.0
        };
        let querschnitt = Querschnitt {
            breite,
            tiefe,
            brustpunkt,
            vorn,
            hinten,
        };
        if querschnitt.form(STUETZEN).is_none() {
            meldungen.push(format!(
                "{}: Querschnitt aus {breite:.2}/{tiefe:.2}/{brustpunkt:.2}/{vorn:.2}/{hinten:.2} nicht konstruierbar",
                ort.name()
            ));
            continue;
        }
        ebenen.push(FormzielEbene {
            ort,
            hoehe,
            umfang,
            querschnitt,
        });
    }
    (ebenen, meldungen)
}

/// Legt alle fünf Rumpfmesslinien als Materiallinien an einem
/// Referenzkörper an.
pub fn messlinien(grund: &Netz) -> Result<Messlinien, String> {
    messlinien_aus(grund, &Messort::ALLE)
}

/// Legt die gewünschten Messlinien als Materiallinien an.
///
/// Die Reihenfolge wird auf die natürliche Reihenfolge der [`Messort`]
/// gebracht (von oben nach unten); doppelte Orte sind ein Fehler.
pub fn messlinien_aus(grund: &Netz, orte: &[Messort]) -> Result<Messlinien, String> {
    let wahl: Vec<(Messort, Option<f64>)> = orte.iter().map(|o| (*o, None)).collect();
    messlinien_gemischt(grund, &wahl)
}

/// Legt Messlinien an: je Ort entweder ein fester Schnitt (`Some(hoehe)`)
/// oder eine mitwandernde Materiallinie von der Referenz (`None`).
pub fn messlinien_gemischt(
    grund: &Netz,
    wahl: &[(Messort, Option<f64>)],
) -> Result<Messlinien, String> {
    if wahl.is_empty() {
        return Err("keine Messlinien gewählt".to_string());
    }
    let mut gewaehlt = wahl.to_vec();
    gewaehlt.sort_by_key(|(ort, _)| *ort);
    for fenster in gewaehlt.windows(2) {
        if fenster[0].0 == fenster[1].0 {
            return Err(format!("Messort {} doppelt", fenster[0].0.name()));
        }
    }

    let mut linien = Vec::with_capacity(gewaehlt.len());
    for (ort, hoehe) in gewaehlt {
        match hoehe {
            Some(y) => {
                if !y.is_finite() || y <= 0.0 {
                    return Err(format!("{}: unsinnige Höhe {y}", ort.name()));
                }
                // Nicht am Grundkörper prüfen: die Zielperson kann ganz
                // andere Landmarkenhöhen haben, und der Grundkörper ist an
                // den Nähten offen (Befund 25). Fehlt der Schnitt später
                // wirklich, liefert `messen` NaN und `anpassen` bricht ab.
                linien.push(Messlinie {
                    ort,
                    spur: None,
                    hoehe: y,
                    art: Messart::Umfang,
                });
            }
            None => {
                let y = ort.hoehe();
                let q = rumpf(grund, y * 10.0)
                    .ok_or_else(|| format!("kein geschlossener Rumpf bei {y:.2} cm"))?;
                let s = spur(grund, &q)
                    .ok_or_else(|| format!("Schnitt bei {y:.2} cm nicht zuordenbar"))?;
                linien.push(Messlinie {
                    ort,
                    spur: Some(s),
                    hoehe: y,
                    art: Messart::Umfang,
                });
            }
        }
    }
    Ok(Messlinien {
        linien,
        massband: None,
    })
}

/// Misst alle Linien eines Netzes.
///
/// Ein fester Schnitt, der auf dem geformten Netz keine geschlossene
/// Rumpfschleife trifft, liefert `NaN`; [`anpassen`] bricht dann mit einer
/// Meldung ab, statt mit einem falschen Wert weiterzurechnen. Mit Maßband
/// gilt dasselbe für einen leeren Schnitt oder Messdaten, die nicht zum Netz
/// passen.
///
/// Der Umfang kommt aus der Hülle selbst (bei [`Messart::Umfang`] also wie
/// bisher), die Formmaße aus [`crate::bogenmasse::boegen`] derselben Hülle.
pub fn messen(netz: &Netz, linien: &Messlinien) -> Ist {
    if let Some(d) = &linien.massband {
        let passt = d.segment.len() == netz.punkte.len();
        // Gemessen wird im Rahmen der Spezifikation (`formung.md` 8): x und z
        // um den Mittelwert, der tiefste Punkt auf y = 0. Die Zielhöhen der
        // Maßdatei zählen vom Boden; das geformte Netz steht aber nicht auf
        // y = 0 (bei der Anwenderin 1,95 cm darüber).
        let v = zentriert_cm(netz, 10.0);
        let boden = netz.hoehe().map(|(lo, _)| lo / 10.0).unwrap_or(0.0);
        return Ist {
            masse: linien
                .linien
                .iter()
                .map(|l| {
                    let y = match &l.spur {
                        Some(s) => s.mittlere_hoehe(netz) / 10.0 - boden,
                        None => l.hoehe,
                    };
                    if !passt {
                        return f64::NAN;
                    }
                    let (h, umfang) = rumpf_huelle(&v, d, y);
                    aus_huelle(&h, umfang, l.art)
                })
                .collect(),
        };
    }
    Ist {
        masse: linien
            .linien
            .iter()
            .map(|l| match &l.spur {
                Some(s) => {
                    if l.art == Messart::Umfang {
                        s.laenge(netz) / 10.0
                    } else {
                        // Ohne Maßband ist der Ring die Hautkontur; die
                        // Formmaße gelten auf der Hülle, also wird sie hier
                        // aus dem Ring gebildet (wie `massband::huelle`).
                        let ring: Vec<Punkt3> = s
                            .kanten
                            .iter()
                            .map(|k| {
                                let p = k.punkt3(netz);
                                [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0]
                            })
                            .collect();
                        let (h, u) = huelle(&ring, WAAGERECHT);
                        aus_huelle(&h, u, l.art)
                    }
                }
                None => match rumpf(netz, l.hoehe * 10.0) {
                    Some(q) if l.art == Messart::Umfang => q.laenge / 10.0,
                    Some(q) => {
                        let ring: Vec<Punkt3> = q
                            .ring
                            .iter()
                            .map(|p| [p.0 / 10.0, l.hoehe, p.1 / 10.0])
                            .collect();
                        let (h, u) = huelle(&ring, WAAGERECHT);
                        aus_huelle(&h, u, l.art)
                    }
                    None => f64::NAN,
                },
            })
            .collect(),
    }
}

/// Eine Zahl aus einer Hülle: Umfang direkt, alles andere über
/// [`crate::bogenmasse::boegen`].
///
/// `NaN`, wenn die Hülle nicht messbar ist (zu wenige Punkte, keine zwei
/// Seitenpunkte) — dann gibt es die Formmaße nicht, und der Löser soll das als
/// fehlende Messung sehen, nicht als 0. Ein Querschnitt ohne Brustpunktpaar ist
/// messbar; sein Brustpunktabstand ist 0.
fn aus_huelle(h: &[Punkt3], umfang: f64, art: Messart) -> f64 {
    if art == Messart::Umfang {
        return if umfang > 0.0 { umfang } else { f64::NAN };
    }
    match boegen(h) {
        Some(b) => art.aus_bogen(&b),
        None => f64::NAN,
    }
}

/// Wie weit das Feld über die oberste und unterste Formlinie hinaus
/// ausläuft, cm.
///
/// Ohne diesen Auslauf entstünde an der letzten Linie eine Kante: darunter
/// bliebe der Körper, wie er ist, darüber wäre er verformt. Drei Zentimeter
/// sind an jeder Rumpflinie noch Rumpf und keine Schulter und kein Schritt.
pub const AUSLAUF: f64 = 3.0;

/// Wie weit ausserhalb des gemessenen Querschnitts ein Netupunkt noch
/// mitgeht (Faktor auf den gemessenen Radius).
///
/// Der Querschnitt ist die Rumpfschleife um die Körperachse; Arme und Beine
/// sind eigene Schleifen und liegen **ausserhalb** von ihr. Ohne diese Grenze
/// zöge eine Verschmälerung der Taille den daneben hängenden Arm mit nach
/// innen — er bliebe in Form, aber an der falschen Stelle. 1,25 lässt die
/// Haut zwischen Rumpf und Hülle mitgehen und hält den Arm draussen.
const PUNKT_GRENZE: f64 = 1.25;

/// Eine Querschnittsform, die auf eine Höhe des Netzes gelegt wird, cm.
#[derive(Clone, Copy, Debug)]
pub struct FormzielEbene {
    pub ort: Messort,
    /// Höhe am Netz über dem Boden, cm — dieselbe Zahl wie in der Maßdatei.
    pub hoehe: f64,
    /// Zielumfang der Linie, cm.
    pub umfang: f64,
    pub querschnitt: Querschnitt,
}

impl FormzielEbene {
    /// Die sechs Sollwerte in der Reihenfolge von [`Messart::ALLE`], cm.
    pub fn soll(&self) -> [(Messart, f64); 6] {
        [
            (Messart::Umfang, self.umfang),
            (Messart::Vorn, self.querschnitt.vorn),
            (Messart::Hinten, self.querschnitt.hinten),
            (Messart::Breite, self.querschnitt.breite),
            (Messart::Tiefe, self.querschnitt.tiefe),
            (Messart::Brustpunkt, self.querschnitt.brustpunkt),
        ]
    }
}

/// Was eine Formlinie vorher und nachher misst.
#[derive(Clone, Debug)]
pub struct Formlinie {
    pub ort: Messort,
    pub hoehe: f64,
    pub ziel: FormzielEbene,
    /// Vor der Formung gemessen, cm.
    pub vorher: Boegen,
    /// Nach der Formung gemessen, cm.
    pub nachher: Option<Boegen>,
}

impl Formlinie {
    /// Die sechs Werte nach der Formung, cm; `None` wo nicht messbar.
    pub fn ist(&self) -> [(Messart, Option<f64>); 6] {
        Messart::ALLE.map(|art| {
            let wert = self.nachher.as_ref().map(|b| match art {
                Messart::Umfang => b.umfang,
                _ => art.aus_bogen(b),
            });
            (art, wert)
        })
    }

    /// Fehler `ist − soll` je Messart, cm. `None`, wo nicht messbar.
    pub fn fehler(&self) -> [(Messart, Option<f64>); 6] {
        let ist = self.ist();
        let soll = self.ziel.soll();
        Messart::ALLE.map(|art| {
            let i = ist.iter().find(|(a, _)| *a == art).and_then(|(_, w)| *w);
            let s = soll.iter().find(|(a, _)| *a == art).map(|(_, w)| *w);
            (art, match (i, s) {
                (Some(i), Some(s)) => Some(i - s),
                _ => None,
            })
        })
    }

    /// Hält diese Linie alle sechs Abnahmegrenzen?
    pub fn gehalten(&self) -> bool {
        self.fehler().iter().all(|(art, f)| match f {
            Some(f) => f.abs() <= art.toleranz() + 1.0e-9,
            // Kein Messwert ist kein gehaltener Wert.
            None => false,
        })
    }
}

/// Zieht das Netz auf die Querschnittsformen.
///
/// Der Weg ist derselbe wie in [`crate::bogenmasse`]: je Linie wird der
/// gemessene Rumpfquerschnitt auf den aus den fünf Zahlen konstruierten
/// Umriss gezogen ([`auf_umriss`]), und der Unterschied wird als radiales
/// Feld ([`versatz_feld`]) auf das Netz gelegt. Zwischen zwei Linien werden
/// die Felder linear überblendet, über die äusserste Linie hinaus laufen sie
/// über [`AUSLAUF`] aus.
///
/// Vorher bekommt jede Linienhöhe einen eigenen Netzring
/// ([`ring_einfuegen`]). Ohne ihn ist der gemessene Querschnitt eine Sehne
/// durch ein Netz, dessen Punkte 0,6–1,0 cm darüber und darunter liegen: an
/// der Brustspitze verliert die Sehne 2,0 cm des Zielabstands, und an dieser
/// Stelle wird der Punkt nicht mehr erreichbar, weil die Verschiebung schon
/// vorher in die benachbarten Punkte interpoliert wurde.
///
/// Bewegt wird nur, was zum Rumpf gehört: Punkte weiter als
/// [`PUNKT_GRENZE`] mal dem gemessenen Radius liegen in einer eigenen
/// Schleife (Arm, Bein) und bleiben stehen.
///
/// Gibt den Bericht je Linie zurück, in der Reihenfolge der Eingabe, und die
/// Meldungen zu Linien, die nicht geformt werden konnten.
pub fn formen(netz: &mut Netz, ebenen: &[FormzielEbene]) -> (Vec<Formlinie>, Vec<String>) {
    let mut meldungen = Vec::new();
    if ebenen.is_empty() {
        meldungen.push("keine Formlinien".to_string());
        return (Vec::new(), meldungen);
    }

    // Von oben nach unten, damit die Überblendung eine Richtung hat.
    let mut reihenfolge: Vec<usize> = (0..ebenen.len()).collect();
    reihenfolge.sort_by(|&a, &b| {
        ebenen[b]
            .hoehe
            .partial_cmp(&ebenen[a].hoehe)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Erst die Ringe, dann messen: der Querschnitt soll auf Netzpunkten
    // liegen und nicht als Sehne zwischen zweien entstehen.
    for &i in &reihenfolge {
        ring_einfuegen(netz, ebenen[i].hoehe * 10.0);
    }

    // Dann die Ecken der Zielform in die Ringe setzen. Eine radiale
    // Verschiebung kann keine Ecke erzeugen, die zwischen zwei Ringpunkten
    // liegt: an der Brustspitze sind das 6° Lücke und 0,5 cm des Zielabstands,
    // und der Fehler geht mit der Ringdichte nur asymptotisch weg. Der Punkt
    // liegt auf einer Ringkante, das Polygon behält also seine Form, und die
    // Messung findet ihn anschliessend als Scheitel.
    for &i in &reihenfolge {
        let e = &ebenen[i];
        let Some((ziel, _)) = e.querschnitt.form(STUETZEN) else {
            continue;
        };
        let richtungen = ecken_richtungen(&ziel);
        if !richtungen.is_empty() {
            ringecken::ecken_einpassen(netz, e.hoehe * 10.0, &richtungen);
        }
    }

    struct Ebene {
        versatz: crate::bogenmasse::Versatz,
        ring: Vec<Punkt3>,
        hoehe: f64,
        platz: usize,
    }

    let mut felder: Vec<Ebene> = Vec::new();
    let mut linien: Vec<Formlinie> = ebenen
        .iter()
        .map(|e| Formlinie {
            ort: e.ort,
            hoehe: e.hoehe,
            ziel: *e,
            vorher: leerer_bogen(),
            nachher: None,
        })
        .collect();

    for &i in &reihenfolge {
        let e = &ebenen[i];
        let y = e.hoehe * 10.0;
        let Some(q) = rumpf(netz, y) else {
            meldungen.push(format!(
                "{}: kein geschlossener Rumpfquerschnitt bei {:.2} cm",
                e.ort.name(),
                e.hoehe
            ));
            continue;
        };
        let ring: Vec<Punkt3> = q
            .ring
            .iter()
            .map(|p| [p.0 / 10.0, e.hoehe, p.1 / 10.0])
            .collect();
        let (ist_h, ist_u) = huelle(&ring, WAAGERECHT);
        let Some(ist_b) = boegen(&ist_h) else {
            meldungen.push(format!(
                "{}: Querschnitt bei {:.2} cm ist nicht messbar",
                e.ort.name(),
                e.hoehe
            ));
            continue;
        };
        if !(ist_u > 0.0) || !ist_u.is_finite() {
            meldungen.push(format!(
                "{}: Umfang bei {:.2} cm nicht messbar",
                e.ort.name(),
                e.hoehe
            ));
            continue;
        }
        let Some((ziel_umriss, _)) = e.querschnitt.form(STUETZEN) else {
            meldungen.push(format!(
                "{}: die fünf Zahlen bei {:.2} cm sind nicht konstruierbar",
                e.ort.name(),
                e.hoehe
            ));
            continue;
        };
        // Der Umfang, gegen den geschlossen wird: der Zielumfang der Linie.
        // Fehlt er, bleibt der gemessene - dann formt nur der Umriss.
        let umfang = if e.umfang > 0.0 && e.umfang.is_finite() {
            e.umfang
        } else {
            ist_u
        };
        let Some(neu) = auf_umriss(&ring, &ziel_umriss, umfang, UMRISS_RUNDEN) else {
            meldungen.push(format!(
                "{}: Querschnitt bei {:.2} cm lässt sich nicht ziehen",
                e.ort.name(),
                e.hoehe
            ));
            continue;
        };
        let Some(versatz) = versatz_feld(&ring, &neu) else {
            meldungen.push(format!(
                "{}: kein Versatzfeld bei {:.2} cm",
                e.ort.name(),
                e.hoehe
            ));
            continue;
        };
        linien[i].vorher = ist_b;
        felder.push(Ebene {
            versatz,
            ring,
            hoehe: e.hoehe,
            platz: i,
        });
    }

    if felder.is_empty() {
        return (linien, meldungen);
    }

    // Die Felder liegen von oben nach unten; `felder` ist schon so sortiert.
    let hoehen: Vec<f64> = felder.iter().map(|f| f.hoehe).collect();
    let mitte: Vec<(f64, f64)> = felder.iter().map(|f| f.versatz.mitte).collect();
    // Radius des gemessenen Querschnitts je Feld, für die Punktgrenze.
    let strahl: Vec<Vec<(f64, f64)>> = felder
        .iter()
        .map(|f| {
            f.ring
                .iter()
                .map(|p| (p[0] - f.versatz.mitte.0, p[2] - f.versatz.mitte.1))
                .collect()
        })
        .collect();

    for p in &mut netz.punkte {
        let y = p[1] / 10.0;
        let gewichte = ueberblendung(y, &hoehen);
        if gewichte.is_empty() {
            continue;
        }
        let summe: f64 = gewichte.iter().map(|(_, w)| w).sum();
        if summe <= 1.0e-12 {
            continue;
        }
        // Gemischter Mittelpunkt der beteiligten Querschnitte, cm.
        let (mut cx, mut cz) = (0.0, 0.0);
        for &(k, w) in &gewichte {
            cx += w * mitte[k].0;
            cz += w * mitte[k].1;
        }
        cx /= summe;
        cz /= summe;
        // Alles in Zentimetern, wie die Querschnitte.
        let (x, z) = (p[0] / 10.0, p[2] / 10.0);
        let (dx, dz) = (x - cx, z - cz);
        let r = (dx * dx + dz * dz).sqrt();
        if r < 1.0e-9 {
            continue;
        }
        let winkel = dz.atan2(dx);
        // Ausserhalb der Rumpfschleife: Arm oder Bein, stehen lassen.
        let mut im_rumpf = false;
        for &(k, w) in &gewichte {
            if w <= 0.0 {
                continue;
            }
            if let Some(r_haut) = strahlaustritt_winkel(&strahl[k], winkel) {
                if r <= PUNKT_GRENZE * r_haut {
                    im_rumpf = true;
                }
            }
        }
        if !im_rumpf {
            continue;
        }
        let mut verschiebung = 0.0;
        for &(k, w) in &gewichte {
            verschiebung += w * felder[k].versatz.bei(winkel);
        }
        // **Nicht** durch die Gewichtssumme teilen: zwischen zwei Linien ist
        // sie ohnehin 1, über die äusserste Linie hinaus ist sie der
        // Auslauf. Wer sie herausrechnet, nimmt [`AUSLAUF`] die Wirkung -
        // der Punkt bekäme 3 cm über der letzten Linie noch die volle
        // Verschiebung und danach einen Sprung auf null.
        let faktor = 1.0 + verschiebung / r;
        if !faktor.is_finite() || faktor <= 0.0 {
            continue;
        }
        p[0] = 10.0 * (cx + dx * faktor);
        p[2] = 10.0 * (cz + dz * faktor);
    }

    // Nachmessen, damit der Bericht zeigt, was geworden ist.
    for f in &felder {
        let y = f.hoehe * 10.0;
        if let Some(q) = rumpf(netz, y) {
            let ring: Vec<Punkt3> = q
                .ring
                .iter()
                .map(|p| [p.0 / 10.0, f.hoehe, p.1 / 10.0])
                .collect();
            let (h, u) = huelle(&ring, WAAGERECHT);
            let _ = u;
            linien[f.platz].nachher = boegen(&h);
        }
    }

    (linien, meldungen)
}

/// Setzt einen durchgehenden Netzring auf die Höhe `y`, mm.
///
/// Der gemessene Querschnitt entsteht als Schnitt durch die Dreiecke. Liegt
/// die Schnittebene zwischen zwei Netzringen, ist er eine **Sehne** durch
/// ein Netz, dessen Punkte 0,6–1,0 cm darüber und darunter liegen. An der
/// Brustspitze verläuft die Form dort so stark, dass die Sehne 2,0 cm des
/// Zielabstands verliert - und dieser Verlust ist nicht mehr zu korrigieren,
/// weil die Verschiebung vorher in die Nachbarpunkte interpoliert wird.
///
/// Deshalb bekommt jede Formlinie einen eigenen Ring: jede Kante, die `y`
/// kreuzt, wird in ihrem Kreuzungspunkt geteilt, und das angrenzende Dreieck
/// zerfällt in drei. Das ist eine konforme Verfeinerung - jede neue Kante
/// hängt an einem neuen Punkt, keine Naht reisst, kein Punkt wandert.
///
/// Ein Netz, das die Höhe schon als Ring hat (oder gar nicht kreuzt), bleibt
/// unverändert.
fn ring_einfuegen(netz: &mut Netz, y: f64) {
    /// Toleranz, ab der ein Punkt als auf der Höhe liegend gilt, mm.
    ///
    /// Ein Punkt, der 1e-6 mm über der Ebene liegt, verschöbe den Schnitt um
    /// 1e-6 mm - weit unter der Messgenauigkeit, aber er erzeugte einen
    /// Nullspalt im Ring, und der reisst die Verkettung auf. Deshalb wird
    /// vorher geschnappt statt gerechnet.
    const EPS: f64 = 1.0e-6;

    let mut schnappt = false;
    for p in &mut netz.punkte {
        if (p[1] - y).abs() <= EPS {
            p[1] = y;
            schnappt = true;
        }
    }

    let basis = netz.punkte.len();
    let mut neue: Vec<Punkt3> = Vec::new();
    let mut zwischen: HashMap<(usize, usize), usize> = HashMap::new();
    let mut dreiecke: Vec<[usize; 3]> = Vec::with_capacity(netz.dreiecke.len() * 2);

    for &[a, b, c] in &netz.dreiecke {
        let ecken = [a, b, c];
        let ys = [
            netz.punkte[a][1],
            netz.punkte[b][1],
            netz.punkte[c][1],
        ];
        // Liegt das Dreieck ganz auf einer Seite (oder mit einer Kante auf der
        // Höhe), bleibt es, wie es ist.
        let echt_geteilt = ys.iter().any(|&v| v < y) && ys.iter().any(|&v| v > y);
        if !echt_geteilt {
            dreiecke.push([a, b, c]);
            continue;
        }

        // Beide Hälften einzeln abschneiden. Beide laufen in derselben
        // Reihenfolge um das Dreieck, behalten also den Umlaufsinn, und beide
        // holen ihre Kreuzungspunkte aus derselben Tabelle - deshalb passen
        // die neuen Kanten der Nachbardreiecke zusammen.
        for oben in [true, false] {
            let mut teil: Vec<usize> = Vec::with_capacity(4);
            for k in 0..3 {
                let (i, j) = (ecken[k], ecken[(k + 1) % 3]);
                let (yi, yj) = (ys[k], ys[(k + 1) % 3]);
                let i_da = if oben { yi >= y } else { yi <= y };
                if i_da {
                    teil.push(i);
                }
                let j_da = if oben { yj >= y } else { yj <= y };
                if i_da == j_da {
                    continue;
                }
                // Liegt ein Endpunkt selbst auf der Höhe, ist er die Kreuzung.
                if (yi - y).abs() <= 1.0e-12 {
                    continue; // i wurde schon eingefügt
                }
                let idx = if (yj - y).abs() <= 1.0e-12 {
                    j
                } else {
                    let schluessel = if i < j { (i, j) } else { (j, i) };
                    match zwischen.get(&schluessel) {
                        Some(&v) => v,
                        None => {
                            let t = (y - yi) / (yj - yi);
                            let p = netz.punkte[i];
                            let q = netz.punkte[j];
                            neue.push([p[0] + t * (q[0] - p[0]), y, p[2] + t * (q[2] - p[2])]);
                            let v = basis + neue.len() - 1;
                            zwischen.insert(schluessel, v);
                            v
                        }
                    }
                };
                if teil.last() != Some(&idx) {
                    teil.push(idx);
                }
            }
            // Am Schluss kann der Anfangspunkt noch einmal auftauchen.
            if teil.len() > 1 && teil.first() == teil.last() {
                teil.pop();
            }
            if teil.len() < 3 {
                continue;
            }
            for k in 1..teil.len() - 1 {
                dreiecke.push([teil[0], teil[k], teil[k + 1]]);
            }
        }
    }

    if neue.is_empty() {
        // Die Höhe war schon ein Ring: die geschnappten Punkte bleiben, die
        // Dreiecke sind unverändert.
        let _ = schnappt;
        return;
    }
    netz.punkte.append(&mut neue);
    netz.dreiecke = dreiecke;
}

/// Ein [`Boegen`] aus lauter Nullen, als Platzhalter vor der ersten Messung.
fn leerer_bogen() -> Boegen {
    Boegen {
        umfang: f64::NAN,
        breite: f64::NAN,
        tiefe: f64::NAN,
        vorn: f64::NAN,
        hinten: f64::NAN,
        brustpunkt: f64::NAN,
        seite: f64::NAN,
    }
}

/// Gewichte der Formlinien für die Höhe `y`, cm; die Summe ist 1.
///
/// Zwischen zwei Linien linear, über die äusserste hinaus linear über
/// [`AUSLAUF`] auf null. `hoehen` ist absteigend sortiert.
fn ueberblendung(y: f64, hoehen: &[f64]) -> Vec<(usize, f64)> {
    let n = hoehen.len();
    if n == 0 {
        return Vec::new();
    }
    if y >= hoehen[0] {
        let d = y - hoehen[0];
        return if d < AUSLAUF {
            vec![(0, 1.0 - d / AUSLAUF)]
        } else {
            Vec::new()
        };
    }
    if y <= hoehen[n - 1] {
        let d = hoehen[n - 1] - y;
        return if d < AUSLAUF {
            vec![(n - 1, 1.0 - d / AUSLAUF)]
        } else {
            Vec::new()
        };
    }
    for i in 0..n - 1 {
        let (oben, unten) = (hoehen[i], hoehen[i + 1]);
        if oben >= y && y >= unten {
            let abstand = oben - unten;
            if abstand <= 1.0e-12 {
                return vec![(i, 1.0)];
            }
            let t = (oben - y) / abstand;
            return vec![(i, 1.0 - t), (i + 1, t)];
        }
    }
    Vec::new()
}

/// Strahlaustritt eines Querschnitts, der schon um seinen Mittelpunkt
/// verschoben ist: die Punkte sind `(dx, dz)`, `winkel` in Bogenmaß.
fn strahlaustritt_winkel(punkte: &[(f64, f64)], winkel: f64) -> Option<f64> {
    let (dx, dz) = (winkel.cos(), winkel.sin());
    let m = punkte.len();
    if m < 3 {
        return None;
    }
    let mut weit = f64::NEG_INFINITY;
    for i in 0..m {
        let a = punkte[i];
        let b = punkte[(i + 1) % m];
        let (ex, ez) = (b.0 - a.0, b.1 - a.1);
        let nenner = dx * ez - dz * ex;
        if nenner.abs() < 1.0e-12 {
            continue;
        }
        let t = (a.0 * ez - a.1 * ex) / nenner;
        let s = (a.0 * dz - a.1 * dx) / nenner;
        if (0.0..=1.0).contains(&s) && t > 0.0 {
            weit = weit.max(t);
        }
    }
    (weit > 0.0).then_some(weit)
}

/// Ergebnis einer Anpassung.
#[derive(Clone, Debug)]
pub struct Ergebnis {
    pub c: Vec<f64>,
    pub ist: Ist,
    pub runden: usize,
    /// Fehler nach der letzten Runde, cm.
    pub fehler: Vec<f64>,
    /// Fehler je Runde, cm.
    pub verlauf: Vec<Vec<f64>>,
    /// Wurde die Iteration wegen Divergenz zurückgenommen?
    pub divergiert: bool,
}

/// Löst ein lineares Gleichungssystem mit Gauß-Elimination und
/// Spaltenpivot. Die Dimension kommt aus der rechten Seite.
fn loese(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for s in 0..n {
        let mut p = s;
        for z in s + 1..n {
            if a[z][s].abs() > a[p][s].abs() {
                p = z;
            }
        }
        a.swap(s, p);
        b.swap(s, p);
        let d = a[s][s];
        if d.abs() < 1.0e-300 {
            continue;
        }
        for z in s + 1..n {
            let f = a[z][s] / d;
            for k in s..n {
                a[z][k] -= f * a[s][k];
            }
            b[z] -= f * b[s];
        }
    }
    let mut x = vec![0.0f64; n];
    for s in (0..n).rev() {
        let mut summe = b[s];
        for k in s + 1..n {
            summe -= a[s][k] * x[k];
        }
        x[s] = if a[s][s].abs() > 1.0e-300 {
            summe / a[s][s]
        } else {
            0.0
        };
    }
    x
}

/// Löst das Normalgleichungssystem `A δ = g` der kleinsten Quadrate.
///
/// `A` ist symmetrisch und wird mit `λ · diag(A)` gedämpft (Levenberg-
/// Marquardt): die Dämpfung wirkt relativ zu jeder Spalte, damit ein Feld,
/// das in Zentimetern stark wirkt, nicht durch ein schwaches verdrängt wird.
fn kleinste_quadrate(mut a: Vec<Vec<f64>>, g: Vec<f64>, lambda: f64) -> Vec<f64> {
    let k = g.len();
    for j in 0..k {
        let d = a[j][j].abs();
        a[j][j] += lambda * if d > 1.0e-12 { d } else { 1.0 };
    }
    loese(a, g)
}

/// Grösster erlaubter Schritt je Runde, in Einheiten von `c`.
const MAX_SCHRITT: f64 = 8.0;
/// Abbruch, wenn alle Maße so nah sind, in Zentimetern.
const TOLERANZ: f64 = 0.01;
/// Höchstzahl der Runden.
const MAX_RUNDEN: usize = 30;
/// Startdämpfung, relativ zur Diagonale der Normalgleichungen.
const DAMPFUNG: f64 = 1.0e-6;
/// So oft wird die Dämpfung erhöht, wenn kein Schritt angenommen wird.
const DAMPFUNGSVERSUCHE: usize = 6;

/// Passt den Körper an die Zielmaße an.
///
/// `c_ref` ist der Referenzvektor aus [`crate::formung::referenz_vektor`]
/// (Länge = Felder − 1). Der Vektor darf vorher an die Zielperson angepasst
/// sein (z. B. andere `HEIGHT_*`-Felder); überschrieben wird nur `c[0]` mit
/// `ziel.hoehe`. `aktiv` nennt je **Umfangslinie** den Feldindex in `c` (siehe
/// [`Messlinien::aktiv`]); nur diese Felder werden verschoben.
///
/// Das System darf überbestimmt sein: es gibt mehr Ziele als Stellschrauben,
/// sobald Formmaße dazukommen ([`Messart`]). Gelöst wird gedämpft nach den
/// kleinsten Quadraten, gewichtet mit `1/Toleranz` je Ziel
/// ([`Messart::toleranz`]) — ein Zentimeter an der Taille wiegt damit so viel
/// wie die Abnahmegrenze es verlangt, und ein unerreichbares Ziel zieht die
/// übrigen nicht mit.
pub fn anpassen(
    grund: &Netz,
    formung: &Formung,
    c_ref: &[f64],
    linien: &Messlinien,
    ziel: &Ziel,
    aktiv: &[usize],
) -> Result<Ergebnis, String> {
    let n = c_ref.len();
    if n + 1 != formung.felder() {
        return Err(format!(
            "{n} Koeffizienten für {} Felder",
            formung.felder()
        ));
    }
    let m = linien.len();
    if m != ziel.masse.len() {
        return Err(format!("{m} Messlinien, aber {} Zielmaße", ziel.masse.len()));
    }
    if linien.ist_leer() {
        return Err("keine Messlinien".to_string());
    }
    if ziel.masse.iter().any(|w| !w.is_finite()) {
        return Err("die Zielmaße enthalten keinen endlichen Wert".to_string());
    }
    if aktiv.is_empty() {
        return Err("keine aktiven Felder".to_string());
    }
    let mut gesehen = vec![false; n];
    for &k in aktiv {
        if k == 0 || k >= n {
            return Err(format!("aktives Feld {k} liegt ausserhalb von 1..{n}"));
        }
        if gesehen[k] {
            return Err(format!("aktives Feld {k} doppelt"));
        }
        gesehen[k] = true;
    }
    let gewicht = linien.toleranzen();

    let mut c = c_ref.to_vec();
    // Die Höhe ist in CLOs Modell der Koeffizient selbst (Befund 22).
    c[0] = ziel.hoehe;

    let mut ist = messen(&formung.anwenden(grund, &c), linien);
    if ist.masse.iter().any(|w| !w.is_finite()) {
        return Err("die Messung liefert keine endlichen Werte".to_string());
    }

    let mut verlauf = vec![ist.fehler(ziel)];
    let mut divergiert = false;
    let mut runden = 0;
    let mut kosten = ist.kosten(ziel, &gewicht);
    for runde in 0..MAX_RUNDEN {
        runden = runde + 1;
        if ist.groesster_fehler(ziel) < TOLERANZ {
            break;
        }

        // Jacobi der aktiven Spalten an der aktuellen Form:
        // J[i][j] = ∂(Maß i)/∂(c_aktiv[j]), Schritt 1,0.
        let k = aktiv.len();
        let mut jac = vec![vec![0.0f64; k]; m];
        for (j, &feld) in aktiv.iter().enumerate() {
            let mut c2 = c.clone();
            c2[feld] += 1.0;
            let mess = messen(&formung.anwenden(grund, &c2), linien);
            for i in 0..m {
                jac[i][j] = mess.masse[i] - ist.masse[i];
            }
        }
        // Normalgleichungen gewichtet: A = Jᵀ W J, g = Jᵀ W r.
        let mut a = vec![vec![0.0f64; k]; k];
        let mut g = vec![0.0f64; k];
        for i in 0..m {
            let w = 1.0 / gewicht[i];
            let r = (ziel.masse[i] - ist.masse[i]) * w;
            for j in 0..k {
                g[j] += jac[i][j] * w * r;
                for l in 0..=j {
                    a[j][l] += jac[i][j] * w * jac[i][l] * w;
                }
            }
        }
        for j in 0..k {
            for l in j + 1..k {
                a[j][l] = a[l][j];
            }
        }

        // Dämpfung so weit erhöhen, bis ein Schritt den Fehler senkt.
        let mut lambda = DAMPFUNG;
        let mut angenommen = false;
        let mut neu = c.clone();
        let mut neu_ist = ist.clone();
        let mut neu_kosten = kosten;
        for _ in 0..DAMPFUNGSVERSUCHE {
            let mut delta = kleinste_quadrate(a.clone(), g.clone(), lambda);
            let max = delta.iter().fold(0.0f64, |m, d| m.max(d.abs()));
            if max > MAX_SCHRITT {
                let f = MAX_SCHRITT / max;
                for d in &mut delta {
                    *d *= f;
                }
            }
            // Rücknahme, wenn der Fehler wächst (wie CLOs Treiber).
            let mut faktor = 1.0;
            for _ in 0..8 {
                for (j, &feld) in aktiv.iter().enumerate() {
                    neu[feld] = c[feld] + delta[j] * faktor;
                }
                let mess = messen(&formung.anwenden(grund, &neu), linien);
                let neu_k = mess.kosten(ziel, &gewicht);
                if neu_k.is_finite() && neu_k < kosten {
                    neu_ist = mess;
                    neu_kosten = neu_k;
                    angenommen = true;
                    break;
                }
                faktor *= 0.5;
            }
            if angenommen {
                break;
            }
            lambda *= 10.0;
        }
        if !angenommen {
            divergiert = true;
            break;
        }
        c = neu;
        ist = neu_ist;
        let besserung = (kosten - neu_kosten).abs() / kosten.max(1.0e-12);
        kosten = neu_kosten;
        verlauf.push(ist.fehler(ziel));
        // Kein Fortschritt mehr: die Ziele widersprechen sich (F + B = P
        // lässt sich an der Hochhüfte nicht mit 0,0 cm Lücke erfüllen).
        if besserung < 1.0e-10 {
            break;
        }
    }

    Ok(Ergebnis {
        fehler: ist.fehler(ziel),
        c,
        ist,
        runden,
        verlauf,
        divergiert,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loese_reproduziert_ein_bekanntes_system() {
        // 5×5, diagonal dominant, Lösung x = [1, 2, 3, 4, 5].
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut a = vec![vec![0.0f64; 5]; 5];
        for i in 0..5 {
            for j in 0..5 {
                a[i][j] = if i == j { 10.0 } else { 0.1 * (i + j) as f64 };
            }
        }
        let b: Vec<f64> = (0..5)
            .map(|i| (0..5).map(|j| a[i][j] * x[j]).sum())
            .collect();
        let geloest = loese(a, b);
        for i in 0..5 {
            assert!((geloest[i] - x[i]).abs() < 1.0e-9, "{i}: {}", geloest[i]);
        }
    }

    #[test]
    fn umfang_eines_zylinders_wird_als_zentimeter_gemessen() {
        let n = Netz::zylinder(100.0, 3000.0, 720, 20);
        let linien = messlinien(&n).expect("Linien");
        assert_eq!(linien.len(), 5);
        let ist = messen(&n, &linien);
        for (i, wert) in ist.masse.iter().enumerate() {
            assert!((wert - 62.83).abs() < 0.01, "Linie {i}: {wert}");
        }
    }

    #[test]
    fn teilmenge_der_linien_haelt_ihre_reihenfolge() {
        let n = Netz::zylinder(100.0, 3000.0, 720, 20);
        let linien = messlinien_aus(&n, &[Messort::Huefte, Messort::Brust]).expect("Linien");
        assert_eq!(linien.orte(), vec![Messort::Brust, Messort::Huefte]);
        assert_eq!(linien.hoehen(), vec![BRUSTHOEHE_REF, HUEFTHOEHE_REF]);
    }

    #[test]
    fn leer_oder_doppelt_ist_ein_fehler() {
        let n = Netz::zylinder(100.0, 3000.0, 720, 20);
        assert!(messlinien_aus(&n, &[]).is_err());
        assert!(messlinien_aus(&n, &[Messort::Taille, Messort::Taille]).is_err());
        assert!(messlinien_gemischt(&n, &[(Messort::Taille, Some(0.0))]).is_err());
    }

    #[test]
    fn fester_schnitt_misst_auf_der_hoehe() {
        let n = Netz::zylinder(100.0, 3000.0, 720, 20);
        let linien = messlinien_gemischt(&n, &[(Messort::Taille, Some(150.0))]).expect("Linien");
        let ist = messen(&n, &linien);
        assert!((ist.masse[0] - 62.83).abs() < 0.01, "{}", ist.masse[0]);
        assert!((linien.linien[0].ist_hoehe(&n) - 150.0).abs() < 1.0e-9);
    }

    #[test]
    fn fester_schnitt_ohne_rumpf_liefert_nan() {
        // Linie bei 190 cm: am hohen Zylinder gebaut, am kurzen nicht.
        let hoch = Netz::zylinder(100.0, 3000.0, 720, 20);
        let linien =
            messlinien_gemischt(&hoch, &[(Messort::Brust, Some(190.0))]).expect("Linien");
        let kurz = Netz::zylinder(100.0, 1500.0, 720, 20);
        let ist = messen(&kurz, &linien);
        assert!(ist.masse[0].is_nan());
    }

    #[test]
    fn massband_misst_den_zylinder_als_band() {
        let n = Netz::zylinder(100.0, 3000.0, 720, 20);
        let d = Arc::new(Messdaten::aus_netz(&n, crate::massband::segment::RUMPF));
        let linien = messlinien_gemischt(&n, &[(Messort::Taille, Some(150.0))])
            .expect("Linien")
            .mit_massband(d);
        let ist = messen(&n, &linien);
        // 720 Ecken, nach dem 0,5-cm-Filter jede sechste: die Sehnen sind
        // kaum kürzer als der Kreis.
        assert!((ist.masse[0] - 62.83).abs() < 0.01, "{}", ist.masse[0]);
    }

    #[test]
    fn massband_ohne_passende_messdaten_liefert_nan() {
        let n = Netz::zylinder(100.0, 3000.0, 720, 20);
        let fremd = Netz::zylinder(100.0, 3000.0, 36, 2);
        let d = Arc::new(Messdaten::aus_netz(&fremd, crate::massband::segment::RUMPF));
        let linien = messlinien_gemischt(&n, &[(Messort::Taille, Some(150.0))])
            .expect("Linien")
            .mit_massband(d);
        assert!(messen(&n, &linien).masse[0].is_nan());
    }

    #[test]
    fn aktiv_findet_die_fuenf_felder() {
        let namen: Vec<String> = [
            "HEIGHT_Total",
            "CIRCUMFERENCE_Bust",
            "CIRCUMFERENCE_UnderBust",
            "CIRCUMFERENCE_Waist",
            "CIRCUMFERENCE_HighHip",
            "CIRCUMFERENCE_LowHip",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let n = Netz::zylinder(100.0, 3000.0, 720, 20);
        let linien = messlinien(&n).expect("Linien");
        let aktiv = linien.aktiv(&namen).expect("Zuordnung");
        assert_eq!(aktiv, vec![1, 2, 3, 4, 5]);
    }

    /// Die Zahlen ihrer `.smis`, wie `person::zielperson` sie sieht.
    fn ihre_masse(name: &str) -> Option<f64> {
        Some(match name {
            "bust_circ" => 92.67,
            "lowbust_circ" => 76.0,
            "waist_circ" => 70.0,
            "highhip_circ" => 83.0,
            "hip_circ" => 90.0,
            "bust_arc_f" => 56.0,
            "bust_arc_b" => 37.0,
            "waist_arc_f" => 36.9,
            "waist_arc_b" => 34.1,
            "highhip_arc_f" => 44.0,
            "highhip_arc_b" => 41.0,
            "hip_arc_f" => 44.0,
            "hip_arc_b" => 46.0,
            "bustpoint_to_bustpoint" => 22.0,
            _ => return None,
        })
    }

    #[test]
    fn formziele_aus_ihrer_massdatei_schliessen_die_boegen() {
        let f = formziele_aus_smis(&ihre_masse);
        let wert = |ort: Messort, art: Messart| {
            f.ziele
                .iter()
                .find(|z| z.ort == ort && z.art == art)
                .map(|z| z.wert)
        };
        // Breite und Tiefe stehen nicht in der Datei und kommen aus der Tabelle.
        assert_eq!(wert(Messort::Brust, Messart::Breite), Some(28.16));
        assert_eq!(wert(Messort::Huefte, Messart::Tiefe), Some(20.81));
        // Der Brustpunktabstand ist ihrer.
        assert_eq!(wert(Messort::Brust, Messart::Brustpunkt), Some(22.0));
        // Die Bögen schliessen jetzt zum Umfang: vorn + hinten = P.
        for (ort, p) in [
            (Messort::Brust, 92.67),
            (Messort::Taille, 70.0),
            (Messort::HighHip, 83.0),
            (Messort::Huefte, 90.0),
        ] {
            let v = wert(ort, Messart::Vorn).expect("vorderer Bogen");
            let h = wert(ort, Messart::Hinten).expect("hinterer Bogen");
            assert!(
                (v + h - p).abs() < 1.0e-9,
                "{}: {v:.2} + {h:.2} statt {p:.2}",
                ort.name()
            );
        }
        // Brust: 56/37 gegen 92,67 → Lücke 0,33, je 0,165 abgezogen.
        let v = wert(Messort::Brust, Messart::Vorn).unwrap();
        let h = wert(Messort::Brust, Messart::Hinten).unwrap();
        assert!((v - 55.835).abs() < 1.0e-9, "{v}");
        assert!((h - 36.835).abs() < 1.0e-9, "{h}");
        // Die Unterbrust hat keine Bögen und bekommt keine.
        assert_eq!(wert(Messort::Unterbrust, Messart::Vorn), None);
        // Drei Lücken, also drei Meldungen; die Hüfte schliesst.
        assert_eq!(f.meldungen.len(), 3, "{:?}", f.meldungen);
        assert!(f.meldungen.iter().any(|m| m.contains("HighHip")), "{:?}", f.meldungen);
    }

    #[test]
    fn formziele_ohne_massdatei_tragen_nur_breite_und_tiefe() {
        let f = formziele_aus_smis(&|_| None);
        assert!(f.meldungen.is_empty());
        assert_eq!(f.ziele.len(), 2 * BREITE_TIEFE.len());
        assert!(f.ziele.iter().all(|z| z.art == Messart::Breite || z.art == Messart::Tiefe));
    }

    #[test]
    fn formziele_haengen_an_ihren_umfangslinien() {
        let n = Netz::zylinder(100.0, 3000.0, 720, 20);
        let f = formziele_aus_smis(&ihre_masse);
        // Der Zylinder hat alle fünf Linien; die Unterbrust bekommt keine
        // Formziele, also ist `mit_form` hier zufrieden.
        let linien = messlinien(&n).expect("Linien").mit_form(&f.ziele).expect("Form");
        assert_eq!(linien.len(), 5 + f.ziele.len());
        assert_eq!(linien.formlinien(), f.ziele.len());
        // Reihenfolge: je Ort erst der Umfang, dann die Formarten.
        let namen = linien.namen();
        let brust = namen
            .iter()
            .position(|s| s == "Brust")
            .expect("Brustlinie");
        assert_eq!(namen[brust + 1], "Brust Bogen vorn");
        assert_eq!(namen[brust + 2], "Brust Bogen hinten");
        assert_eq!(namen[brust + 3], "Brust Breite");
        assert_eq!(namen[brust + 4], "Brust Tiefe");
        assert_eq!(namen[brust + 5], "Brust Brustpunktabstand");
        // Das Gewicht ist die Abnahmegrenze des Issues.
        let tol = linien.toleranzen();
        assert_eq!(tol[brust], 0.2);
        assert_eq!(tol[brust + 1], 1.0);
        assert_eq!(tol[brust + 5], 0.5);
    }
}
