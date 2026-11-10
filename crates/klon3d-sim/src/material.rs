//! Materialwerte in SI-Einheiten, aus CLO-Stoffprofilen übernommen.
//!
//! Die Felder folgen CLOs Stoffparametern (ADR-015, `MATERIAL.md`); für
//! Latex kommen Kette, Schuss und Diagonale aus **einer** Messung und liegen
//! deshalb fast aufeinander (0,002 % in der Dehnung, 0,17 % in der Biegung).
//! Umrechnung der CLO-Einheiten (Annahme aus `MATERIAL.md`, nicht belegt):
//!
//! | CLO | Einheit | hier |
//! |---|---|---|
//! | Stretch, Shear | g/s² = mN/m | N/m (× 10⁻³) |
//! | Bending | g·mm²/s²/rad | N·m (× 10⁻⁹) |
//! | Density (Flächengewicht) | g/m² | kg/m² (× 10⁻³) |
//! | Thickness | mm | m (× 10⁻³) |
//!
//! Die Plattenformel D = E·t³ / (12·(1 − ν²)) gibt für Radical Rubber 0,4 mm
//! etwa 6,2·10⁻⁶ N·m, gemessen sind 2,5·10⁻⁶ N·m — dieselbe Größenordnung,
//! die Umrechnung ist also plausibel.

/// Ein Stoff, wie der Solver ihn braucht.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    pub name: &'static str,
    /// Dehnsteifigkeit der Membran in **Schussrichtung** (CLO Stretch Weft,
    /// `fSuK`) in N/m.
    ///
    /// Die Richtung ist Teil des Namens, nicht Zierde: `szene.rs` (Kantenfedern),
    /// `saumfalte.rs` (Bandkanten) und `roehre.rs` lesen dieses Feld als die
    /// Steifigkeit, die sie meinen, und `Klartschnitt`/tests prüfen es. Die Kette
    /// steht getrennt in [`Self::kette_n_m`].
    pub dehnung_n_m: f64,
    /// Dehnsteifigkeit der Membran in **Kettrichtung** (CLO Stretch Warp,
    /// `fSvK`) in N/m. Für die acht Profile im Repo fast gleich dem Schuss
    /// (0,002 % Unterschied); getrennt geführt, weil ein echtes Gewebe sie
    /// trennt und die Membran beide Richtungen einzeln braucht.
    pub kette_n_m: f64,
    /// Schubsteifigkeit (CLO Shear, `fHK`) in N/m. Die Membran nimmt sie als
    /// dritte unabhängige Größe; ν folgt daraus (`ν = E/(2G) − 1`), es wird
    /// nicht gesetzt.
    pub schub_n_m: f64,
    /// Biegesteifigkeit **in Schussrichtung** (CLO Bending Weft, `fBuK`) in
    /// N·m.
    ///
    /// Die Richtung ist Teil des Namens, so wie bei [`Self::dehnung_n_m`]:
    /// `saumfalte.rs` (Bandkanten), `klebung.rs` (`Stoffpaar`) und
    /// `platzierung.rs` lesen dieses Feld und haben keinen eigenen Fadenlauf -
    /// sie meinen die Schussrichtung. `szene.rs` rechnet die Biegung je Kante
    /// dagegen seit #129 richtungsabhängig aus [`Self::biegung_kette_nm`],
    /// diesem Wert und [`Self::biegung_schraege_nm`].
    pub biegung_nm: f64,
    /// Biegesteifigkeit in **Kettrichtung** (CLO Bending Warp, `fBvK`) in N·m.
    /// Gleich [`Self::biegung_nm`] bei einem Stoff, der nur eine Zahl für alle
    /// Richtungen angibt; die acht Profile im Repo liegen 0,17 % darüber
    /// (Schuss 2482,2424 gegen Kette = Schräg 2486,4924 g·mm²/s²/rad).
    pub biegung_kette_nm: f64,
    /// Biegesteifigkeit unter **45°** (CLO Bending Bias, `fBhK`) in N·m.
    ///
    /// Der dritte Stützpunkt der Tensor-Rotation ([`crate::biegung`]) neben
    /// Schuss und Kette; er entscheidet, wie weit die Kurve zwischen beiden
    /// liegt. Die acht Profile im Repo geben hier denselben Wert wie für die
    /// Kette.
    pub biegung_schraege_nm: f64,
    /// Flächengewicht in kg/m².
    pub flaechengewicht_kg_m2: f64,
    /// Dicke in m; die Hälfte ist der Kollisionsabstand.
    pub dicke_m: f64,
    /// Reibungszahl (CLO Friction).
    pub reibung: f64,
}

