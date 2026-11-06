//! glTF 2.0 (`.glb`) out of the motion path: one mesh with one morph target per
//! solved frame, and an animation that switches between them.
//!
//! **Why morph targets and not one mesh per frame.** A motion of 60 frames
//! would otherwise repeat the whole geometry 60 times. A morph target carries
//! only the displacement against frame 0, which is three floats per point.
//!
//! **Interpolation is `STEP`.** Every frame is a solved state; a blend between
//! two solved states is a shape nobody computed, and the cloth does not pass
//! through it.
//!
//! **Axes.** glTF is y-up, right-handed; the scene is z-up. The conversion is
//! `(x, y, z) ↦ (x, z, −y)` and is applied once when the file is built, so every
//! position in the file — base and displacement — is in the file's frame.
//!
//! **Pieces keep their names.** A node is the only place glTF allows a name a
//! viewer lists, so each piece becomes a node of its own; all of them hang under
//! one garment node. The body is a node of its own as well, so it can be hidden
//! and the garment examined alone.
//!
//! What is **not** in the file: the strain and distance maps (they stay CSV),
//! colours, and anything read back. The file is written, not imported.

use std::fmt::Write as _;
use std::ops::Range;

use klon3d_sim::{Koerper, Szene};

/// A piece of the garment: its name and where its triangles sit in the scene.
struct Teil {
    name: String,
    dreiecke: Range<usize>,
}

/// The body as one node of its own.
struct Haut {
    punkte: Vec<[f64; 3]>,
    dreiecke: Vec<[u32; 3]>,
}

/// One solved frame after frame 0, in scene coordinates (m, z up).
struct Bildstand {
    kleid: Vec<[f64; 3]>,
    haut: Option<Vec<[f64; 3]>>,
}

/// A motion collected frame by frame: frame 0 from [`Bewegung::neu`], then one
/// [`Bewegung::bild`] per solved frame, then [`Bewegung::glb`].
pub struct Bewegung {
    kleid: Vec<[f64; 3]>,
    kleid_dreiecke: Vec<[u32; 3]>,
    teile: Vec<Teil>,
    haut: Option<Haut>,
    bilder: Vec<Bildstand>,
    /// Length of one frame (s): keyframe `k` sits at `k · dt`.
    dt: f64,
}

/// `(x, y, z) ↦ (x, z, −y)`: scene frame (z up) to the file's frame (y up).
fn y_up(q: [f64; 3]) -> [f64; 3] {
    [q[0], q[2], -q[1]]
}

