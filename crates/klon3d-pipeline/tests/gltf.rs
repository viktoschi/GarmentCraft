//! The motion as one glTF: container, geometry, animation — read back out of the
//! written bytes.
//!
//! A flat piece on a simple box body, two motion frames, no CLO data. What is
//! checked here:
//!
//! 1. **Container.** Magic `glTF`, version 2, the declared total length equals
//!    the file length, both chunks carry their declared length and type, the
//!    JSON chunk is padded with spaces and the binary one with zeros.
//! 2. **The geometry is the geometry of the run.** The base positions of frame 0
//!    and the morph delta of every frame reproduce the scene's `x`, after the
//!    y-up conversion, to 1e-6 m — for the garment and for the posed body.
//! 3. **The animation is the motion.** One weight channel per animated node,
//!    `STEP`, keyframe times `bild · dt` from 0, and as many morph targets as
//!    motion frames.
//! 4. **Names survive.** Every piece is a named mesh and a named node, and the
//!    body is a node of its own.
//! 5. **Reproducible.** The same motion gives byte-identical files.

use klon3d_pipeline::gltf::Bewegung;
use klon3d_sim::netzkoerper::{Koerpernetz, SUCHTIEFE};
use klon3d_sim::roehre::rohr_netz;
use klon3d_sim::szene::Teilvorgabe;
use klon3d_sim::{Koerper, Simparameter, Szene, RADICAL_RUBBER_04};

use serde_json::Value;

/// The piece of the test pattern; `rohr_netz` builds exactly one.
const TEIL: &str = "Rohr";

/// A closed box as the body: side 0.2 m, from z = 0 to z = 0.2, outward faces.
fn kasten() -> Koerpernetz {
    let a = 0.1;
    let h = 0.2;
    let punkte = vec![
        [-a, -a, 0.0],
        [a, -a, 0.0],
        [a, a, 0.0],
        [-a, a, 0.0],
        [-a, -a, h],
        [a, -a, h],
        [a, a, h],
        [-a, a, h],
    ];
    let dreiecke = vec![
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    Koerpernetz::neu(punkte, dreiecke, SUCHTIEFE).expect("Kasten als Körpernetz")
}

/// The flat piece around the box, both from the same builder as the real path.
fn szene() -> Szene {
    let netz = rohr_netz(0.2, 0.1, 10.0).expect("Rechteck vernetzbar");
    let vg = Teilvorgabe {
        material: RADICAL_RUBBER_04,
        reduktion: 0.0,
    };
    let lage = |_: usize, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, 0.05];
    Szene::aus_netz(
        &netz,
        &|_| vg,
        &lage,
        vec![Koerper::netz(kasten())],
        Simparameter::default(),
    )
}

/// A frame that is visibly not frame 0: stretched in x and lifted in z.
fn bewegt(s: &Szene, faktor: f64, hoch: f64) -> Szene {
    let mut k = s.clone();
    for p in k.x.iter_mut() {
        p[0] *= faktor;
        p[2] += hoch;
    }
    let mut netz = kasten();
    for p in netz.punkte.iter_mut() {
        p[2] += hoch * 0.5;
    }
    k.koerper = vec![Koerper::netz(netz)];
    k
}

/// `(x, y, z) ↦ (x, z, −y)` — the conversion the file does.
fn y_up(q: [f64; 3]) -> [f64; 3] {
    [q[0], q[2], -q[1]]
}

/// The two chunks of a GLB, after the header was checked.
fn zerlegen(bytes: &[u8]) -> (Value, Vec<u8>) {
    assert!(bytes.len() >= 20, "die Datei ist zu kurz für einen GLB-Kopf");
    assert_eq!(&bytes[0..4], b"glTF", "die Kennung fehlt");
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    assert_eq!(version, 2, "die Version ist {version}, erwartet 2");
    let gesamt = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    assert_eq!(gesamt, bytes.len(), "die Gesamtlänge passt nicht zur Datei");

    let mut at = 12;
    let mut json = None;
    let mut bin = None;
    while at < bytes.len() {
        assert!(at + 8 <= bytes.len(), "abgeschnittener Chunk bei {at}");
        let laenge = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let typ = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap());
        let daten = &bytes[at + 8..at + 8 + laenge];
        assert_eq!(daten.len(), laenge, "die Chunk-Länge passt nicht");
        assert_eq!(laenge % 4, 0, "die Chunk-Länge {laenge} ist kein Vielfaches von 4");
        match typ {
            0x4E4F_534A => {
                assert!(json.is_none(), "zwei JSON-Chunks");
                assert!(!daten.is_empty(), "der JSON-Chunk ist leer");
                json = Some(daten);
            }
            0x004E_4942 => {
                assert!(bin.is_none(), "zwei BIN-Chunks");
                bin = Some(daten);
            }
            t => panic!("unbekannter Chunk-Typ {t:#010x}"),
        }
        at += 8 + laenge;
    }
    assert_eq!(at, bytes.len(), "nach dem letzten Chunk bleibt etwas übrig");

    let json = json.expect("kein JSON-Chunk");
    let bin = bin.expect("kein BIN-Chunk").to_vec();

    // JSON is padded with spaces, the binary part with zeros — the format asks
    // for exactly that, and a viewer is allowed to depend on it.
    let mut j = json;
    while j.last() == Some(&b' ') {
        j = &j[..j.len() - 1];
    }
    assert!(
        !json[j.len()..].iter().any(|&b| b != b' '),
        "der JSON-Chunk ist nicht mit Leerzeichen aufgefüllt"
    );
    let wert: Value = serde_json::from_slice(j).expect("JSON-Chunk lesbar");
    (wert, bin)
}