/// Gemessenes CLO-Profil (Fabric Kit) `Latex_0.4mm_Radical_Rubber`
/// (`latex-stoffe/werte/latex_profile.csv`). Die Reibung 0,03 ist CLOs
/// Vorgabe, kein Messwert.
pub const RADICAL_RUBBER_04: Material = Material {
    name: "Latex_0.4mm_Radical_Rubber",
    dehnung_n_m: 348.274_156_2,
    kette_n_m: 348.281_718_8,
    schub_n_m: 174.159_046_9,
    biegung_nm: 2482.2424e-9,
    biegung_kette_nm: 2486.4924e-9,
    biegung_schraege_nm: 2486.4924e-9,
    flaechengewicht_kg_m2: 0.348_989_9,
    dicke_m: 0.4e-3,
    reibung: 0.03,
};

/// Retifist-Vorlage `NR-sheet-0.40` (`latex-stoffe/werte/retifist_nr_sheet.csv`):
/// gesetzte Werte für „weichen, fließenden Fall“, nicht gemessen
/// [retifist S.28–29]; Dehnung rund 10×, Biegung 41× weicher als Radical.
pub const RETIFIST_NR_040: Material = Material {
    name: "NR-sheet-0.40 (Retifist)",
    dehnung_n_m: 33.333_332,
    kette_n_m: 33.333_332,
    schub_n_m: 6.666_666_5,
    biegung_nm: 60.361e-9,
    // Set by hand for a soft drape, in one direction only - so the same in all
    // three. A real sheet is not.
    biegung_kette_nm: 60.361e-9,
    biegung_schraege_nm: 60.361e-9,
    flaechengewicht_kg_m2: 0.380,
    dicke_m: 0.4e-3,
    reibung: 0.03,
};

impl Material {
    /// Dieselbe Mischung in einer anderen Stärke `dicke` (m), nach den
    /// Rechenregeln in `latex-stoffe/LIESMICH.md` („Andere Stärken und
    /// Laminate", wie Retifist): Flächengewicht, Dehnung und Schub wachsen mit
    /// der Dicke, die Biegung mit ihrer dritten Potenz (Platte); Reibung
    /// bleibt. Ein Laminat zählt voll verklebt wie eine Folie der
    /// Gesamtdicke.
    pub fn mit_dicke(&self, dicke: f64) -> Material {
        let f = dicke / self.dicke_m;
        Material {
            dehnung_n_m: self.dehnung_n_m * f,
            kette_n_m: self.kette_n_m * f,
            schub_n_m: self.schub_n_m * f,
            biegung_nm: self.biegung_nm * f.powi(3),
            biegung_kette_nm: self.biegung_kette_nm * f.powi(3),
            biegung_schraege_nm: self.biegung_schraege_nm * f.powi(3),
            flaechengewicht_kg_m2: self.flaechengewicht_kg_m2 * f,
            dicke_m: dicke,
            ..*self
        }
    }

    /// Nach Kurznamen: `radical` oder `retifist`.
    pub fn nach_namen(name: &str) -> Option<Material> {
        match name.to_ascii_lowercase().as_str() {
            "radical" | "radical_rubber" | "latex_0.4mm_radical_rubber" => Some(RADICAL_RUBBER_04),
            "retifist" | "nr-sheet-0.40" | "nr_sheet_040" => Some(RETIFIST_NR_040),
            _ => None,
        }
    }
}
