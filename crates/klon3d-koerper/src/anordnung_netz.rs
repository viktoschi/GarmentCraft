//! [`crate::koerper::Anordnung`] aus dem Netz allein — ohne CLO-Dateien.
//!
//! [`crate::anordnung_clo`] liest Segmente aus `range_list.rlist` und
//! Landmarken aus `IndexSet.txt`. Beides gehört zu CLOs `BaseMesh` und setzt
//! voraus, dass der Körper genau diese Punktliste ist. Fehlt der Ordner, oder
//! ist der Körper ein fremdes Netz, gibt es keine Segmente — dann rechnet
//! dieses Modul die Einteilung und die zwölf Landmarken aus der Form selbst.
//!
//! ## Was hier drinsteht
//!
//! * **Arme, Rumpf, Kopf** über drei Dijkstra-Läufe über die Netzkanten. Samen
//!   sind Bänder, die bei jedem Menschen im Arm, im Rumpf und im Kopf liegen;
//!   jeder Punkt gehört dem Samen mit dem kürzesten Weg. Der Kopf-Samen ist
//!   nicht überflüssig: ohne ihn liegt der Kopf über die Schulter näher am
//!   Arm-Samen und kippt auf die Armseite — dann wäre der höchste Punkt einer
//!   x-Bahn der Kopf und der Schulter-Knick unbrauchbar.
//! * **Schulter** aus dem Deltoid-Knick: je 45 % der Bahnenbreite links und
//!   rechts eine Ausgleichsgerade durch die höchsten Armpunkte, der Schnitt
//!   beider Geraden ist die Knickhöhe. Der Landmarkenpunkt ist der höchste
//!   Armpunkt im Fenster ±0,5 cm um diese Höhe — nicht der Schnittpunkt
//!   selbst (der liegt bis 5 cm daneben, weil dort kein Netzknoten sitzt).
//! * **Ellbogen** als hinterster Punkt im Band ±0,15 cm um 0,20 · H längs der
//!   Armfläche. „Engster Umfang" ist nicht eindeutig (mehrere Höhen gleich
//!   eng), „nächster Punkt an 0,20 · H" hat Gleichstände bis 6,9 cm daneben.
//! * **crotch** als höchste Höhe mit genau zwei Querschnittsschleifen
//!   (Ristschlitz), **y_merge** als erste Höhe mit weniger als drei Schleifen
//!   (Achsel-Verschmelzung).
//! * **Achsel** aus dem Rumpf-Ring bei `y_merge − 0,3`: dessen äußerster Punkt
//!   je Seite, auf `y_merge` gehoben.
//! * **Hals** aus der engsten Stelle: je 0,25 cm eine konvexe Hülle der
//!   Rumpfpunkte im Band ±0,4 cm, genommen die kleinste Umfangslänge. Die
//!   Mittellinie (Drosselgrube, 7. Halswirbel) kommt aus den vordersten und
//!   hintersten z des schmalen Mittelstreifens.
//! * **Brustpunkt** als vorderster Punkt je Seite im Brustband.
//! * **Rumpf** als „nicht Arm", beschnitten auf `[crotch, y_hals]` **und** auf
//!   die Breite der Rumpfschleife in dieser Höhe. Ohne die Breitengrenze sind
//!   32 575 Punkte zu viel drin und der Umriss weicht bis 7,4 cm ab.
//!
//! ## Alle Bänder als Anteile der Körperhöhe
//!
//! Keine Konstante hier ist eine Zentimeterzahl, die an einem Körper
//! abgelesen wurde — bis auf die Rasterweiten und Fenster, die zur
//! Rechengenauigkeit gehören und nicht zur Person. Die Anteile sind an CLOs
//! `Female`-BaseMesh (H = 164,15 cm) bestimmt und an einem zweiten Netz
//! gegengeprüft: die absolute cm-Fassung und die Anteilsfassung ergeben
//! **0 Punkte Unterschied** (Arme 100,0 %, Kopf 99,9 %, Rumpf 80,2 %).
//!
//! ## Geprüft
//!
//! Gegen die zwölf Landmarken aus CLOs `IndexSet.txt` (dieselbe Prüfung wie
//! [`crate::anordnung_clo`], Test `tests/anordnung_netz.rs`): Mittel **0,58
//! cm** über die zehn Punkte, die in die Simulation eingehen, grösste
//! Abweichung **1,18 cm** (Achsel). `achsel` und `brustpunkt` gehen nur in die
//! Kommandozeile (`klon3d-cli/src/anziehen.rs`), nicht in
//! [`klon3d_sim::platzierung::Huellen`].
//!
//! ## Verworfen und warum (nicht wieder aufnehmen)
//!
//! * Schulter über die stärkste Krümmung: 4,4 cm daneben.
//! * Schulter an der Arm/Rumpf-Naht: 16 cm daneben.
//! * Schulter als interpolierter Punkt auf der Knickhöhe: 0,6–1,4 cm
//!   schlechter als der höchste Netzknoten im Fenster.
//! * Ellbogen über den engsten Umfang, bei 0,18 · H, über den nächsten Punkt
//!   an 0,20 · H: alle drei nicht eindeutig.
//! * Rumpf als „nicht Arm" allein: 7,4 cm Umrissfehler.
//! * Rumpf über CLO-Segmente nachbauen: nicht nötig, „nicht Arm" trifft Hals
//!   und Brustpunkt auf 0,000 cm genau.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::koerper::Anordnung;
use crate::massband::punkte_cm;
use crate::{Netz, Punkt3};