fn plus(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

impl Bewegung {
    /// Frame 0: the state the garment is handed in, with the body of that state.
    ///
    /// A body that is not a triangle net (a cylinder) is left out; there is no
    /// skin to show then, and the garment is written alone rather than with a
    /// stand-in.
    pub fn neu(s: &Szene) -> Result<Bewegung, String> {
        if s.x.is_empty() {
            return Err("die Szene hat keine Stoffpunkte; es gibt nichts zu schreiben".into());
        }
        let teile = s
            .teile
            .iter()
            .map(|t| Teil {
                name: t.name.clone(),
                dreiecke: t.dreiecke.clone(),
            })
            .collect();
        let haut = match s.koerper.as_slice() {
            [Koerper::Netz { netz, versatz, .. }] => Some(Haut {
                punkte: netz.punkte.iter().map(|&p| plus(p, *versatz)).collect(),
                dreiecke: netz.dreiecke.clone(),
            }),
            _ => None,
        };
        Ok(Bewegung {
            kleid: s.x.clone(),
            kleid_dreiecke: s.dreiecke.clone(),
            teile,
            haut,
            bilder: Vec::new(),
            dt: s.param.bild_dt,
        })
    }

    /// One solved frame. The point counts have to be those of frame 0 — a scene
    /// that changed its mesh between frames is an error, not a shorter target.
    pub fn bild(&mut self, s: &Szene) -> Result<(), String> {
        if s.x.len() != self.kleid.len() {
            return Err(format!(
                "das Bild hat {} Stoffpunkte statt {} — es gehört nicht zu diesem Lauf",
                s.x.len(),
                self.kleid.len()
            ));
        }
        let haut = match (&self.haut, s.koerper.as_slice()) {
            (Some(h), [Koerper::Netz { netz, versatz, .. }]) => {
                if netz.punkte.len() != h.punkte.len() {
                    return Err(format!(
                        "der Körper hat jetzt {} Punkte statt {}",
                        netz.punkte.len(),
                        h.punkte.len()
                    ));
                }
                Some(netz.punkte.iter().map(|&p| plus(p, *versatz)).collect())
            }
            (Some(_), _) => {
                return Err("der Körper der Szene ist nicht mehr dasselbe Netz".into());
            }
            (None, _) => None,
        };
        self.bilder.push(Bildstand {
            kleid: s.x.clone(),
            haut,
        });
        Ok(())
    }

    /// Number of motion frames written (frame 0 is the base state).
    pub fn bilder(&self) -> usize {
        self.bilder.len()
    }

    /// The file. Deterministic: the same motion gives the same bytes.
    pub fn glb(&self) -> Vec<u8> {
        let mut b = Baufeld::default();
        let t = self.bilder.len();

        // ── Garment: base positions, one displacement per frame, indices per
        // piece (all pieces index the same point array) ─────────────────────
        let pos_kleid = b.vec3(&self.kleid, true);
        let ziele_kleid: Vec<u32> = self
            .bilder
            .iter()
            .map(|bild| b.vec3(&delta(&bild.kleid, &self.kleid), true))
            .collect();
        let idx_kleid: Vec<u32> = self
            .teile
            .iter()
            .map(|t| b.indizes(&self.kleid_dreiecke[t.dreiecke.clone()]))
            .collect();

        // ── Body, same shape ────────────────────────────────────────────────
        let pos_haut = self.haut.as_ref().map(|h| b.vec3(&h.punkte, true));
        let ziele_haut: Vec<u32> = match &self.haut {
            Some(h) => self
                .bilder
                .iter()
                .map(|bild| {
                    let haut = bild
                        .haut
                        .as_deref()
                        .expect("jedes Bild trägt den Körper, wenn Bild 0 ihn trug");
                    b.vec3(&delta(haut, &h.punkte), true)
                })
                .collect(),
            None => Vec::new(),
        };
        let idx_haut = self.haut.as_ref().map(|h| b.indizes(&h.dreiecke));

        // ── Time and weights. Both nodes share one weights array: every frame
        // is one morph target, so the weights are the same for all of them.
        let zeiten = (0..=t)
            .map(|k| k as f64 * self.dt)
            .collect::<Vec<f64>>();
        let (zeit, gewichte) = if t == 0 {
            (None, None)
        } else {
            let mut w = Vec::with_capacity((t + 1) * t);
            for k in 0..=t {
                for ziel in 0..t {
                    w.push(if k >= 1 && ziel == k - 1 { 1.0 } else { 0.0 });
                }
            }
            (Some(b.scalar(&zeiten, true)), Some(b.scalar(&w, false)))
        };

        let json = self.json(
            &b,
            pos_kleid,
            &ziele_kleid,
            &idx_kleid,
            pos_haut,
            &ziele_haut,
            idx_haut,
            zeit,
            gewichte,
            t,
        );
        glb_datei(&json, &b.bin)
    }

    #[allow(clippy::too_many_arguments)]
    fn json(
        &self,
        b: &Baufeld,
        pos_kleid: u32,
        ziele_kleid: &[u32],
        idx_kleid: &[u32],
        pos_haut: Option<u32>,
        ziele_haut: &[u32],
        idx_haut: Option<u32>,
        zeit: Option<u32>,
        gewichte: Option<u32>,
        t: usize,
    ) -> String {
        let n_teile = self.teile.len();
        let mut s = String::new();
        s += "{\n\"asset\":{\"version\":\"2.0\",\"generator\":\"klon3d\"},\n";

        // ── Meshes: one per piece and one for the body, all over the same
        // point array ──────────────────────────────────────────────────────
        s += "\"meshes\":[\n";
        let mut first = true;
        for (i, _t) in self.teile.iter().enumerate() {
            if !first {
                s += ",\n";
            }
            first = false;
            let _ = write!(
                s,
                "{{\"name\":{},\"primitives\":[{}]}}",
                json_str(&self.teile[i].name),
                primitive(pos_kleid, idx_kleid[i], ziele_kleid)
            );
        }
        if let (Some(p), Some(i)) = (pos_haut, idx_haut) {
            if !first {
                s += ",\n";
            }
            let _ = write!(
                s,
                "{{\"name\":\"Koerper\",\"primitives\":[{}]}}",
                primitive(p, i, ziele_haut)
            );
        }
        s += "\n],\n";

        // ── Nodes: a garment node holding one node per piece, and the body ──
        s += "\"nodes\":[\n";
        let kinder: Vec<String> = (1..=n_teile).map(|i| i.to_string()).collect();
        let _ = write!(
            s,
            "{{\"name\":\"Kleid\",\"children\":[{}]}}",
            kinder.join(",")
        );
        for (i, teil) in self.teile.iter().enumerate() {
            s += ",\n";
            let _ = write!(s, "{{\"name\":{},", json_str(&teil.name));
            let _ = write!(s, "\"mesh\":{i}");
            if t > 0 {
                s += &format!(",\"weights\":{}", nullen(t));
            }
            s += "}";
        }
        let mut hauptknoten = vec![0usize];
        if pos_haut.is_some() {
            let i = n_teile;
            s += ",\n";
            let _ = write!(s, "{{\"name\":\"Koerper\",\"mesh\":{i}");
            if t > 0 {
                s += &format!(",\"weights\":{}", nullen(t));
            }
            s += "}";
            hauptknoten.push(n_teile + 1);
        }
        s += "\n],\n";

        let _ = write!(
            s,
            "\"scenes\":[{{\"name\":\"Bewegung\",\"nodes\":[{}]}}],\n\"scene\":0,\n",
            hauptknoten
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        // ── Animation: one channel per node, all on the same sampler ────────
        if let (Some(zeit), Some(gewichte)) = (zeit, gewichte) {
            let mut kanaele: Vec<String> = Vec::new();
            for i in 1..=n_teile {
                kanaele.push(format!(
                    "{{\"sampler\":0,\"target\":{{\"node\":{i},\"path\":\"weights\"}}}}"
                ));
            }
            if pos_haut.is_some() {
                kanaele.push(format!(
                    "{{\"sampler\":0,\"target\":{{\"node\":{},\"path\":\"weights\"}}}}",
                    n_teile + 1
                ));
            }
            let _ = write!(
                s,
                "\"animations\":[{{\"name\":\"Arme heben\",\
                 \"samplers\":[{{\"input\":{zeit},\"output\":{gewichte},\"interpolation\":\"STEP\"}}],\
                 \"channels\":[{}]}}],\n",
                kanaele.join(",")
            );
        }

        s += "\"materials\":[{\"name\":\"Stoff\",\"doubleSided\":true,\
             \"pbrMetallicRoughness\":{\"baseColorFactor\":[0.42,0.42,0.46,1.0],\
             \"metallicFactor\":0.0,\"roughnessFactor\":0.9}}],\n";

        // ── Buffers, views, accessors ───────────────────────────────────────
        let _ = write!(
            s,
            "\"buffers\":[{{\"byteLength\":{}}}],\n",
            b.bin.len()
        );
        s += "\"bufferViews\":[\n";
        s += &b.views.join(",\n");
        s += "\n],\n\"accessors\":[\n";
        s += &b.accessors.join(",\n");
        s += "\n]\n}\n";
        s
    }
}

/// `p - p0` per point: what a morph target stores.
fn delta(p: &[[f64; 3]], p0: &[[f64; 3]]) -> Vec<[f64; 3]> {
    p.iter()
        .zip(p0)
        .map(|(a, b)| [a[0] - b[0], a[1] - b[1], a[2] - b[2]])
        .collect()
}

fn nullen(n: usize) -> String {
    let w = vec!["0"; n].join(",");
    format!("[{w}]")
}

fn primitive(pos: u32, ind: u32, ziele: &[u32]) -> String {
    let mut s = format!("{{\"attributes\":{{\"POSITION\":{pos}}},\"indices\":{ind}");
    if !ziele.is_empty() {
        s += ",\"targets\":[";
        for (i, z) in ziele.iter().enumerate() {
            if i > 0 {
                s += ",";
            }
            let _ = write!(s, "{{\"POSITION\":{z}}}");
        }
        s += "]";
    }
    s += ",\"material\":0}";
    s
}

/// A JSON string with the escaping glTF needs. Non-ASCII stays as it is (the
/// file is UTF-8, which is what JSON expects).
fn json_str(t: &str) -> String {
    let mut s = String::with_capacity(t.len() + 2);
    s.push('"');
    for c in t.chars() {
        match c {
            '"' => s += "\\\"",
            '\\' => s += "\\\\",
            '\n' => s += "\\n",
            '\r' => s += "\\r",
            '\t' => s += "\\t",
            c if (c as u32) < 0x20 => {
                let _ = write!(s, "\\u{:04x}", c as u32);
            }
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

/// The binary buffer and its JSON bookkeeping. Every view starts at a multiple
/// of four, so the accessors in it are aligned as the format asks.
#[derive(Default)]
struct Baufeld {
    bin: Vec<u8>,
    views: Vec<String>,
    accessors: Vec<String>,
}

/// glTF component types.
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
/// bufferView targets.
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;

impl Baufeld {
    fn ansehen(&mut self, daten: &[u8], ziel: Option<u32>) -> u32 {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let off = self.bin.len();
        self.bin.extend_from_slice(daten);
        let i = self.views.len();
        let ziel = ziel.map_or(String::new(), |z| format!(",\"target\":{z}"));
        self.views.push(format!(
            "{{\"buffer\":0,\"byteOffset\":{off},\"byteLength\":{}{ziel}}}",
            daten.len()
        ));
        i as u32
    }

    /// A `VEC3` accessor over `punkte`, converted to the file's y-up frame.
    /// `mit_grenzen` writes `min`/`max`, which the format requires for
    /// `POSITION`.
    fn vec3(&mut self, punkte: &[[f64; 3]], mit_grenzen: bool) -> u32 {
        let mut bytes = Vec::with_capacity(punkte.len() * 12);
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in punkte {
            let q = y_up(*p);
            for k in 0..3 {
                let v = q[k] as f32;
                bytes.extend_from_slice(&v.to_le_bytes());
                min[k] = min[k].min(v);
                max[k] = max[k].max(v);
            }
        }
        let view = self.ansehen(&bytes, Some(ARRAY_BUFFER));
        let i = self.accessors.len();
        let mut s = format!(
            "{{\"bufferView\":{view},\"componentType\":{FLOAT},\"count\":{},\"type\":\"VEC3\"",
            punkte.len()
        );
        if mit_grenzen && !punkte.is_empty() {
            let _ = write!(
                s,
                ",\"min\":[{},{},{}],\"max\":[{},{},{}]",
                f32_json(min[0]),
                f32_json(min[1]),
                f32_json(min[2]),
                f32_json(max[0]),
                f32_json(max[1]),
                f32_json(max[2])
            );
        }
        s.push('}');
        self.accessors.push(s);
        i as u32
    }

    /// A `SCALAR` float array: keyframe times and morph weights.
    fn scalar(&mut self, werte: &[f64], mit_grenzen: bool) -> u32 {
        let mut bytes = Vec::with_capacity(werte.len() * 4);
        let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
        for &w in werte {
            let v = w as f32;
            bytes.extend_from_slice(&v.to_le_bytes());
            min = min.min(v);
            max = max.max(v);
        }
        let view = self.ansehen(&bytes, None);
        let i = self.accessors.len();
        let mut s = format!(
            "{{\"bufferView\":{view},\"componentType\":{FLOAT},\"count\":{},\"type\":\"SCALAR\"",
            werte.len()
        );
        if mit_grenzen && !werte.is_empty() {
            let _ = write!(s, ",\"min\":[{}],\"max\":[{}]", f32_json(min), f32_json(max));
        }
        s.push('}');
        self.accessors.push(s);
        i as u32
    }

    /// Triangle indices, one accessor over the given triangles.
    fn indizes(&mut self, dreiecke: &[[u32; 3]]) -> u32 {
        let mut bytes = Vec::with_capacity(dreiecke.len() * 12);
        for d in dreiecke {
            for &i in d {
                bytes.extend_from_slice(&i.to_le_bytes());
            }
        }
        let view = self.ansehen(&bytes, Some(ELEMENT_ARRAY_BUFFER));
        let i = self.accessors.len();
        self.accessors.push(format!(
            "{{\"bufferView\":{view},\"componentType\":{UNSIGNED_INT},\"count\":{},\"type\":\"SCALAR\"}}",
            dreiecke.len() * 3
        ));
        i as u32
    }
}

/// A float as JSON. `Display` gives the shortest form that reads back to the
/// same `f32`, which keeps the file small and the output deterministic.
fn f32_json(v: f32) -> String {
    if v == 0.0 {
        return "0".into();
    }
    format!("{v}")
}

/// The container: header, JSON chunk, binary chunk, every chunk padded as the
/// format asks (spaces for JSON, zeros for the binary part).
fn glb_datei(json: &str, bin: &[u8]) -> Vec<u8> {
    let mut j = json.as_bytes().to_vec();
    while j.len() % 4 != 0 {
        j.push(b' ');
    }
    let mut d = bin.to_vec();
    while d.len() % 4 != 0 {
        d.push(0);
    }
    let gesamt = 12 + 8 + j.len() + 8 + d.len();
    let mut out = Vec::with_capacity(gesamt);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(gesamt as u32).to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes()); // "JSON"
    out.extend_from_slice(&j);
    out.extend_from_slice(&(d.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes()); // "BIN\0"
    out.extend_from_slice(&d);
    out
}
