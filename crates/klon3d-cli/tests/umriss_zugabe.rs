//! `klon3d umriss` prints the cutting line and `--svg` draws it next to the
//! seam line.
//!
//! The contour is checked against an independent evaluation of
//! [`Saumkontur::aus_teilrand`] with [`Ecken::Eigen`] on the same file - not
//! against a number copied out of the program: a copy would pass even when both
//! sides drift together. The lengths are cross-checked against the ones the DXF
//! export measured in #78 (Vorne 2557.01, Hinten 2847.36, Bund 1526.36 mm).

use std::path::{Path, PathBuf};
use std::process::Command;

use klon3d_pattern::geom::{Auswertung, lies_messungen};
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::saumkontur::{Ecken, Saumkontur};
use klon3d_pattern::{Pattern, Pt, read_path};

const EPS: f64 = 1e-9;

fn musterordner() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster")
}

/// Load a pattern and its measurement file the way the command line does.
fn laden(pfad: &Path) -> (Pattern, std::collections::HashMap<String, f64>) {
    let muster = read_path(pfad).expect("pattern readable");
    let mess = muster
        .measurements
        .as_ref()
        .map(|m| {
            let p = Path::new(m);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                pfad.parent().unwrap_or(Path::new(".")).join(p)
            }
        })
        .filter(|p| p.is_file())
        .map(|p| lies_messungen(&p, &muster.unit).expect("measurements readable"))
        .unwrap_or_default();
    (muster, mess)
}

fn runde_datei(name: &str) -> PathBuf {
    musterordner().join(name)
}

fn lauf(muster: &Path, args: &[&str]) -> String {
    let o = Command::new(env!("CARGO_BIN_EXE_klon3d"))
        .arg("umriss")
        .arg(muster)
        .args(args)
        .output()
        .expect("klon3d starts");
    let t = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(o.status.success(), "{t}");
    t
}

/// The printed cutting-line column of a piece, `None` when the row has none.
fn gedruckter_schnitt(ausgabe: &str, teil: &str) -> Option<f64> {
    let zeile = ausgabe
        .lines()
        .find(|z| z.contains("Knoten") && z.contains(teil))
        .unwrap_or_else(|| panic!("keine Zeile fuer {teil} in\n{ausgabe}"));
    let rest = zeile.split("mm Schnitt").next()?;
    if !zeile.contains("mm Schnitt") {
        return None;
    }
    let zahl = rest.split_whitespace().last()?;
    Some(
        zahl.parse()
            .unwrap_or_else(|e| panic!("{zahl:?} in {zeile}: {e}")),
    )
}

/// An independent evaluation of the cutting line for one piece: the same
/// construction, its own deduplication, in millimetres.
fn erwartete_kontur(muster: &Path, teil: &str) -> Option<Vec<Pt>> {
    let (m, mess) = laden(muster);
    let ev = Auswertung::new(&m, mess);
    let f = m.unit_to_mm().unwrap_or(1.0);
    let teile = m.pieces().to_vec();
    let t = teile
        .iter()
        .find(|t| t.name.as_deref().unwrap_or_default().contains(teil))
        .unwrap_or_else(|| panic!("{teil} not in {}", muster.display()));
    let rand = ev.teilrand(t, Kurvenquelle::Seamly).ok()?;
    if !rand
        .punktknoten
        .iter()
        .any(|k| k.zugabe_nach.unwrap_or(0.0).abs() > EPS)
    {
        return None;
    }
    let roh = Saumkontur::aus_teilrand(&rand, Ecken::Eigen)
        .punkte()
        .to_vec();
    let mut out: Vec<Pt> = Vec::with_capacity(roh.len());
    for p in roh {
        if out.last().is_none_or(|q: &Pt| q.dist(p) > EPS) {
            out.push(p);
        }
    }
    while out.len() > 1 && out[0].dist(*out.last().unwrap()) <= EPS {
        out.pop();
    }
    (out.len() >= 3).then(|| {
        out.into_iter()
            .map(|p| Pt {
                x: p.x * f,
                y: p.y * f,
            })
            .collect()
    })
}