/// Reads a `VEC3`/`FLOAT` accessor back out of the binary chunk.
fn vec3(json: &Value, bin: &[u8], index: usize) -> Vec<[f32; 3]> {
    let a = &json["accessors"][index];
    let view = &json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
    let anzahl = a["count"].as_u64().unwrap() as usize;
    let off = view["byteOffset"].as_u64().unwrap_or(0) as usize;
    let len = view["byteLength"].as_u64().unwrap() as usize;
    assert!(
        off + len <= bin.len(),
        "bufferView {index} reicht über den BIN-Chunk hinaus"
    );
    assert_eq!(len, anzahl * 12, "VEC3 aus {anzahl} Punkten hat {len} Bytes");
    (0..anzahl)
        .map(|i| {
            let p = off + i * 12;
            [
                f32::from_le_bytes(bin[p..p + 4].try_into().unwrap()),
                f32::from_le_bytes(bin[p + 4..p + 8].try_into().unwrap()),
                f32::from_le_bytes(bin[p + 8..p + 12].try_into().unwrap()),
            ]
        })
        .collect()
}

/// Reads a `SCALAR`/`FLOAT` accessor back out.
fn scalar(json: &Value, bin: &[u8], index: usize) -> Vec<f32> {
    let a = &json["accessors"][index];
    let view = &json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
    let anzahl = a["count"].as_u64().unwrap() as usize;
    let off = view["byteOffset"].as_u64().unwrap_or(0) as usize;
    (0..anzahl)
        .map(|i| {
            let p = off + i * 4;
            f32::from_le_bytes(bin[p..p + 4].try_into().unwrap())
        })
        .collect()
}

/// Read indices of one primitive back out.
fn indizes(json: &Value, bin: &[u8], index: usize) -> Vec<u32> {
    let a = &json["accessors"][index];
    let view = &json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
    let anzahl = a["count"].as_u64().unwrap() as usize;
    let off = view["byteOffset"].as_u64().unwrap_or(0) as usize;
    (0..anzahl)
        .map(|i| u32::from_le_bytes(bin[off + i * 4..off + i * 4 + 4].try_into().unwrap()))
        .collect()
}

fn naeherung(ist: [f32; 3], soll: [f64; 3], was: &str) {
    for k in 0..3 {
        let d = (ist[k] as f64 - soll[k]).abs();
        assert!(
            d <= 1e-6,
            "{was}: Achse {k} ist {:.9} statt {:.9} ({:.2e} m daneben)",
            ist[k],
            soll[k],
            d
        );
    }
}

