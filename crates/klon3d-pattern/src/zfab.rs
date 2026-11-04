//! CLO fabrics (`.zfab`) read from their property stream.
//!
//! MATERIAL.md documents eight latex fabrics in `assets/stoffe/` and states the
//! open point: they sit in the repository but no Rust code can open one, so no
//! value ever reaches the solver. This module closes that gap - it reads, it
//! does not connect anything.
//!
//! ## What a `.zfab` is
//!
//! ```text
//! Latex_0.4mm_Radical_Rubber.zfab   3220 byte
//!   head      `u32[0x20]` = 515, `u32[0x24]` = 2705, 515 + 2705 = 3220
//!   ZIP       at 515, two entries: `*.fab` and `clofiles.json`
//!     .fab    12307 byte, one map block, starts at byte 0 (**no** head)
//!       mapPhysical   33 fields, `fThickness`, `fDensity`, `fSuK`, ...
//! ```
//!
//! The grammar is the one [`crate::property`] implements; `.zfab` only adds the
//! 515-byte head. The `.fab` inside carries no head of its own, exactly like a
//! project's `.pac` (finding 58) - so [`property::Leser::neu`] starts at 0.
//!
//! The rule from `../latex-stoffe/FORMAT.md` that the property tree is fully
//! understood holds here too: a complete `.fab` read and written back is
//! byte-identical, therefore a correct reader lands exactly on the end of the
//! block. All eight files end on byte 0 of the remainder, top level and
//! `mapPhysical` alike.
//!
//! ## What is read, and what is not
//!
//! Read are the linear values MATERIAL.md lists: thickness, areal weight,
//! stretch, shear, bending, buckling, friction, damping, plus the profile name
//! and whether the raw Fabric Kit data (`mapRawData`) is present. Units are
//! CLO's, unchanged - `fThickness` mm, `fDensity` g/mm³, `fSuK`/`fSvK`/`fHK`
//! g/s², `fBuK`/`fBvK`/`fBhK` g·mm²/s²/rad.
//!
//! **Not** read: `mapColorwayInfo` and the textures (colour, not material), and
//! the nonlinear curves `listNonlinearSuK`/`SvK`/`HK`/`LeftShearK` and the
//! `MV*LengthList`/`MV*ForceList` in `mapRawData`. They are present in every
//! file; MATERIAL.md documents the linear values, and a curve without a checked
//! interpretation would be a guess.

use std::path::Path;

use crate::property::{self, Blatt, Map};
use crate::zip::{Archiv, ZipFehler};

/// First 16 bytes of every `.zfab`: type code, spaces, `CLO`.
pub const KOPF_MAGIC: &[u8] = b" FAB        CLO ";

