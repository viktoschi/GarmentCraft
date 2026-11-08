//! One report line for the grainline direction per triangle
//! ([`klon3d_sim::Szene::faser`]), shared by `anziehen` and `bewegen`.
//!
//! The direction is computed and carried, but nothing reads it yet: the material
//! is isotropic, so it changes no number. What the line has to say is therefore
//! its **provenance** - how many pieces brought a readable grainline, which ones
//! fell back to the substitute axis, and how many triangles had no plane left.
//! A substitute that stays silent would read like a measurement.

use klon3d_mesh::Teilnetz;
use klon3d_sim::Szene;

/// The line, without trailing newline.
pub fn zeile(teile: &[Teilnetz], s: &Szene) -> String {
    let ohne: Vec<&str> = teile
        .iter()
        .filter(|t| t.fadenlauf.and_then(|f| f.richtung()).is_none())
        .map(|t| t.teil.rsplit(" - ").next().unwrap_or(&t.teil))
        .collect();
    let mut z = format!(
        "  Fadenlauf    {} von {} Teilen aus dem Muster, {} Dreiecke mit Richtung",
        teile.len() - ohne.len(),
        teile.len(),
        s.faser.len()
    );
    if !ohne.is_empty() {
        // A whole garment rarely carries grainlines: name three and count the
        // rest, or the line would be longer than the table below it.
        let nennen = ohne.iter().take(3).copied().collect::<Vec<_>>().join(", ");
        let rest = ohne.len().saturating_sub(3);
        z.push_str(&format!(
            "; Ersatzachse fuer {nennen}{}",
            if rest > 0 { format!(" und {rest} weitere") } else { String::new() }
        ));
    }
    if s.faser_entartet > 0 {
        z.push_str(&format!("; {} Dreiecke ohne Flaeche (laengste Kante)", s.faser_entartet));
    }
    z
}

#[cfg(test)]
mod tests {
    use super::*;
    use klon3d_mesh::{Fadenlauf, Teilnetz};
    use klon3d_sim::szene::Teilvorgabe;
    use klon3d_sim::{Simparameter, RADICAL_RUBBER_04};

    /// A flat scene from a rectangle whose one piece carries `fadenlauf`.
    fn szene(fadenlauf: Option<Fadenlauf>) -> Szene {
        let mut n = klon3d_sim::roehre::rohr_netz(0.2, 0.2, 10.0).expect("Rechteck vernetzbar");
        n.teile[0].fadenlauf = fadenlauf;
        let vg = Teilvorgabe { material: RADICAL_RUBBER_04, reduktion: 0.0 };
        let lage = |_: usize, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, 0.0];
        Szene::aus_netz(&n, &|_| vg, &lage, Vec::new(), Simparameter::default())
    }

    /// A piece with only a name and a grainline; this test is about the line,
    /// not about a mesh.
    fn teil(name: &str, fadenlauf: Option<Fadenlauf>) -> Teilnetz {
        Teilnetz {
            teil: name.into(),
            punkte: Vec::new(),
            dreiecke: Vec::new(),
            nrand: 0,
            randort: Vec::new(),
            punktknoten: Vec::new(),
            kanten: Vec::new(),
            kontur: Vec::new(),
            schnitt: Vec::new(),
            fadenlauf,
            aufgegeben: 0,
        }
    }

    /// The line names the provenance: a grainline from the pattern, no
    /// substitute, one direction per triangle.
    #[test]
    fn mit_fadenlauf_keine_ersatzachse() {
        let f = Fadenlauf { p1: [0.0, 0.0], p2: [0.0, 100.0], pfeile: 0 };
        let s = szene(Some(f));
        let z = zeile(&[teil("Prüf", Some(f))], &s);
        assert!(z.starts_with("  Fadenlauf    1 von 1 Teilen aus dem Muster"), "{z}");
        assert!(z.contains(&format!("{} Dreiecke mit Richtung", s.faser.len())), "{z}");
        assert!(!z.contains("Ersatzachse"), "{z}");
        assert!(!z.contains("ohne Flaeche"), "{z}");
    }

    /// A piece without a readable grainline is named, never silently replaced.
    #[test]
    fn ohne_fadenlauf_nennt_die_ersatzachse() {
        let s = szene(None);
        let z = zeile(&[teil("Ruecken - Bluse", None)], &s);
        assert!(z.contains("0 von 1 Teilen aus dem Muster"), "{z}");
        // Only the piece name after the last " - ", as the other report lines do.
        assert!(z.contains("Ersatzachse fuer Bluse"), "{z}");

        // A whole garment names three and counts the rest, so the line stays
        // one line: five pieces give three names and "und 2 weitere".
        let fuenf: Vec<_> = ["A - Vorne", "B - Hinten", "C - Seite", "D - Aermel", "E - Kragen"]
            .iter()
            .map(|n| teil(n, None))
            .collect();
        let z = zeile(&fuenf, &s);
        assert!(z.contains("Ersatzachse fuer Vorne, Hinten, Seite und 2 weitere"), "{z}");
    }

    /// A grainline that collapses to a point is no grainline.
    #[test]
    fn fadenlauf_ohne_laenge_gilt_als_ersatz() {
        let s = szene(None);
        let punkt = Fadenlauf { p1: [7.0, 7.0], p2: [7.0, 7.0], pfeile: 0 };
        let z = zeile(&[teil("Vorne", Some(punkt))], &s);
        assert!(z.contains("0 von 1 Teilen aus dem Muster"), "{z}");
        assert!(z.contains("Ersatzachse fuer Vorne"), "{z}");
    }
}