#[test]
fn bewegung_als_gltf() {
    let s0 = szene();
    let mut m = Bewegung::neu(&s0).expect("Bewegung anlegen");
    assert_eq!(m.bilder(), 0, "Bild 0 ist noch kein Bewegungsbild");

    // Two frames, deliberately not a rigid shift of each other.
    let s1 = bewegt(&s0, 1.10, 0.02);
    let s2 = bewegt(&s0, 0.94, 0.03);
    m.bild(&s1).expect("Bild 1");
    m.bild(&s2).expect("Bild 2");
    assert_eq!(m.bilder(), 2);

    let bytes = m.glb();
    let (json, bin) = zerlegen(&bytes);

    // ── Names ─────────────────────────────────────────────────────────────
    let meshes = json["meshes"].as_array().expect("meshes");
    let nodes = json["nodes"].as_array().expect("nodes");
    let namen: Vec<&str> = nodes.iter().filter_map(|n| n["name"].as_str()).collect();
    assert!(namen.contains(&TEIL), "das Teil {TEIL:?} fehlt in den Knoten: {namen:?}");
    assert!(namen.contains(&"Koerper"), "der Körper fehlt als eigener Knoten");
    assert!(namen.contains(&"Kleid"), "der Kleid-Knoten fehlt");
    assert_eq!(meshes.len(), 2, "ein Mesh je Teil plus eines für den Körper");
    assert_eq!(
        meshes[0]["name"].as_str(),
        Some(TEIL),
        "der Name des Teils steht am Mesh"
    );

    // ── The geometry of frame 0 and of both frames ────────────────────────
    let t = m.bilder();
    for (i, mesh) in meshes.iter().enumerate() {
        let prim = &mesh["primitives"][0];
        let pos = &prim["attributes"]["POSITION"];
        let ist = vec3(&json, &bin, pos.as_u64().unwrap() as usize);
        let ziele = prim["targets"].as_array().expect("morph targets");
        assert_eq!(ziele.len(), t, "Mesh {i}: {} Ziele statt {t}", ziele.len());

        let (soll0, soll_bilder): (Vec<[f64; 3]>, Vec<&Vec<[f64; 3]>>) = if i == 0 {
            (
                s0.x.iter().map(|&p| y_up(p)).collect(),
                vec![&s1.x, &s2.x],
            )
        } else {
            (
                match &s0.koerper[0] {
                    Koerper::Netz { netz, .. } => netz.punkte.iter().map(|&p| y_up(p)).collect(),
                    _ => panic!("Körper der Szene ist kein Netz"),
                },
                vec![
                    match &s1.koerper[0] {
                        Koerper::Netz { netz, .. } => &netz.punkte,
                        _ => panic!(),
                    },
                    match &s2.koerper[0] {
                        Koerper::Netz { netz, .. } => &netz.punkte,
                        _ => panic!(),
                    },
                ],
            )
        };
        assert_eq!(ist.len(), soll0.len(), "Mesh {i}: andere Punktzahl");
        for (k, (&a, &b)) in ist.iter().zip(&soll0).enumerate() {
            naeherung(a, b, &format!("Mesh {i}, Punkt {k}, Grundform"));
        }

        // Base + delta of frame f is the point of frame f.
        for (f, ziel) in ziele.iter().enumerate() {
            let d = vec3(&json, &bin, ziel["POSITION"].as_u64().unwrap() as usize);
            assert_eq!(d.len(), ist.len(), "Ziel {f} hat eine andere Punktzahl");
            for (k, &soll) in soll_bilder[f].iter().enumerate() {
                let summe = [
                    (ist[k][0] + d[k][0]) as f64,
                    (ist[k][1] + d[k][1]) as f64,
                    (ist[k][2] + d[k][2]) as f64,
                ];
                naeherung(
                    [summe[0] as f32, summe[1] as f32, summe[2] as f32],
                    y_up(soll),
                    &format!("Mesh {i}, Punkt {k}, Bild {f}"),
                );
            }
        }

        // The indices are the piece's triangles, read back out of the bytes.
        let dreiecke: Vec<[u32; 3]> = if i == 0 {
            s0.dreiecke.clone()
        } else {
            kasten().dreiecke
        };
        let gelesen = indizes(&json, &bin, prim["indices"].as_u64().unwrap() as usize);
        assert_eq!(gelesen.len(), dreiecke.len() * 3, "Mesh {i}: Indexzahl");
        for (d, g) in dreiecke.iter().zip(gelesen.chunks(3)) {
            assert_eq!(d, g, "Mesh {i}: Dreiecke weichen ab");
        }
    }

    // ── The animation ─────────────────────────────────────────────────────
    let anim = &json["animations"][0];
    assert_eq!(anim["name"].as_str(), Some("Arme heben"));
    let samplers = anim["samplers"].as_array().expect("samplers");
    assert_eq!(samplers.len(), 1, "ein Sampler für alle Kanäle");
    assert_eq!(
        samplers[0]["interpolation"].as_str(),
        Some("STEP"),
        "ein Bild ist ein gelöster Zustand, keine Mischung"
    );
    let kanaele = anim["channels"].as_array().expect("channels");
    assert_eq!(kanaele.len(), 2, "je Knoten mit Mesh ein Kanal");
    for k in kanaele {
        assert_eq!(k["target"]["path"].as_str(), Some("weights"));
        assert_eq!(k["sampler"].as_u64(), Some(0));
        let knoten = k["target"]["node"].as_u64().unwrap() as usize;
        assert!(
            nodes[knoten]["mesh"].is_number(),
            "Kanal auf einen Knoten ohne Mesh"
        );
    }

    let zeiten = scalar(&json, &bin, samplers[0]["input"].as_u64().unwrap() as usize);
    assert_eq!(zeiten.len(), t + 1, "ein Zeitpunkt je Bild, Bild 0 mitgezählt");
    let dt = s0.param.bild_dt as f32;
    for (k, &z) in zeiten.iter().enumerate() {
        assert!(
            (z - k as f32 * dt).abs() < 1e-7,
            "Zeitpunkt {k} ist {z} statt {}",
            k as f32 * dt
        );
    }

    // The weights switch exactly one target on, per keyframe.
    let gewichte = scalar(&json, &bin, samplers[0]["output"].as_u64().unwrap() as usize);
    assert_eq!(gewichte.len(), (t + 1) * t, "ein Gewicht je Bild und Ziel");
    for k in 0..=t {
        for ziel in 0..t {
            let w = gewichte[k * t + ziel];
            let erwartet = if k >= 1 && ziel == k - 1 { 1.0 } else { 0.0 };
            assert_eq!(w, erwartet, "Bild {k}, Ziel {ziel}: Gewicht {w}");
        }
    }
    // Every animated node carries the same weights as its default pose.
    for an in json["nodes"].as_array().unwrap() {
        if an["mesh"].is_number() {
            let w = an["weights"].as_array().expect("weights am Knoten");
            assert_eq!(w.len(), t);
            assert!(w.iter().all(|x| x.as_f64() == Some(0.0)), "die Grundform steht am Anfang");
        }
    }

    // ── Reproduzierbar ────────────────────────────────────────────────────
    let mut nochmal = Bewegung::neu(&s0).expect("Bewegung anlegen");
    nochmal.bild(&s1).expect("Bild 1");
    nochmal.bild(&s2).expect("Bild 2");
    assert_eq!(bytes, nochmal.glb(), "zwei Läufe schreiben verschiedene Bytes");

    eprintln!(
        "== glTF: {} Punkte Kleid + {} Punkte Körper, {t} Ziele, {} Bytes",
        s0.x.len(),
        kasten().punkte.len(),
        bytes.len()
    );
}