/// Error when reading a `.zfab`.
#[derive(Debug, thiserror::Error)]
pub enum ZfabFehler {
    #[error("Datei {0:?} nicht lesbar: {1}")]
    Io(String, #[source] std::io::Error),

    #[error("{0:?}: kein .zfab-Kopf (16 Byte \" FAB        CLO \", u32[0x20] + u32[0x24] == Dateilänge)")]
    KeinKopf(String),

    #[error("{0:?}: kein .fab im Archiv")]
    KeineFab(String),

    #[error("{0:?}: .fab endet {1} Byte vor dem Dateiende, der Block ist nicht vollständig gelesen")]
    Rest(String, usize),

    #[error("{0:?}: mapPhysical fehlt oder ist keine Map")]
    KeinMapPhysical(String),

    #[error("ZIP: {0}")]
    Zip(#[from] ZipFehler),

    #[error("Property-Stream: {0}")]
    Stream(#[from] property::StreamFehler),
}

/// One fabric as CLO stores it: the material values MATERIAL.md names.
///
/// Every physical value is `Option`: a fabric may omit a field, and a missing
/// bending stiffness is "not stated", not "zero". `0.0` in the file is a real
/// zero and stays `Some(0.0)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Zfab {
    /// `qsName`, CLO's short name (`Latex_0.4mm_Radical_Rubber`).
    pub name: Option<String>,
    /// `mapPhysical.qsPhysicalPropertyName` - the profile's name.
    pub profil: Option<String>,
    /// `mapPhysical.fThickness`, in mm.
    pub dicke_mm: Option<f64>,
    /// `mapPhysical.fDensity`, in g/mm³. Multiply by 1e6 for g/m²; use
    /// [`Zfab::gewicht_g_m2`].
    pub dichte_g_mm3: Option<f64>,
    /// `fSuK` - stretch, weft, g/s².
    pub stretch_weft_g_s2: Option<f64>,
    /// `fSvK` - stretch, warp, g/s².
    pub stretch_warp_g_s2: Option<f64>,
    /// `fHK` - shear, g/s². In latex half the stretch (MATERIAL.md).
    pub shear_g_s2: Option<f64>,
    /// `fBuK` - bending, weft, g·mm²/s²/rad.
    pub bending_weft: Option<f64>,
    /// `fBvK` - bending, warp, g·mm²/s²/rad.
    pub bending_warp: Option<f64>,
    /// `fBhK` - bending, bias, g·mm²/s²/rad.
    pub bending_bias: Option<f64>,
    /// `fBuLR` - buckling ratio, 0..1.
    pub buckling_ratio: Option<f64>,
    /// `fBucklingStiffnessU` - buckling stiffness, weft.
    pub buckling_stiffness_weft: Option<f64>,
    /// `fBucklingStiffnessV` - buckling stiffness, warp.
    pub buckling_stiffness_warp: Option<f64>,
    /// `fBucklingStiffnessH` - buckling stiffness, bias.
    pub buckling_stiffness_bias: Option<f64>,
    /// `fFriction` - friction coefficient, dimensionless.
    pub reibung: Option<f64>,
    /// `fIDS` - internal damping, dimensionless.
    pub daempfung: Option<f64>,
    /// Whether `mapPhysical.mapRawData` exists - the Fabric Kit measurement
    /// behind the derived values.
    pub hat_rohdaten: bool,
}

impl Zfab {
    /// Areal weight in g/m²: `fDensity` × 1e6, the same conversion
    /// [`crate::zprj::Stoff::gewicht_g_m2`] uses (0.00034899 → 349).
    pub fn gewicht_g_m2(&self) -> Option<f64> {
        self.dichte_g_mm3.map(|d| d * 1.0e6)
    }

    /// Reads a fabric from disk.
    pub fn lies(pfad: impl AsRef<Path>) -> Result<Self, ZfabFehler> {
        let pfad = pfad.as_ref();
        let daten = std::fs::read(pfad)
            .map_err(|e| ZfabFehler::Io(pfad.display().to_string(), e))?;
        Self::lies_bytes(&daten, &pfad.display().to_string())
    }

    /// Reads a fabric from memory. `name` only appears in error messages.
    pub fn lies_bytes(daten: &[u8], name: &str) -> Result<Self, ZfabFehler> {
        // The 16 byte type code `" FAB        CLO "` first: a foreign file is
        // refused by name and not parsed as garbage. `hat_kopf` then checks
        // that the ZIP boundaries inside the head add up to the file length.
        if !daten.starts_with(KOPF_MAGIC) || !property::hat_kopf(daten) {
            return Err(ZfabFehler::KeinKopf(name.to_string()));
        }
        // `hat_kopf` has checked both bounds and the sum.
        let basis = u32::from_le_bytes(daten[0x20..0x24].try_into().unwrap()) as usize;
        let laenge = u32::from_le_bytes(daten[0x24..0x28].try_into().unwrap()) as usize;

        let archiv = Archiv::neu(&daten[basis..basis + laenge])?;
        if archiv.erster_mit_endung(".fab").is_none() {
            return Err(ZfabFehler::KeineFab(name.to_string()));
        }
        let fab = archiv.lies_mit_endung(".fab")?;

        // The `.fab` has no head: the map block starts at byte 0 (like a `.pac`).
        let mut leser = property::Leser::neu(&fab).mit_pfad("fab");
        let wurzel = leser.map(None)?;
        if leser.rest() != 0 {
            return Err(ZfabFehler::Rest(name.to_string(), leser.rest()));
        }

        let physik = wurzel
            .pfad_map(&["mapPhysical"])
            .ok_or_else(|| ZfabFehler::KeinMapPhysical(name.to_string()))?;

        Ok(Self {
            name: text(&wurzel, "qsName"),
            profil: text(physik, "qsPhysicalPropertyName"),
            dicke_mm: zahl(physik, "fThickness"),
            dichte_g_mm3: zahl(physik, "fDensity"),
            stretch_weft_g_s2: zahl(physik, "fSuK"),
            stretch_warp_g_s2: zahl(physik, "fSvK"),
            shear_g_s2: zahl(physik, "fHK"),
            bending_weft: zahl(physik, "fBuK"),
            bending_warp: zahl(physik, "fBvK"),
            bending_bias: zahl(physik, "fBhK"),
            buckling_ratio: zahl(physik, "fBuLR"),
            buckling_stiffness_weft: zahl(physik, "fBucklingStiffnessU"),
            buckling_stiffness_warp: zahl(physik, "fBucklingStiffnessV"),
            buckling_stiffness_bias: zahl(physik, "fBucklingStiffnessH"),
            reibung: zahl(physik, "fFriction"),
            daempfung: zahl(physik, "fIDS"),
            hat_rohdaten: physik.hat("mapRawData"),
        })
    }
}

/// A `f32` field as `f64`, `None` when absent or not four bytes.
fn zahl(m: &Map, name: &str) -> Option<f64> {
    m.feld(name)?.als_blatt()?.als_f32().map(f64::from)
}

/// A text field.
fn text(m: &Map, name: &str) -> Option<String> {
    let b: &Blatt = m.feld(name)?.als_blatt()?;
    b.als_text()
}