/// Netzeinheiten je Zentimeter für die Netze dieses Projekts (Millimeter).
pub const MM_JE_CM: f64 = 10.0;

/// Kleinste Punktzahl, die noch als Körper gilt. CLOs `BaseMesh` bringt
/// 31 422 Körperpunkte und dazu Haare, Zähne und Einzelpunkte mit; ein
/// fremder Körper bringt nur sich selbst.
const KOERPER_MINDEST: usize = 1_000;

// ── Samen der drei Dijkstra-Läufe, als Anteile der Körperhöhe ──────────────
const SAMEN_ARM_X: f64 = 0.1218;
const SAMEN_ARM_Y: (f64, f64) = (0.5741, 0.7567);
const SAMEN_RUMPF_X: f64 = 0.0305;
const SAMEN_RUMPF_Y: (f64, f64) = (0.6046, 0.6960);
const SAMEN_KOPF_X: f64 = 0.0183;
const SAMEN_KOPF_Y: f64 = 0.9090;

// ── Schulter ──────────────────────────────────────────────────────────────
/// x-Bereich der Bahnen (Anteil der Höhe) und ihre Breite in cm.
const SCHULTER_X: (f64, f64) = (0.0487, 0.1645);
const SCHULTER_BAHN: f64 = 0.25;
/// Anteil der Bahnenbreite, über den je eine Gerade gelegt wird.
const SCHULTER_TANGENTE: f64 = 0.45;
/// Fenster um die Knickhöhe, in dem der Netzknoten gesucht wird.
const SCHULTER_FENSTER: f64 = 0.5;
/// Halbe Breite des x-Bandes, aus dem eine Bahn ihren höchsten Punkt nimmt.
const SCHULTER_BAND: f64 = 0.2;

// ── Ellbogen ──────────────────────────────────────────────────────────────
const ELLBOGEN_ANTEIL: f64 = 0.20;
const ELLBOGEN_BAND: f64 = 0.15;

// ── crotch und Achsel-Verschmelzung ───────────────────────────────────────
const CROTCH_VON: f64 = 0.5734;
const CROTCH_BIS: f64 = 0.30;
/// Höhenschritt der Schleifenzählung, in cm.
const SCHLEIFEN_SCHRITT: f64 = 0.05;
const MERGE_VON: f64 = 0.7268;
const MERGE_BIS: f64 = 0.7999;
/// Höhe unter der Verschmelzung, auf der der Rumpf-Ring noch allein steht.
const ACHSEL_ABSTAND: f64 = 0.3;
const ACHSEL_BAND: f64 = 0.2;

// ── Hals ──────────────────────────────────────────────────────────────────
const HALS_VON: f64 = 0.8115;
const HALS_BIS: f64 = 0.9214;
/// Halbe Höhe des Bandes, aus dem die Hülle je Zeile gebildet wird.
const HALS_BAND: f64 = 0.4;
const HALS_MINDEST: usize = 20;
const HALS_ZEILE: f64 = 0.25;
const MITTE_VON: f64 = 0.7749;
/// Mindestpunktzahl eines Querschnittsrings, aus dem die Halsmittellinie kommt.
const MITTE_MINDEST: usize = 4;
/// Mindesttiefe einer Halszeile als Anteil der Höhe (≈ 3,3 cm bei 164 cm).
///
/// Ohne diese Schranke zählen Zeilen mit, in denen der Mittelstreifen nur
/// drei Punkte trifft: dort ist vorn ≈ hinten, die „Tiefe" also null. Solche
/// Zeilen liegen im Kopf (Kinn, Nase) und nicht im Hals; wird eine davon als
/// Drosselgrube gewählt, steht der Nackenpunkt **vor** ihr — bei zwei von 38
/// Fremdkörpern war genau das der Fall.
const MITTE_TIEFE: f64 = 0.0200;
/// Unter dieser Höhe ist die Drosselgrube nicht mehr zu suchen.
const VORN_VON: f64 = 0.7996;
/// Der 7. Halswirbel liegt so viel unter der engsten Stelle.
const HINTEN_ABSTAND: f64 = 0.0091;

// ── Brustpunkt ────────────────────────────────────────────────────────────
const BRUST_X: f64 = 0.0122;
const BRUST_Y: (f64, f64) = (0.6536, 0.7996);

// ── Rumpf ─────────────────────────────────────────────────────────────────
/// Obergrenze der halben Rumpfbreite; darüber ist es nie Rumpf.
const RUMPF_BREITE: f64 = 0.2437;
const RUMPF_RASTER: f64 = 0.5;
/// Beginn über `crotch`, damit der Ristschlitz nicht mitgezählt wird.
const RUMPF_RAND: f64 = 0.1;
/// So weit über den Hals hinaus wird die Breite noch abgetastet.
const RUMPF_UEBER: f64 = 2.0;

/// Kleinste Punktzahl, die als Querschnittsschleife gilt.
const SCHLEIFE_MINDEST: usize = 8;