/// `klon3d bewegen --bilder 0` never calls [`Bewegung::bild`]: the file then
/// holds the rest state alone, without targets and without an animation. A
/// morph target list of zero entries would be invalid glTF, so the writer has to
/// leave both out.
#[test]
fn ohne_bewegung_nur_die_grundform() {
    let s0 = szene();
    let m = Bewegung::neu(&s0).expect("Bewegung anlegen");
    let bytes = m.glb();
    let (json, bin) = zerlegen(&bytes);

    assert!(
        json.get("animations").is_none(),
        "ohne Bewegungsbild gibt es keine Animation"
    );
    for mesh in json["meshes"].as_array().expect("meshes") {
        let prim = &mesh["primitives"][0];
        assert!(
            prim.get("targets").is_none(),
            "ohne Bewegungsbild gibt es keine Morph-Targets"
        );
        let ist = vec3(&json, &bin, prim["attributes"]["POSITION"].as_u64().unwrap() as usize);
        assert!(!ist.is_empty());
    }
    for n in json["nodes"].as_array().unwrap() {
        assert!(n.get("weights").is_none(), "ohne Ziele gibt es keine Gewichte");
    }
    // The rest state is still the geometry of the scene.
    let mesh0 = &json["meshes"][0];
    let ist = vec3(
        &json,
        &bin,
        mesh0["primitives"][0]["attributes"]["POSITION"].as_u64().unwrap() as usize,
    );
    assert_eq!(ist.len(), s0.x.len());
    for (k, (&a, &b)) in ist.iter().zip(&s0.x).enumerate() {
        naeherung(a, y_up(b), &format!("Punkt {k} der Grundform"));
    }
}

/// A body that is not a triangle net (a cylinder) has no skin to show. The
/// garment is then written alone rather than with a stand-in.
#[test]
fn zylinderkoerper_wird_weggelassen() {
    let netz = rohr_netz(0.2, 0.1, 10.0).expect("Rechteck vernetzbar");
    let vg = Teilvorgabe {
        material: RADICAL_RUBBER_04,
        reduktion: 0.0,
    };
    let lage = |_: usize, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, 0.05];
    let s = Szene::aus_netz(
        &netz,
        &|_| vg,
        &lage,
        vec![Koerper::zylinder_z(0.15)],
        Simparameter::default(),
    );

    let mut m = Bewegung::neu(&s).expect("Bewegung anlegen");
    m.bild(&s).expect("Bild 1");
    let bytes = m.glb();
    let (json, bin) = zerlegen(&bytes);

    let namen: Vec<&str> = json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["name"].as_str())
        .collect();
    assert!(
        !namen.contains(&"Koerper"),
        "ein Zylinder ist keine Haut und gehört nicht in die Datei: {namen:?}"
    );
    assert_eq!(
        json["meshes"].as_array().unwrap().len(),
        1,
        "nur das eine Teil, kein Ersatzkörper"
    );
    // The garment still has its target, and exactly one channel.
    let prim = &json["meshes"][0]["primitives"][0];
    assert_eq!(prim["targets"].as_array().expect("targets").len(), 1);
    assert_eq!(
        json["animations"][0]["channels"].as_array().unwrap().len(),
        1,
        "ein Kanal, weil nur ein Knoten ein Mesh trägt"
    );
    assert!(!bin.is_empty());
}