fn ringlaenge(ring: &[Pt]) -> f64 {
    (0..ring.len())
        .map(|i| ring[i].dist(ring[(i + 1) % ring.len()]))
        .sum()
}

/// Count the closed paths of one stroke group of the SVG, in document order.
fn svg_gruppen(svg: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find("<g fill=\"none\"") {
        rest = &rest[i..];
        let Some(ende) = rest.find("</g>") else { break };
        let gruppe = &rest[..ende];
        let farbe = gruppe
            .split("stroke=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap_or("?")
            .to_string();
        out.push((farbe, gruppe.matches("<path ").count()));
        rest = &rest[ende..];
    }
    out
}

#[test]
fn bluse3_rechnet_die_schnittlinie_wie_eine_eigene_auswertung() {
    let muster = runde_datei("bluse3_v2.sm2d");
    let ausgabe = lauf(&muster, &[]);
    // 28 pieces, 16 of them carry an allowance (#SeamOverlap 0.6 = 6 mm).
    // The trims, the stepped stand collar, the donuts and the six small
    // washers are glued flat and carry none.
    let zeilen: Vec<&str> = ausgabe.lines().filter(|z| z.contains("Knoten")).collect();
    assert_eq!(zeilen.len(), 28, "{ausgabe}");
    let mit = zeilen.iter().filter(|z| z.contains("mm Schnitt")).count();
    assert_eq!(mit, 16, "{ausgabe}");

    for name in [
        "Rueckenteil Mitte Links",
        "Rueckenteil Seite Rechts",
        "Oberteil vorn Links",
    ] {
        let want =
            erwartete_kontur(&muster, name).unwrap_or_else(|| panic!("{name} has a contour"));
        let gedruckt =
            gedruckter_schnitt(&ausgabe, name).unwrap_or_else(|| panic!("{name} prints 0"));
        let soll = ringlaenge(&want);
        assert!(
            (gedruckt - soll).abs() <= 0.05,
            "{name}: gedruckt {gedruckt:.3} gegen {soll:.3} mm"
        );
    }

    // A piece without an allowance prints no column at all, not `0.0`.
    assert_eq!(gedruckter_schnitt(&ausgabe, "Trim Oeffnung Links"), None);
    assert_eq!(
        gedruckter_schnitt(&ausgabe, "Stehkragen Aussen gestuft"),
        None
    );
}

#[test]
fn rock_teilt_die_teile_in_mit_und_ohne_zugabe() {
    let muster = runde_datei("rock_2schlitze_v34.sm2d");
    let ordner = std::env::temp_dir().join(format!("klon3d-umriss-{}", std::process::id()));
    std::fs::create_dir_all(&ordner).unwrap();
    let svg_pfad = ordner.join("rock.svg");
    let ausgabe = lauf(&muster, &["--svg", svg_pfad.to_str().unwrap()]);

    let zeilen: Vec<&str> = ausgabe.lines().filter(|z| z.contains("Knoten")).collect();
    assert_eq!(zeilen.len(), 23, "{ausgabe}");
    let mit = zeilen.iter().filter(|z| z.contains("mm Schnitt")).count();
    let ohne = zeilen.len() - mit;
    // The four panels and the waistband carry the allowance; the bindings,
    // the reinforcements, the donuts and the hem trims do not.
    assert_eq!((mit, ohne), (5, 18), "{ausgabe}");

    // The three lengths #78 measured on the DXF export.
    let nah = |teil: &str| gedruckter_schnitt(&ausgabe, teil).unwrap();
    assert!((nah("2 VORNE RECHTS") - 2557.01).abs() <= 0.1, "Vorne");
    assert!((nah("4 HINTEN RECHTS") - 2847.36).abs() <= 0.1, "Hinten");
    assert!((nah("UND - Umschlag") - 1526.36).abs() <= 0.1, "Bund");

    let svg = std::fs::read_to_string(&svg_pfad).unwrap();
    let gruppen = svg_gruppen(&svg);
    assert_eq!(
        gruppen.len(),
        2,
        "eine graue und eine schwarze Gruppe: {gruppen:?}"
    );
    assert_eq!(gruppen[0].0, "#808080");
    assert_eq!(gruppen[1].0, "black");
    assert_eq!(gruppen[0].1, mit, "graue Pfade = Teile mit Zugabe");
    assert_eq!(gruppen[1].1, zeilen.len(), "schwarze Pfade = alle Teile");
    std::fs::remove_dir_all(&ordner).ok();
}

#[test]
fn svg_legt_die_schnittlinie_vor_die_naht() {
    let muster = runde_datei("rock_2schlitze_v34.sm2d");
    let ordner = std::env::temp_dir().join(format!("klon3d-umriss-reihe-{}", std::process::id()));
    std::fs::create_dir_all(&ordner).unwrap();
    let svg_pfad = ordner.join("rock.svg");
    lauf(&muster, &["--svg", svg_pfad.to_str().unwrap()]);
    let svg = std::fs::read_to_string(&svg_pfad).unwrap();

    let grau = svg.find("stroke=\"#808080\"").expect("graue Gruppe");
    let schwarz = svg.find("stroke=\"black\"").expect("schwarze Gruppe");
    assert!(grau < schwarz, "die Schnittlinie steht vor der Naht");
    assert_eq!(svg.matches("stroke=\"#808080\"").count(), 1);
    assert_eq!(svg.matches("stroke-width=\"0.2\"").count(), 1);
    // The seam group and its comment are unchanged.
    assert!(svg.contains("<!-- Nahtlinien, Einheit mm, y nach unten wie in der .sm2d -->"));
    assert!(svg.contains("stroke-width=\"0.3\""));
    std::fs::remove_dir_all(&ordner).ok();
}

#[test]
fn teile_ohne_zugabe_bekommen_keine_zweite_linie() {
    // Bolero v37: 16 pieces, 8 of them carry an allowance. The stepped stand
    // collar (`G1` runs its lower edge without allowance), the four washers,
    // the two donuts and the frog plate are glued flat and keep one line.
    //
    // No test pattern takes the `umriss_aus_knoten` fallback any more - the
    // bishop sleeve that used to (2142.7 mm on the drawn curves) resolves to
    // Seamly2D's 1418.58 mm and now carries a contour too.
    let muster = runde_datei("hexen_2026_10_27/bolero_v37.sm2d");
    let ausgabe = lauf(&muster, &[]);

    let zeilen: Vec<&str> = ausgabe.lines().filter(|z| z.contains("Knoten")).collect();
    assert_eq!(zeilen.len(), 16, "{ausgabe}");
    let mit = zeilen.iter().filter(|z| z.contains("mm Schnitt")).count();
    assert_eq!((mit, zeilen.len() - mit), (8, 8), "{ausgabe}");

    for ohne in [
        "Stehkragen gestuft 1x",
        "Donut Keyhole links",
        "Donut Keyhole rechts",
        "Scheibe Saum links",
        "Scheibe Saum rechts",
        "Scheibe Achsel links",
        "Scheibe Achsel rechts",
        "Frog-Plaettchen",
    ] {
        assert_eq!(gedruckter_schnitt(&ausgabe, ohne), None, "{ohne}");
    }

    // The bishop sleeve carries one: its seam line is Seamly2D's 1418.58 mm.
    assert_eq!(
        ausgabe
            .lines()
            .filter(|z| z.contains("1418.6 mm Naht"))
            .count(),
        2,
        "{ausgabe}"
    );
    let aermel = gedruckter_schnitt(&ausgabe, "Bischofaermel links 1x").expect("Ärmel hat Kontur");
    let soll = ringlaenge(&erwartete_kontur(&muster, "Bischofaermel links 1x").unwrap());
    assert!(
        (aermel - soll).abs() <= 0.05,
        "{aermel:.3} gegen {soll:.3} mm"
    );
}