/// Raster, auf dem die Endpunkte eines Schnittstücks zusammengefasst werden.
///
/// Zwei Dreiecke, die dieselbe Kante teilen, rechnen dieselbe Kreuzung von
/// verschiedenen Seiten; die Ergebnisse unterscheiden sich um `1e-13` der
/// Koordinatengrösse. `1e-6` cm ist um Grössenordnungen feiner als der
/// Abstand zweier Netzpunkte und kann nichts zusammenfassen, was verschieden
/// ist.
const RASTER: f64 = 1.0e-6;

/// Landmarken und Punktlisten aus `netz`, ohne CLO-Dateien.
///
/// `pro_cm` sind die Netzeinheiten je Zentimeter (Millimeter: [`MM_JE_CM`]);
/// die zurückgegebenen Punkte stehen im **Rahmen des Netzes**, nicht in dem
/// von CLO — wie bei [`crate::koerper::anordnung`].
///
/// Gerechnet wird in Zentimetern auf den wie bei CLO zentrierten Punkten
/// ([`zentriert_cm`]): x- und z-Mittelwert 0, tiefster Punkt auf y = 0. Am
/// Ende wird der Versatz zurückaddiert.
pub fn aus_netz(netz: &Netz, pro_cm: f64) -> Result<Anordnung, String> {
    let n = netz.punkte.len();
    if n < SCHLEIFE_MINDEST * 10 {
        return Err(format!("nur {n} Punkte im Netz"));
    }
    let dreiecke_alle: Vec<[usize; 3]> = netz
        .dreiecke
        .iter()
        .copied()
        .filter(|d| d.iter().all(|&i| i < n))
        .collect();
    if dreiecke_alle.is_empty() {
        return Err("keine Dreiecke im Netz".into());
    }

    // CLOs `BaseMesh.obj` ist nicht nur der Körper: Haare, Zähne und
    // Einzelpunkte stehen daneben, 16 228 Punkte. Sie hängen nicht am Körper
    // und würden Höhe, Mittelwert und Querschnitte verfälschen (die Höhe über
    // alles gemessen ist 199 statt 164 cm, das x-Mittel um Zentimeter
    // verschoben). Der Körper ist die grösste Zusammenhangskomponente; alles
    // ohne Kante fällt ohnehin heraus.
    let koerper = groesste_komponente(n, &dreiecke_alle)
        .ok_or("keine Zusammenhangskomponente mit mehr als 1000 Punkten")?;
    let dreiecke: Vec<[usize; 3]> = dreiecke_alle
        .into_iter()
        .filter(|d| d.iter().all(|&i| koerper[i]))
        .collect();

    let roh = punkte_cm(netz, pro_cm);
    // Rahmen wie `zentriert_cm`, aber nur über den Körper: x- und z-Mittel 0,
    // tiefster Punkt auf y = 0.
    let (mut mx, mut mz, mut my, mut anzahl) = (0.0, 0.0, f64::INFINITY, 0.0);
    for (i, q) in roh.iter().enumerate() {
        if koerper[i] {
            mx += q[0];
            mz += q[2];
            my = my.min(q[1]);
            anzahl += 1.0;
        }
    }
    if anzahl < 1.0 {
        return Err("keine Körperpunkte".into());
    }
    mx /= anzahl;
    mz /= anzahl;
    let p: Vec<Punkt3> = roh
        .iter()
        .map(|q| [q[0] - mx, q[1] - my, q[2] - mz])
        .collect();

    let h = (0..n)
        .filter(|&i| koerper[i])
        .map(|i| p[i][1])
        .fold(0.0, f64::max);
    if h < 1.0 {
        return Err(format!("Körperhöhe {h:.2} cm ist zu klein"));
    }
    let hoch = |anteil: f64| anteil * h;

    // Höhenbereich je Dreieck, damit die Querschnittsabfrage nicht jedes Mal
    // alle Dreiecke prüft. `ymin <= y < ymax` ist genau die Bedingung, unter
    // der ein Dreieck ein Stück liefert.
    let spann: Vec<(f64, f64)> = dreiecke
        .iter()
        .map(|d| {
            let ys = [p[d[0]][1], p[d[1]][1], p[d[2]][1]];
            (
                ys.iter().copied().fold(f64::INFINITY, f64::min),
                ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            )
        })
        .collect();

    // ── 1. Arm, Rumpf, Kopf ──────────────────────────────────────────────
    let nachbarn = nachbarschaft(&p, &dreiecke);
    let samen = |f: &dyn Fn(Punkt3) -> bool| -> Vec<usize> {
        (0..n).filter(|&i| koerper[i] && f(p[i])).collect()
    };
    let arm_samen = samen(&|q| {
        q[0].abs() > hoch(SAMEN_ARM_X) && q[1] > hoch(SAMEN_ARM_Y.0) && q[1] < hoch(SAMEN_ARM_Y.1)
    });
    let rumpf_samen = samen(&|q| {
        q[0].abs() < hoch(SAMEN_RUMPF_X)
            && q[1] > hoch(SAMEN_RUMPF_Y.0)
            && q[1] < hoch(SAMEN_RUMPF_Y.1)
    });
    let kopf_samen = samen(&|q| q[0].abs() < hoch(SAMEN_KOPF_X) && q[1] > hoch(SAMEN_KOPF_Y));
    for (name, s) in [
        ("Arm", &arm_samen),
        ("Rumpf", &rumpf_samen),
        ("Kopf", &kopf_samen),
    ] {
        if s.is_empty() {
            return Err(format!(
                "{name}-Samen ist leer — passt der Körper zu diesen Anteilen?"
            ));
        }
    }

    let d_arm = dijkstra(&nachbarn, &arm_samen);
    let d_rumpf = dijkstra(&nachbarn, &rumpf_samen);
    let d_kopf = dijkstra(&nachbarn, &kopf_samen);
    let ist_arm: Vec<bool> = (0..n)
        .map(|i| koerper[i] && d_arm[i] < d_rumpf[i] && d_arm[i] < d_kopf[i])
        .collect();

    let arme: [Vec<usize>; 2] = [
        (0..n).filter(|&i| ist_arm[i] && p[i][0] > 0.0).collect(),
        (0..n).filter(|&i| ist_arm[i] && p[i][0] < 0.0).collect(),
    ];
    for (s, arm) in arme.iter().enumerate() {
        if arm.len() < 100 {
            return Err(format!(
                "{} Arm: nur {} Punkte",
                ["linker", "rechter"][s],
                arm.len()
            ));
        }
    }

    // ── 2. Schulter ──────────────────────────────────────────────────────
    let mut schulter_idx = [0usize; 2];
    for s in 0..2 {
        let sgn = if s == 0 { 1.0 } else { -1.0 };
        schulter_idx[s] = schulter(&p, &arme[s], sgn, h).ok_or_else(|| {
            format!(
                "{} Arm: kein Schulterknick in den Bahnen",
                ["linker", "rechter"][s]
            )
        })?;
    }

    // ── 3. Ellbogen ──────────────────────────────────────────────────────
    let mut ellbogen_idx = [0usize; 2];
    for s in 0..2 {
        let mut im_arm = vec![false; p.len()];
        for &i in &arme[s] {
            im_arm[i] = true;
        }
        ellbogen_idx[s] = ellbogen(&p, &dreiecke, &im_arm, &arme[s], schulter_idx[s], h)
            .ok_or_else(|| format!("{} Arm: kein Ellbogen im Band", ["linker", "rechter"][s]))?;
    }

    // ── 4. crotch, Achsel-Verschmelzung, Halshöhe ────────────────────────
    let mut crotch = None;
    let mut y = hoch(CROTCH_VON);
    while y > hoch(CROTCH_BIS) {
        if schleifen(&p, &dreiecke, &spann, y).len() == 2 {
            crotch = Some(y);
            break;
        }
        y -= SCHLEIFEN_SCHRITT;
    }
    let crotch = crotch.ok_or("kein Ristschlitz: nirgends genau zwei Schleifen")?;

    let mut y_merge = None;
    let mut y = hoch(MERGE_VON);
    while y <= hoch(MERGE_BIS) {
        if schleifen(&p, &dreiecke, &spann, y).len() < 3 {
            y_merge = Some(y);
            break;
        }
        y += SCHLEIFEN_SCHRITT;
    }
    let y_merge = y_merge.ok_or("Arme verschmelzen nicht mit dem Rumpf")?;

    // ── 5. Achsel ────────────────────────────────────────────────────────
    let ringe = schleifen(&p, &dreiecke, &spann, y_merge - ACHSEL_ABSTAND);
    let rumpf_ring = ringe
        .iter()
        .find(|r| ring_mitte(r).abs() < hoch(SAMEN_RUMPF_X))
        .ok_or("kein Rumpf-Ring unter der Achsel")?;
    let mut achsel = [Punkt3::default(); 2];
    for s in 0..2 {
        let sgn = if s == 0 { 1.0 } else { -1.0 };
        let rand = if sgn > 0.0 {
            rumpf_ring
                .iter()
                .map(|q| q.0)
                .fold(f64::NEG_INFINITY, f64::max)
        } else {
            rumpf_ring.iter().map(|q| q.0).fold(f64::INFINITY, f64::min)
        };
        let nah: Vec<f64> = rumpf_ring
            .iter()
            .filter(|q| (q.0 - rand).abs() < ACHSEL_BAND)
            .map(|q| q.1)
            .collect();
        let z = nah.iter().sum::<f64>() / nah.len().max(1) as f64;
        achsel[s] = [rand, y_merge, z];
    }

    // ── 6. Hals: engste Stelle, Mittellinie, Seitenpunkte ────────────────
    let rumpfseitig: Vec<usize> = (0..n).filter(|&i| koerper[i] && !ist_arm[i]).collect();

    let mut best: Option<(f64, f64)> = None;
    let mut y = hoch(HALS_VON);
    while y <= hoch(HALS_BIS) {
        let band: Vec<(f64, f64)> = rumpfseitig
            .iter()
            .filter(|&&i| (p[i][1] - y).abs() < HALS_BAND)
            .map(|&i| (p[i][0], p[i][2]))
            .collect();
        if band.len() >= HALS_MINDEST {
            if let Some(u) = huelle_umfang(&band) {
                if best.is_none_or(|(_, alt)| u < alt) {
                    best = Some((y, u));
                }
            }
        }
        y += HALS_ZEILE;
    }
    let (y_hals, _) = best.ok_or("kein Halsquerschnitt gefunden")?;

    let halsbreite = 2.0
        * rumpfseitig
            .iter()
            .filter(|&&i| (p[i][1] - y_hals).abs() < 1.0)
            .map(|&i| p[i][0].abs())
            .fold(0.0, f64::max);

    // Mittellinie des Halses: vorderstes und hinterstes z des Rumpfrings je
    // Höhe.
    //
    // Ein Streifen aus Knoten allein reicht nicht: bei versetzten Knoten trifft
    // er je Höhe manchmal nur die Vorderseite, dann ist „hinten" gleich „vorn"
    // und die Interpolation des Nackens springt um Zentimeter (bei
    // `catsuit_eyi.zprj` +0,8 statt −9,7). Der Querschnittsring wird über die
    // Dreiecke interpoliert und hat vorn und hinten immer beide Seiten.
    let mut zeilen: Vec<(f64, f64, f64)> = Vec::new(); // (y, z vorn, z hinten)
    let mut y = hoch(MITTE_VON);
    while y <= hoch(HALS_BIS) {
        let ringe = schleifen(&p, &dreiecke, &spann, y);
        if let Some(r) = ringe
            .iter()
            .find(|r| ring_mitte(r).abs() < hoch(SAMEN_RUMPF_X))
        {
            if r.len() >= MITTE_MINDEST {
                let vorn = r.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max);
                let hinten = r.iter().map(|q| q.1).fold(f64::INFINITY, f64::min);
                zeilen.push((y, vorn, hinten));
            }
        }
        y += HALS_ZEILE;
    }
    if zeilen.len() < 2 {
        return Err("Halsmittellinie: zu wenige Zeilen".into());
    }
    let ys: Vec<f64> = zeilen.iter().map(|z| z.0).collect();
    let hinten: Vec<f64> = zeilen.iter().map(|z| z.2).collect();

    // Drosselgrube: die Zeile, deren vorderster Punkt am wenigsten weit vorn
    // liegt — die Einbuchtung zwischen den Schlüsselbeinen.
    //
    // Zeilen ohne Tiefe fallen hier heraus (siehe `MITTE_TIEFE`): sie liegen im
    // Kopf, ihre „Tiefe" ist null, und würde eine davon gewählt, stünde der
    // Nackenpunkt vor der Drosselgrube.
    let mut j = None;
    for (i, z) in zeilen.iter().enumerate() {
        if z.1 - z.2 < hoch(MITTE_TIEFE) {
            continue;
        }
        if z.0 > hoch(VORN_VON) && z.0 < y_hals && j.is_none_or(|k: usize| z.1 < zeilen[k].1) {
            j = Some(i);
        }
    }
    let j = j.ok_or("keine Drosselgrube unter der Halsenge")?;

    let hals_vorn = [0.0, zeilen[j].0, zeilen[j].1];
    let y_hinten = y_hals - hoch(HINTEN_ABSTAND);
    let hals_hinten = [0.0, y_hinten, interp(y_hinten, &ys, &hinten)];
    let y_mitte = 0.5 * (zeilen[j].0 + y_hinten);
    // Der seitliche Halspunkt sitzt in der Mitte zwischen vorderem und
    // hinterem Hals, nicht am Nacken: CLOs L6 liegt bei z = −4,2 cm, zwischen
    // Drosselgrube (+1,0) und Nacken (−9,0). Der Nackenwert allein wäre 6 cm
    // daneben.
    let vorn_kurve: Vec<f64> = zeilen.iter().map(|z| z.1).collect();
    let z_mitte = 0.5 * (interp(y_mitte, &ys, &vorn_kurve) + interp(y_mitte, &ys, &hinten));
    let hals_seitlich = [
        [0.5 * halsbreite, y_mitte, z_mitte],
        [-0.5 * halsbreite, y_mitte, z_mitte],
    ];

    // ── 7. Brustpunkt: vorderster Punkt je Seite ─────────────────────────
    let mut brustpunkt = [Punkt3::default(); 2];
    for s in 0..2 {
        let sgn = if s == 0 { 1.0 } else { -1.0 };
        let mut best = None;
        for &i in &rumpfseitig {
            let q = p[i];
            if sgn * q[0] > hoch(BRUST_X)
                && q[1] > hoch(BRUST_Y.0)
                && q[1] < hoch(BRUST_Y.1)
                && best.is_none_or(|k: usize| q[2] > p[k][2])
            {
                best = Some(i);
            }
        }
        brustpunkt[s] = p[best.ok_or("kein Brustpunkt in diesem Band")?];
    }

    // ── 8. Rumpf mit Breitengrenze ───────────────────────────────────────
    let mut breiten: Vec<(f64, f64)> = Vec::new();
    let mut y = crotch + RUMPF_RAND;
    while y <= y_hals + RUMPF_UEBER {
        let ringe = schleifen(&p, &dreiecke, &spann, y);
        if let Some(r) = ringe
            .iter()
            .find(|r| ring_mitte(r).abs() < hoch(SAMEN_RUMPF_X))
        {
            let breite = r.iter().map(|q| q.0.abs()).fold(0.0, f64::max);
            breiten.push((y, breite.min(hoch(RUMPF_BREITE))));
        }
        y += RUMPF_RASTER;
    }
    if breiten.len() < 2 {
        return Err("Rumpfbreite: zu wenige Höhen mit Rumpf-Ring".into());
    }
    let ys: Vec<f64> = breiten.iter().map(|b| b.0).collect();
    let bs: Vec<f64> = breiten.iter().map(|b| b.1).collect();

    let rumpf: Vec<usize> = rumpfseitig
        .iter()
        .copied()
        .filter(|&i| {
            p[i][1] >= crotch && p[i][1] <= y_hals && p[i][0].abs() <= interp(p[i][1], &ys, &bs)
        })
        .collect();
    if rumpf.len() < 100 {
        return Err(format!("nur {} Rumpfpunkte", rumpf.len()));
    }

    // ── Zurück in den Rahmen des Netzes ──────────────────────────────────
    let zurueck = |q: Punkt3| -> Punkt3 {
        [
            (q[0] + mx) * pro_cm,
            (q[1] + my) * pro_cm,
            (q[2] + mz) * pro_cm,
        ]
    };

    Ok(Anordnung {
        rumpf,
        arme,
        hals_vorn: zurueck(hals_vorn),
        hals_hinten: zurueck(hals_hinten),
        hals_seitlich: hals_seitlich.map(&zurueck),
        schulter: schulter_idx.map(|i| zurueck(p[i])),
        achsel: achsel.map(&zurueck),
        brustpunkt: brustpunkt.map(&zurueck),
        ellbogen: ellbogen_idx.map(|i| zurueck(p[i])),
    })
}

