//! DXF im AAMA/ASTM-Stil (R12) für CLO.
//!
//! Seamlys AAMA-Export legt die **Schnittkante** auf Ebene 1 und die Nahtlinie
//! als Innenlinie auf Ebene 8. Bei Latex ist die Zugabe eine Überlappung, die
//! Teilkante also die Nahtlinie. Mit CLOs „Swap Cutting Line and Sewing Line“
//! bleibt die alte Schnittkante als zweite Linie stehen, und die Klebezonen
//! werden Innenlinien (geprüft am 19.10.2026, `work/rock_dxf/LIESMICH.md`).
//!
//! Diese Datei hat deshalb die Form, die CLO in der früheren Fassung über
//! *File → Import (Add) → DXF (AAMA/ASTM)* **ohne** „Swap“ richtig liest — wie
//! `Rock_v34_CLO_nahtlinie_hauptteile.dxf`, die in CLO geprüft ist:
//!
//! * je Teil ein `BLOCK`, Kante = Nahtlinie als geschlossene `POLYLINE` auf
//!   Ebene 1, in mm, y nach oben;
//! * Passzeichen auf Ebene 4: `LINE` vom Punkt auf der Kante 4 mm nach innen;
//! * Teilename als `TEXT` auf Ebene 1;
//! * im `ENTITIES`-Abschnitt je Teil ein `INSERT` bei (0, 0).
//!
//! Innenlinien (Ebene 8) gibt es nicht.

use std::collections::HashSet;
use std::fmt::Write as _;

/// Punkt in mm, y nach oben.
pub type P = [f64; 2];

/// Länge eines Passzeichens in mm.
pub const KERBE_MM: f64 = 4.0;

/// Schrifthöhe des Teilenamens in mm — wie in Seamlys Export.
const SCHRIFT_MM: f64 = 2.5;

/// Punkte, die näher beieinander liegen, gelten als einer (mm).
const GLEICH_MM: f64 = 1e-6;

/// Ein Schnittteil, fertig in DXF-Koordinaten.
#[derive(Debug, Clone)]
pub struct Teil {
    pub name: String,
    /// Nahtlinie, geschlossen gedacht: der erste Punkt steht nicht am Ende.
    pub kante: Vec<P>,
    /// Passzeichen: Fuß auf der Kante, Ende innen.
    pub kerben: Vec<[P; 2]>,
}

// ───────────────────────────────────────────────────────── Geometrie ──

fn abstand(a: P, b: P) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn einheit(v: P) -> Option<P> {
    let l = v[0].hypot(v[1]);
    (l > 1e-12).then(|| [v[0] / l, v[1] / l])
}

/// Doppelte Punkte weg und den Ring offen lassen.
pub fn bereinige(pts: impl IntoIterator<Item = P>) -> Vec<P> {
    let mut out: Vec<P> = Vec::new();
    for p in pts {
        if out.last().map_or(true, |&q| abstand(q, p) > GLEICH_MM) {
            out.push(p);
        }
    }
    while out.len() > 1 && abstand(out[0], out[out.len() - 1]) <= GLEICH_MM {
        out.pop();
    }
    out
}

/// Länge des geschlossenen Rings.
pub fn ring_laenge(kante: &[P]) -> f64 {
    let n = kante.len();
    (0..n).map(|i| abstand(kante[i], kante[(i + 1) % n])).sum()
}

