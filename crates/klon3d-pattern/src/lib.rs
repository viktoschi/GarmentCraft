//! Parser und Auswertung fuer Seamly2D-Muster (`.sm2d`).
//!
//! Aufbau des Musters:
//!
//! ```text
//! <pattern version="0.6.8" >
//!   <unit>cm</unit>
//!   <measurements>…</measurements>
//!   <increments>
//!     <increment name="…" formula="#A+#B"/>
//!   </increments>
//!   <draftBlock>          <!-- Konstruktionsblock -->
//!     <calculation>…</calculation>   <!-- Punkte, Linien, Splines -->
//!     <modeling>…</modeling>         <!-- Platzieren, Drehen, … -->
//!   </draftBlock>
//!   <pieces>              <!-- fertige Schnittteile -->
//!     <piece name="…"><nodes>…</nodes></piece>
//!   </pieces>
//! </pattern>
//! ```
//!
//! Wichtig: Das im Seamly2D-Repository veröffentlichte XSD (`pattern/v0.2.0.xsd`)
//! deckt dieses Format **nicht** ab. Das Modell hier ist aus echten Dateien
//! abgeleitet.

#![allow(non_snake_case)] // Tests sind deutsch benannt

pub mod formula;
pub mod boegen;
pub mod fadenlauf;
pub mod geom;
pub mod kanten;
pub mod kontur;
pub mod kurven;
pub mod lagevorschlag;
pub mod schleifen;
pub mod schneiden;
pub mod model;
pub mod operationen;
pub mod naehte;
pub mod nahtzugabe;
pub mod parse;
pub mod property;
pub mod saumkontur;
pub mod smis;
pub mod vorschlag;
pub mod zip;
pub mod zfab;
pub mod zprj;

pub use formula::{eval_str, Context, Expression, FormulaError, MapContext};
pub use model::{
    Arc, Calculation, DraftBlock, Increment, Line, Modeling, PathPoint, Pattern, Piece,
    PieceNode, PieceNodes, Point, Spline, Unit,
};
pub use parse::{read_path, read_str};
pub use smis::{lies_smis, lies_smis_str, Einheit, Smis};
pub use geom::{Auswertung, Pt};
pub use zprj::{Naht, Nahtseite, Projekt, Punkt2, Punkt3, Stoff, Teil, ZprjFehler};
pub use zfab::{Zfab, ZfabFehler};

/// Fehler beim Laden einer Musterdatei.
#[derive(Debug, thiserror::Error)]
pub enum PatternError {
    #[error("XML-Fehler: {0}")]
    Xml(String),

    #[error("unerwartetes Element <{0}>")]
    UnexpectedElement(String),

    #[error("unbekannte Messeinheit {0:?}")]
    UnsupportedUnit(String),

    #[error("Pfad {0} ist leer")]
    EmptyPath(String),

    #[error("Formel {expr:?} (bei {name:?}): {source}")]
    Formula {
        name: String,
        expr: String,
        #[source]
        source: FormulaError,
    },

    #[error("unbekannter Punkt {0:?}")]
    UnknownPoint(String),

    #[error("Punkttyp {0:?} wird noch nicht unterstützt")]
    UnsupportedPointType(String),

    #[error("Mass {0:?} ist in der Massdatei nicht definiert")]
    UnknownMeasurement(String),

    #[error("Mass ohne Namen")]
    MissingMeasurementName,

    #[error("Mass {0:?} hat keinen Wert")]
    MissingMeasurementValue(String),

    #[error("Mass {name:?} hat einen unlesbaren Wert {wert:?}")]
    InvalidMeasurementValue { name: String, wert: String },

    #[error("Mass {0:?} ist doppelt")]
    DuplicateMeasurement(String),

    #[error("Massdatei ohne <unit>")]
    MissingUnit,
}