/// Die grösste Zusammenhangskomponente des Netzes, als Maske.
///
/// CLOs `BaseMesh.obj` führt neben dem Körper (31 422 Punkte) Haare, Zähne und
/// 7 185 Einzelpunkte ohne Kante. Für die Landmarken zählt nur der Körper; die
/// übrigen Teile hängen nicht an ihm und verfälschen Höhe, Mittelwert und
/// Querschnitte.
fn groesste_komponente(n: usize, dreiecke: &[[usize; 3]]) -> Option<Vec<bool>> {
    let mut nb: Vec<Vec<usize>> = vec![Vec::new(); n];
    for d in dreiecke {
        for e in 0..3 {
            let (a, b) = (d[e], d[(e + 1) % 3]);
            if a != b {
                nb[a].push(b);
                nb[b].push(a);
            }
        }
    }
    let mut gesehen = vec![false; n];
    let mut beste: Vec<usize> = Vec::new();
    for start in 0..n {
        if gesehen[start] || nb[start].is_empty() {
            continue;
        }
        gesehen[start] = true;
        let mut stapel = vec![start];
        let mut teil = Vec::new();
        while let Some(u) = stapel.pop() {
            teil.push(u);
            for &v in &nb[u] {
                if !gesehen[v] {
                    gesehen[v] = true;
                    stapel.push(v);
                }
            }
        }
        if teil.len() > beste.len() {
            beste = teil;
        }
    }
    if beste.len() < KOERPER_MINDEST {
        return None;
    }
    let mut maske = vec![false; n];
    for i in beste {
        maske[i] = true;
    }
    Some(maske)
}