/// Hülle `[x0, y0, x1, y1]`.
pub fn huelle(pts: &[P]) -> [f64; 4] {
    pts.iter().fold(
        [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
        |h, p| [h[0].min(p[0]), h[1].min(p[1]), h[2].max(p[0]), h[3].max(p[1])],
    )
}

/// Doppelte Fläche mit Vorzeichen: positiv, wenn der Ring gegen den
/// Uhrzeigersinn läuft (y nach oben).
fn flaeche2(kante: &[P]) -> f64 {
    let n = kante.len();
    (0..n)
        .map(|i| {
            let (a, b) = (kante[i], kante[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// Passzeichen am Punkt `p`: Fuß auf der Kante, [`KERBE_MM`] nach innen.
///
/// Auf glatter Kante steht es senkrecht, an einer Ecke auf der
/// Winkelhalbierenden. „Innen“ ist links der Laufrichtung, wenn der Ring
/// gegen den Uhrzeigersinn läuft. `None`, wenn `p` weiter als `tol` mm von der
/// Kante liegt.
pub fn kerbe(kante: &[P], p: P, tol: f64) -> Option<[P; 2]> {
    let n = kante.len();
    if n < 3 {
        return None;
    }
    // Nächstes Segment des Rings.
    let mut best = (0usize, 0.0f64, f64::INFINITY);
    for i in 0..n {
        let (a, b) = (kante[i], kante[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l2 = dx * dx + dy * dy;
        let t = if l2 == 0.0 {
            0.0
        } else {
            (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
        };
        let d = abstand(p, [a[0] + t * dx, a[1] + t * dy]);
        if d < best.2 {
            best = (i, t, d);
        }
    }
    let (i, t, d) = best;
    if d > tol {
        return None;
    }
    let richtung = |k: usize| {
        let (a, b) = (kante[k % n], kante[(k + 1) % n]);
        einheit([b[0] - a[0], b[1] - a[1]]).unwrap_or([0.0, 0.0])
    };
    let (a, b) = (kante[i], kante[(i + 1) % n]);
    let seg = abstand(a, b);
    // Steht der Fuß auf einem Eckpunkt, zählen beide anliegenden Segmente.
    let (fuss, d1, d2) = if t * seg <= GLEICH_MM {
        (a, richtung(i + n - 1), richtung(i))
    } else if (1.0 - t) * seg <= GLEICH_MM {
        (b, richtung(i), richtung(i + 1))
    } else {
        ([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])], richtung(i), richtung(i))
    };
    let links = flaeche2(kante) > 0.0;
    let nach_innen = match einheit([d1[0] + d2[0], d1[1] + d2[1]]) {
        Some(tg) if links => [-tg[1], tg[0]],
        Some(tg) => [tg[1], -tg[0]],
        // Spitze: die Kante kehrt um, innen liegt zwischen beiden Schenkeln.
        None => einheit([d2[0] - d1[0], d2[1] - d1[1]])?,
    };
    let ende = [fuss[0] + KERBE_MM * nach_innen[0], fuss[1] + KERBE_MM * nach_innen[1]];
    Some([fuss, ende])
}

/// Ein Punkt im Teil für den Namen: Mitte der längsten Strecke im Teil auf
/// der Mittellinie der Hülle — waagerecht, bei hohen Teilen senkrecht.
/// Gibt Mittelpunkt, verfügbare Länge und ob senkrecht.
fn namensplatz(kante: &[P]) -> (P, f64, bool) {
    let h = huelle(kante);
    let senkrecht = h[3] - h[1] > h[2] - h[0];
    // Achse a läuft entlang des Textes, q quer dazu.
    let (a, q) = if senkrecht { (1, 0) } else { (0, 1) };
    let mitte_q = (h[q] + h[q + 2]) / 2.0;
    let n = kante.len();
    let mut schnitte: Vec<f64> = (0..n)
        .filter_map(|i| {
            let (u, v) = (kante[i], kante[(i + 1) % n]);
            ((u[q] > mitte_q) != (v[q] > mitte_q))
                .then(|| u[a] + (mitte_q - u[q]) / (v[q] - u[q]) * (v[a] - u[a]))
        })
        .collect();
    schnitte.sort_by(|x, y| x.total_cmp(y));
    let (mut von, mut bis) = (h[a], h[a + 2]);
    let mut beste = -1.0;
    for s in schnitte.chunks_exact(2) {
        if s[1] - s[0] > beste {
            beste = s[1] - s[0];
            (von, bis) = (s[0], s[1]);
        }
    }
    let mut m = [0.0; 2];
    m[a] = (von + bis) / 2.0;
    m[q] = mitte_q;
    (m, bis - von, senkrecht)
}

// ─────────────────────────────────────────────────────────── Namen ──

/// Windows-1252 für ein Zeichen, falls es dort vorkommt.
fn cp1252(c: char) -> Option<u8> {
    let u = c as u32;
    if u < 0x80 || (0xA0..=0xFF).contains(&u) {
        return Some(u as u8);
    }
    Some(match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        'Ž' => 0x8E,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9A,
        '›' => 0x9B,
        'œ' => 0x9C,
        'ž' => 0x9E,
        'Ÿ' => 0x9F,
        _ => return None,
    })
}

/// Blockname wie in Seamlys Export: groß, Leerzeichen als `_`.
///
/// `1 BUND - Umschlag, ganzer Umfang` → `1_BUND_-_UMSCHLAG,_GANZER_UMFANG`.
/// Zeichen, die in DXF-Namen nicht stehen dürfen, werden ebenfalls `_`.
pub fn blockname(name: &str) -> String {
    let n: String = name
        .trim()
        .chars()
        .flat_map(char::to_uppercase)
        .map(|c| match c {
            ' ' | '<' | '>' | '/' | '\\' | '"' | ':' | ';' | '?' | '*' | '|' | '=' | '`' | '\'' => '_',
            c if c.is_control() || cp1252(c).is_none() => '_',
            c => c,
        })
        .collect();
    if n.is_empty() {
        "TEIL".into()
    } else {
        n
    }
}

/// Blocknamen, eindeutig gemacht: Doppelte bekommen `_2`, `_3`, …
fn blocknamen(teile: &[Teil]) -> Vec<String> {
    let mut vergeben = HashSet::new();
    teile
        .iter()
        .map(|t| {
            let grund = blockname(&t.name);
            let mut name = grund.clone();
            let mut k = 2;
            while !vergeben.insert(name.clone()) {
                name = format!("{grund}_{k}");
                k += 1;
            }
            name
        })
        .collect()
}

/// Text für ein `TEXT`-Element: was nicht in Windows-1252 steht, als `\U+XXXX`.
fn textwert(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().filter(|c| !c.is_control()) {
        if cp1252(c).is_some() {
            out.push(c);
        } else {
            let _ = write!(out, "\\U+{:04X}", c as u32);
        }
    }
    out
}

// ───────────────────────────────────────────────────────── Schreiben ──

/// Zahl mit höchstens sechs Nachkommastellen, ohne Exponent und ohne `-0`.
fn zahl(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// Gruppencodes wie Seamlys Export (`dxfrw`): Code rechtsbündig, CRLF.
struct Schreiber {
    out: Vec<u8>,
    handle: u32,
}

impl Schreiber {
    fn new(handle: u32) -> Self {
        Self { out: Vec::new(), handle }
    }

    fn code(&mut self, c: u16, v: &str) {
        self.out.extend(format!("{c:>3}\r\n").bytes());
        self.out.extend(v.chars().map(|c| cp1252(c).unwrap_or(b'?')));
        self.out.extend(b"\r\n");
    }

    fn int(&mut self, c: u16, v: i32) {
        self.code(c, &format!("{v:>6}"));
    }

    fn zahl(&mut self, c: u16, v: f64) {
        self.code(c, &zahl(v));
    }

    fn punkt(&mut self, c: u16, p: P) {
        self.zahl(c, p[0]);
        self.zahl(c + 10, p[1]);
    }

    fn variable(&mut self, name: &str) {
        self.code(9, name);
    }

    /// Kopf eines grafischen Elements: Handle, Ebene, Linientyp und Farbe
    /// nach Ebene.
    fn element(&mut self, typ: &str, ebene: &str) {
        self.code(0, typ);
        self.handle += 1;
        let h = format!("{:X}", self.handle);
        self.code(5, &h);
        self.code(8, ebene);
        self.code(6, "BYLAYER");
        self.int(62, 256);
    }

    fn tabelle(&mut self, name: &str, anzahl: i32) {
        self.code(0, "TABLE");
        self.code(2, name);
        self.int(70, anzahl);
    }

    fn ebene(&mut self, name: &str) {
        self.code(0, "LAYER");
        self.code(2, name);
        self.int(70, 0);
        self.int(62, 7);
        self.code(6, "CONTINUOUS");
    }

    fn linientyp(&mut self, name: &str, text: &str) {
        self.code(0, "LTYPE");
        self.code(2, name);
        self.int(70, 0);
        self.code(3, text);
        self.int(72, 65);
        self.int(73, 0);
        self.zahl(40, 0.0);
    }

    fn block(&mut self, name: &str, ebene: &str) {
        self.code(0, "BLOCK");
        self.code(8, ebene);
        self.code(2, name);
        self.int(70, 0);
        self.punkt(10, [0.0, 0.0]);
        self.zahl(30, 0.0);
        self.code(3, name);
        self.code(1, "");
    }

    fn endblk(&mut self, ebene: &str) {
        self.code(0, "ENDBLK");
        self.code(8, ebene);
    }
}

/// Die ganze Datei, kodiert in Windows-1252.
pub fn schreibe(teile: &[Teil]) -> Vec<u8> {
    let namen = blocknamen(teile);

    // Blöcke und Elemente zuerst: der Kopf braucht den nächsten freien Handle.
    let mut k = Schreiber::new(0x20);
    k.code(0, "SECTION");
    k.code(2, "BLOCKS");
    for leer in ["$MODEL_SPACE", "$PAPER_SPACE"] {
        k.block(leer, "0");
        k.endblk("0");
    }
    for (t, name) in teile.iter().zip(&namen) {
        k.block(name, "1");

        k.element("POLYLINE", "1");
        k.int(66, 1);
        k.punkt(10, [0.0, 0.0]);
        k.zahl(30, 0.0);
        k.int(70, 1); // geschlossen
        for &p in &t.kante {
            k.element("VERTEX", "1");
            k.punkt(10, p);
            k.zahl(30, 0.0);
        }
        k.element("SEQEND", "1");

        for &[a, b] in &t.kerben {
            k.element("LINE", "4");
            k.punkt(10, a);
            k.punkt(11, b);
        }

        let text = textwert(&t.name);
        let (mitte, platz, senkrecht) = namensplatz(&t.kante);
        // Breite eines Zeichens etwa 0,8 Schrifthöhen; zu lange Namen werden
        // kleiner, damit sie im Teil bleiben.
        let zeichen = text.chars().count().max(1) as f64;
        let hoehe = SCHRIFT_MM.min(0.9 * platz / (0.8 * zeichen)).max(0.1);
        let halb = 0.4 * hoehe * zeichen;
        let anfang = if senkrecht {
            [mitte[0] + hoehe / 2.0, mitte[1] - halb]
        } else {
            [mitte[0] - halb, mitte[1] - hoehe / 2.0]
        };
        k.element("TEXT", "1");
        k.punkt(10, anfang);
        k.zahl(30, 0.0);
        k.zahl(40, hoehe);
        k.code(1, &text);
        k.zahl(50, if senkrecht { 90.0 } else { 0.0 });
        k.zahl(41, 1.0);
        k.zahl(51, 0.0);
        k.code(7, "STANDARD");
        k.int(71, 0);
        k.zahl(210, 0.0);
        k.zahl(220, 0.0);
        k.zahl(230, 1.0);

        k.endblk("1");
    }
    k.code(0, "ENDSEC");

    k.code(0, "SECTION");
    k.code(2, "ENTITIES");
    for name in &namen {
        k.element("INSERT", "1");
        k.code(2, name);
        k.punkt(10, [0.0, 0.0]);
        k.zahl(30, 0.0);
        k.zahl(41, 1.0);
        k.zahl(42, 1.0);
        k.zahl(43, 1.0);
        k.zahl(50, 0.0);
        k.int(70, 1);
        k.int(71, 1);
        k.zahl(44, 0.0);
        k.zahl(45, 0.0);
    }
    k.code(0, "ENDSEC");
    k.code(0, "EOF");

    let alle: Vec<P> = teile
        .iter()
        .flat_map(|t| t.kante.iter().copied().chain(t.kerben.iter().flatten().copied()))
        .collect();
    let h = if alle.is_empty() { [0.0; 4] } else { huelle(&alle) };

    let mut s = Schreiber::new(0);
    s.code(999, concat!("klon3d ", env!("CARGO_PKG_VERSION")));
    s.code(0, "SECTION");
    s.code(2, "HEADER");
    s.variable("$ACADVER");
    s.code(1, "AC1009");
    s.variable("$DWGCODEPAGE");
    s.code(3, "ANSI_1252");
    s.variable("$INSBASE");
    s.punkt(10, [0.0, 0.0]);
    s.zahl(30, 0.0);
    s.variable("$EXTMIN");
    s.punkt(10, [h[0], h[1]]);
    s.zahl(30, 0.0);
    s.variable("$EXTMAX");
    s.punkt(10, [h[2], h[3]]);
    s.zahl(30, 0.0);
    s.variable("$LIMMIN");
    s.punkt(10, [h[0], h[1]]);
    s.variable("$LIMMAX");
    s.punkt(10, [h[2], h[3]]);
    s.variable("$TEXTSIZE");
    s.zahl(40, SCHRIFT_MM);
    s.variable("$TEXTSTYLE");
    s.code(7, "STANDARD");
    s.variable("$CLAYER");
    s.code(8, "1");
    s.variable("$LUNITS");
    s.int(70, 2);
    s.variable("$HANDLING");
    s.int(70, 1);
    s.variable("$HANDSEED");
    let saat = format!("{:X}", k.handle + 1);
    s.code(5, &saat);
    s.code(0, "ENDSEC");

    s.code(0, "SECTION");
    s.code(2, "TABLES");
    s.tabelle("VPORT", 1);
    s.code(0, "VPORT");
    s.code(2, "*ACTIVE");
    s.int(70, 0);
    s.punkt(10, [0.0, 0.0]);
    s.punkt(11, [1.0, 1.0]);
    s.punkt(12, [(h[0] + h[2]) / 2.0, (h[1] + h[3]) / 2.0]);
    s.zahl(40, ((h[3] - h[1]) * 1.1).max(1.0));
    s.zahl(41, 1.5);
    s.code(0, "ENDTAB");
    s.tabelle("LTYPE", 3);
    s.linientyp("BYBLOCK", "");
    s.linientyp("BYLAYER", "");
    s.linientyp("CONTINUOUS", "Solid line");
    s.code(0, "ENDTAB");
    s.tabelle("LAYER", 3);
    for e in ["0", "1", "4"] {
        s.ebene(e);
    }
    s.code(0, "ENDTAB");
    s.tabelle("STYLE", 1);
    s.code(0, "STYLE");
    s.code(2, "STANDARD");
    s.int(70, 0);
    s.zahl(40, 0.0);
    s.zahl(41, 1.0);
    s.zahl(50, 0.0);
    s.int(71, 0);
    s.zahl(42, SCHRIFT_MM);
    s.code(3, "txt");
    s.code(4, "");
    s.code(0, "ENDTAB");
    for leer in ["VIEW", "UCS"] {
        s.tabelle(leer, 0);
        s.code(0, "ENDTAB");
    }
    s.tabelle("APPID", 1);
    s.code(0, "APPID");
    s.code(2, "ACAD");
    s.int(70, 0);
    s.code(0, "ENDTAB");
    s.tabelle("DIMSTYLE", 1);
    s.code(0, "DIMSTYLE");
    s.code(2, "STANDARD");
    s.int(70, 0);
    s.code(0, "ENDTAB");
    s.code(0, "ENDSEC");

    s.out.extend(k.out);
    s.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blockname_wie_seamly() {
        assert_eq!(
            blockname("1 BUND - Umschlag, ganzer Umfang, Naht hinten"),
            "1_BUND_-_UMSCHLAG,_GANZER_UMFANG,_NAHT_HINTEN"
        );
        assert_eq!(blockname("Ärmel: links/rechts"), "ÄRMEL__LINKS_RECHTS");
        assert_eq!(blockname("  "), "TEIL");
    }

    #[test]
    fn doppelte_namen_werden_eindeutig() {
        let t = |n: &str| Teil { name: n.into(), kante: vec![], kerben: vec![] };
        let n = blocknamen(&[t("a b"), t("A B"), t("a_b")]);
        assert_eq!(n, ["A_B", "A_B_2", "A_B_3"]);
    }

    #[test]
    fn zahlen_ohne_exponent_und_ohne_minus_null() {
        assert_eq!(zahl(0.0), "0");
        assert_eq!(zahl(-1e-9), "0");
        assert_eq!(zahl(2546.6), "2546.6");
        assert_eq!(zahl(1e20), "100000000000000000000");
        assert_eq!(zahl(-3.25), "-3.25");
    }

    #[test]
    fn text_in_windows_1252() {
        assert_eq!(textwert("Größe – 2"), "Größe – 2");
        assert_eq!(textwert("Zwickel ∆"), "Zwickel \\U+2206");
        let mut s = Schreiber::new(0);
        s.code(1, "Ä–");
        assert_eq!(s.out, b"  1\r\n\xC4\x96\r\n");
    }

    #[test]
    fn ring_ohne_doppelte_punkte() {
        let r = bereinige([[0.0, 0.0], [1.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0]]);
        assert_eq!(r, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]);
    }

    /// Quadrat 10 × 10 mm, beide Laufrichtungen: das Passzeichen zeigt immer
    /// nach innen, an der Ecke auf der Winkelhalbierenden.
    #[test]
    fn kerbe_zeigt_nach_innen() {
        let gegen = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let mit: Vec<P> = gegen.iter().rev().copied().collect();
        for q in [&gegen, &mit] {
            let [f, e] = kerbe(q, [5.0, 0.0], 0.01).unwrap();
            assert!(abstand(f, [5.0, 0.0]) < 1e-12);
            assert!(abstand(e, [5.0, 4.0]) < 1e-12, "{e:?}");

            let [f, e] = kerbe(q, [10.0, 10.0], 0.01).unwrap();
            assert_eq!(f, [10.0, 10.0]);
            let w = KERBE_MM / 2f64.sqrt();
            assert!(abstand(e, [10.0 - w, 10.0 - w]) < 1e-12, "{e:?}");
        }
        assert!(kerbe(&gegen, [5.0, 1.0], 0.01).is_none());
    }

    #[test]
    fn name_steht_im_teil() {
        // L-Form: die Mitte der Hülle liegt außerhalb.
        let l = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 10.0], [10.0, 10.0], [10.0, 100.0], [0.0, 100.0]];
        let (m, platz, senkrecht) = namensplatz(&l);
        assert!(!senkrecht);
        assert_eq!(m, [5.0, 50.0]);
        assert_eq!(platz, 10.0);
    }
}