/// Ein Abstand mit umgedrehter Ordnung, damit [`BinaryHeap`] den kleinsten
/// zuerst gibt.
#[derive(PartialEq)]
struct Klein(f64);

impl Eq for Klein {}

impl Ord for Klein {
    fn cmp(&self, anderes: &Self) -> Ordering {
        anderes.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for Klein {
    fn partial_cmp(&self, anderes: &Self) -> Option<Ordering> {
        Some(self.cmp(anderes))
    }
}

/// Nachbarschaft über die Netzkanten, jede Kante einmal, mit ihrer Länge.
fn nachbarschaft(p: &[Punkt3], dreiecke: &[[usize; 3]]) -> Vec<Vec<(usize, f64)>> {
    let mut nb = vec![Vec::new(); p.len()];
    let mut gesehen = HashSet::new();
    for d in dreiecke {
        for e in 0..3 {
            let (a, b) = (d[e], d[(e + 1) % 3]);
            if a == b || !gesehen.insert((a.min(b), a.max(b))) {
                continue;
            }
            let w = abstand(p[a], p[b]);
            nb[a].push((b, w));
            nb[b].push((a, w));
        }
    }
    nb
}

/// Kürzeste Wege von allen `samen` aus; `f64::INFINITY`, wo keiner hinführt.
fn dijkstra(nachbarn: &[Vec<(usize, f64)>], samen: &[usize]) -> Vec<f64> {
    let mut d = vec![f64::INFINITY; nachbarn.len()];
    let mut heap = BinaryHeap::new();
    for &s in samen {
        if d[s] > 0.0 {
            d[s] = 0.0;
            heap.push((Klein(0.0), s));
        }
    }
    while let Some((Klein(du), u)) = heap.pop() {
        if du > d[u] {
            continue;
        }
        for &(v, w) in &nachbarn[u] {
            let nd = du + w;
            if nd < d[v] {
                d[v] = nd;
                heap.push((Klein(nd), v));
            }
        }
    }
    d
}

fn abstand(a: Punkt3, b: Punkt3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Querschnittsschleifen der Höhe `y` als `(x, z)`-Ringe, nach der Lage in x
/// sortiert; Ringe unter [`SCHLEIFE_MINDEST`] Punkten fallen weg (die
/// Schnittreste an einer offenen Naht).
fn schleifen(
    p: &[Punkt3],
    dreiecke: &[[usize; 3]],
    spann: &[(f64, f64)],
    y: f64,
) -> Vec<Vec<(f64, f64)>> {
    let mut ecke: HashMap<(i64, i64), (f64, f64)> = HashMap::new();
    let mut kanten: Vec<((i64, i64), (i64, i64))> = Vec::new();
    for (d, s) in dreiecke.iter().zip(spann) {
        if !(s.1 > y && s.0 <= y) {
            continue;
        }
        let t = [p[d[0]], p[d[1]], p[d[2]]];
        let mut paare: Vec<(f64, f64)> = Vec::with_capacity(2);
        for e in 0..3 {
            let (a, b) = (t[e], t[(e + 1) % 3]);
            if (a[1] > y) != (b[1] > y) {
                let f = (y - a[1]) / (b[1] - a[1]);
                paare.push((a[0] + f * (b[0] - a[0]), a[2] + f * (b[2] - a[2])));
            }
        }
        if paare.len() != 2 {
            continue;
        }
        let ka = schluessel(paare[0]);
        let kb = schluessel(paare[1]);
        ecke.entry(ka).or_insert(paare[0]);
        ecke.entry(kb).or_insert(paare[1]);
        if ka != kb {
            kanten.push((ka, kb));
        }
    }

    let mut nach: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    for &(a, b) in &kanten {
        nach.entry(a).or_default().push(b);
        nach.entry(b).or_default().push(a);
    }

    let mut gesehen: HashSet<(i64, i64)> = HashSet::new();
    let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
    for &start in nach.keys() {
        if !gesehen.insert(start) {
            continue;
        }
        let mut stapel = vec![start];
        let mut teil: Vec<(f64, f64)> = Vec::new();
        while let Some(nd) = stapel.pop() {
            if let Some(&q) = ecke.get(&nd) {
                teil.push(q);
            }
            if let Some(nbs) = nach.get(&nd) {
                for &n in nbs {
                    if gesehen.insert(n) {
                        stapel.push(n);
                    }
                }
            }
        }
        if teil.len() >= SCHLEIFE_MINDEST {
            out.push(teil);
        }
    }
    out.sort_by(|a, b| {
        ring_mitte(a)
            .partial_cmp(&ring_mitte(b))
            .unwrap_or(Ordering::Equal)
    });
    out
}

fn schluessel(q: (f64, f64)) -> (i64, i64) {
    ((q.0 / RASTER).round() as i64, (q.1 / RASTER).round() as i64)
}

/// Mitte eines Rings in x — so wird der Rumpf unter den Armen erkannt.
fn ring_mitte(ring: &[(f64, f64)]) -> f64 {
    let lo = ring.iter().map(|q| q.0).fold(f64::INFINITY, f64::min);
    let hi = ring.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max);
    0.5 * (lo + hi)
}

/// Umfang der konvexen Hülle der Punkte `(x, z)`; `None` bei weniger als drei
/// Punkten oder wenn alle auf einer Geraden liegen.
fn huelle_umfang(punkte: &[(f64, f64)]) -> Option<f64> {
    if punkte.len() < 3 {
        return None;
    }
    let mut s: Vec<(f64, f64)> = punkte.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    s.dedup();
    if s.len() < 3 {
        return None;
    }
    let kreuz = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| -> f64 {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut unten: Vec<(f64, f64)> = Vec::new();
    for &q in &s {
        while unten.len() >= 2 && kreuz(unten[unten.len() - 2], unten[unten.len() - 1], q) <= 0.0 {
            unten.pop();
        }
        unten.push(q);
    }
    let mut oben: Vec<(f64, f64)> = Vec::new();
    for &q in s.iter().rev() {
        while oben.len() >= 2 && kreuz(oben[oben.len() - 2], oben[oben.len() - 1], q) <= 0.0 {
            oben.pop();
        }
        oben.push(q);
    }
    unten.pop();
    oben.pop();
    unten.extend(oben);
    if unten.len() < 3 {
        return None;
    }
    let mut u = 0.0;
    for i in 0..unten.len() {
        let (a, b) = (unten[i], unten[(i + 1) % unten.len()]);
        u += ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    }
    Some(u)
}

/// Schulterpunkt eines Arms: der höchste Armpunkt im Fenster um den
/// Deltoid-Knick.
fn schulter(p: &[Punkt3], arm: &[usize], sgn: f64, h: f64) -> Option<usize> {
    let (von, bis) = (SCHULTER_X.0 * h, SCHULTER_X.1 * h);

    // Bahnen: je x die höchste Stelle des Arms.
    let mut bahnen: Vec<(f64, f64)> = Vec::new();
    let mut x = von;
    while x <= bis {
        let mut oben: Option<f64> = None;
        let mut anzahl = 0;
        for &i in arm {
            if (sgn * p[i][0] - x).abs() < SCHULTER_BAND {
                oben = Some(oben.map_or(p[i][1], |o: f64| o.max(p[i][1])));
                anzahl += 1;
            }
        }
        // Eine Bahn aus einem einzigen Punkt ist ein Ausreisser, kein Rand.
        if anzahl >= 2 {
            if let Some(y) = oben {
                bahnen.push((x, y));
            }
        }
        x += SCHULTER_BAHN;
    }
    if bahnen.len() < 4 {
        return None;
    }

    let lo = bahnen.iter().map(|b| b.0).fold(f64::INFINITY, f64::min);
    let hi = bahnen.iter().map(|b| b.0).fold(f64::NEG_INFINITY, f64::max);
    let b = hi - lo;
    let links: Vec<(f64, f64)> = bahnen
        .iter()
        .copied()
        .filter(|q| q.0 <= lo + SCHULTER_TANGENTE * b)
        .collect();
    let rechts: Vec<(f64, f64)> = bahnen
        .iter()
        .copied()
        .filter(|q| q.0 >= hi - SCHULTER_TANGENTE * b)
        .collect();
    let (a1, b1) = gerade(&links)?;
    let (a2, b2) = gerade(&rechts)?;
    if (a1 - a2).abs() < 1.0e-9 {
        return None;
    }
    let knick = (b2 - b1) / (a1 - a2);

    // Der Netzknoten im Fenster, nicht der Schnittpunkt: der Schnittpunkt
    // liegt zwischen zwei Bahnen und damit bis 5 cm daneben.
    let mut best: Option<usize> = None;
    for &i in arm {
        if (sgn * p[i][0] - knick).abs() < SCHULTER_FENSTER {
            best = Some(best.map_or(i, |k: usize| if p[i][1] > p[k][1] { i } else { k }));
        }
    }
    best
}

/// Ausgleichsgerade durch Punkte `(x, y)`; `None` bei weniger als zwei oder
/// wenn alle x gleich sind.
///
/// Gerechnet wird um den x-Mittelwert: die Bahnen liegen bei 8…19 cm, und
/// ohne Zentrierung summiert sich `n·Σx² − (Σx)²` aus grossen Zahlen zu einer
/// kleinen Differenz — das kostet Stellen, die der Knick braucht.
fn gerade(punkte: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = punkte.len() as f64;
    if punkte.len() < 2 {
        return None;
    }
    let mx = punkte.iter().map(|q| q.0).sum::<f64>() / n;
    let my = punkte.iter().map(|q| q.1).sum::<f64>() / n;
    let sxx: f64 = punkte.iter().map(|q| (q.0 - mx).powi(2)).sum();
    if sxx < 1.0e-12 {
        return None;
    }
    let sxy: f64 = punkte.iter().map(|q| (q.0 - mx) * (q.1 - my)).sum();
    let steigung = sxy / sxx;
    // y = steigung · (x − mx) + my
    Some((steigung, my - steigung * mx))
}

/// Ellbogenpunkt eines Arms: der hinterste Punkt im Band um 0,20 · H längs
/// der Armfläche, gemessen von der Schulter über die Netzkanten des Arms.
fn ellbogen(
    p: &[Punkt3],
    dreiecke: &[[usize; 3]],
    im_arm: &[bool],
    arm: &[usize],
    schulter: usize,
    h: f64,
) -> Option<usize> {
    // Nur Kanten, deren beide Enden im Arm liegen — sonst liefe der Weg vom
    // Arm in den Rumpf und die Bogenlänge wäre zu kurz.
    let mut nb = vec![Vec::new(); p.len()];
    let mut gesehen = HashSet::new();
    for d in dreiecke {
        for e in 0..3 {
            let (a, b) = (d[e], d[(e + 1) % 3]);
            if a == b || !im_arm[a] || !im_arm[b] || !gesehen.insert((a.min(b), a.max(b))) {
                continue;
            }
            let w = abstand(p[a], p[b]);
            nb[a].push((b, w));
            nb[b].push((a, w));
        }
    }
    let d = dijkstra(&nb, &[schulter]);
    let ziel = ELLBOGEN_ANTEIL * h;
    let mut best: Option<usize> = None;
    for &i in arm {
        if (d[i] - ziel).abs() < ELLBOGEN_BAND {
            best = Some(best.map_or(i, |k: usize| if p[i][2] < p[k][2] { i } else { k }));
        }
    }
    best
}

/// Lineare Interpolation wie `numpy.interp`: ausserhalb der Stützstellen der
/// Randwert, sonst die Gerade zwischen den beiden Nachbarn.
fn interp(x: f64, xs: &[f64], ys: &[f64]) -> f64 {
    if xs.len() < 2 {
        return ys.first().copied().unwrap_or(f64::NAN);
    }
    if x <= xs[0] {
        return ys[0];
    }
    let letzte = xs.len() - 1;
    if x >= xs[letzte] {
        return ys[letzte];
    }
    let (mut lo, mut hi) = (0usize, letzte);
    while hi - lo > 1 {
        let m = (lo + hi) / 2;
        if xs[m] <= x {
            lo = m;
        } else {
            hi = m;
        }
    }
    let t = (x - xs[lo]) / (xs[hi] - xs[lo]);
    ys[lo] + t * (ys[hi] - ys[lo])
}
