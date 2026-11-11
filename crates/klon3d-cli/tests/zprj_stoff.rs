//! Choosing the fabric of a `.zprj` project, seen from the command line.
//!
//! `anziehen --zprj` takes the body from a CLO project. #131 adds the fabric
//! from the same project, and the decision it makes has to be loud: a project
//! with several fabrics does not say which one the garment is made of, and the
//! pieces' own fabric index cannot be used - they are named by CLO
//! (`Pattern_999671`), a pattern and its placement file by the names from the
//! `.sm2d`. So these tests assert the refusal as much as the success.
//!
//! The command is driven through the binary (`CARGO_BIN_EXE_klon3d`), like the
//! other CLI tests: `klon3d-cli` has no library target, and the point here is
//! that the option reaches the command at all.
//!
//! **Why the success test stops the child.** A full `anziehen` on the reference
//! project sews and settles for 48 minutes (measured: 2 867 s, debug build). The
//! report lines this test is about are printed *before* the solver starts, so
//! the child is read until the material line appears and then killed. That is
//! the whole claim - the chosen fabric becomes the material - without spending
//! three quarters of an hour to prove it, and without leaving a simulation
//! running behind every test run.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PRUEFPROJEKT: &str =
    r"D:\ai\rea\work\messung\japanese_school_uniform\japanese_school_uniform.zprj";

fn projekt() -> Option<PathBuf> {
    let p = PathBuf::from(PRUEFPROJEKT);
    if p.is_file() {
        Some(p)
    } else {
        eprintln!("übersprungen: {PRUEFPROJEKT} fehlt");
        None
    }
}

fn muster() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../klon3d-pattern/tests/muster/bluse2_v26.sm2d")
}

fn befehl(rest: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_klon3d"));
    c.arg("anziehen").arg(muster()).arg("--zprj").arg(PRUEFPROJEKT);
    c.args(rest);
    c
}

/// Runs `anziehen` to its end. Only used for paths that refuse before the
/// solver starts, so this returns in a second.
fn anziehen_bis_ende(rest: &[&str]) -> (bool, String) {
    let o = befehl(rest).output().expect("klon3d startet");
    let mut t = String::from_utf8_lossy(&o.stdout).into_owned();
    t.push_str(&String::from_utf8_lossy(&o.stderr));
    (o.status.success(), t)
}

/// Runs `anziehen` until `bis` appears on stdout, then kills the child.
///
/// Returns every line read so far and whether the marker was reached. A marker
/// that does not appear within `geduld` is reported as not reached instead of
/// hanging the test suite.
fn anziehen_bis_zeile(rest: &[&str], bis: &str, geduld: Duration) -> (bool, String) {
    let mut kind = befehl(rest)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("klon3d startet");
    let stdout = kind.stdout.take().expect("stdout ist gepipet");

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for z in BufReader::new(stdout).lines() {
            let Ok(z) = z else { break };
            // The reader ends when the child is killed and the pipe closes.
            if tx.send(z).is_err() {
                break;
            }
        }
    });

    let mut zeilen = Vec::new();
    let mut erreicht = false;
    let ende = std::time::Instant::now() + geduld;
    while std::time::Instant::now() < ende {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(z) => {
                if z.contains(bis) {
                    erreicht = true;
                    zeilen.push(z);
                    break;
                }
                zeilen.push(z);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = kind.kill();
    let _ = kind.wait();
    (erreicht, zeilen.join("\n"))
}

/// A name that is not in the project is refused and the alternatives are named
/// - not silently resolved to the first fabric.
#[test]
fn ein_unbekannter_name_wird_abgelehnt() {
    if projekt().is_none() {
        return;
    }
    let (ok, t) = anziehen_bis_ende(&["--zprj-stoff", "gibt-es-nicht"]);
    assert!(!ok, "ein unbekannter Stoffname darf nicht durchlaufen:\n{t}");
    assert!(t.contains("--zprj-stoff"), "die Meldung nennt die Option:\n{t}");
    // Every fabric of the project has to be findable in the message, or the
    // caller cannot copy a name out of it.
    for n in [
        "black latex 0.4mm",
        "white latex 0.4mm",
        "black latex 0.8mm",
        "Ribbon_14",
    ] {
        assert!(t.contains(n), "die Meldung nennt {n} nicht:\n{t}");
    }
}

/// The exact fabric name reaches the solver and the report says which one it
/// is - the numbers come from the project, not from the built-in profile.
#[test]
fn der_genannte_stoff_steht_im_bericht() {
    if projekt().is_none() {
        return;
    }
    let (erreicht, t) = anziehen_bis_zeile(
        &["--zprj-stoff", "white latex 0.4mm"],
        "Material     white latex 0.4mm",
        Duration::from_secs(300),
    );
    assert!(
        erreicht,
        "der Bericht muss den Projektstoff als Material fuehren:\n{t}"
    );
    // The provenance line comes first and names every fabric of the project
    // with its piece count, so the reader can see why one had to be chosen.
    assert!(t.contains("Stoffquelle"), "die Herkunft fehlt:\n{t}");
    assert!(
        t.contains("white latex 0.4mm (18 Teile)"),
        "die Teilezahl des gewaehlten Stoffs fehlt:\n{t}"
    );
}

/// A **profile** name is not unique: `white latex 0.4mm` and `black latex
/// 0.4mm` are both `Latex_0.4mm_Radical_Rubber` in the reference project. So a
/// profile name that matches two fabrics is refused rather than resolved to the
/// first one - the caller has to say which of the two.
#[test]
fn ein_mehrdeutiger_profilname_wird_abgelehnt() {
    if projekt().is_none() {
        return;
    }
    let (ok, t) = anziehen_bis_ende(&["--zprj-stoff", "Latex_0.4mm_Radical_Rubber"]);
    assert!(!ok, "zwei Treffer dürfen nicht stillschweigend zum ersten werden:\n{t}");
    assert!(t.contains("2 Stoffe passen"), "{t}");
    assert!(
        t.contains("white latex 0.4mm") && t.contains("black latex 0.4mm"),
        "beide müssen genannt sein:\n{t}"
    );
}

/// Without `--zprj` there is no project to take a fabric from; the message says
/// where the fabric would have to come from instead of ignoring the option.
#[test]
fn stoff_ohne_projekt_wird_abgelehnt() {
    let o = Command::new(env!("CARGO_BIN_EXE_klon3d"))
        .arg("anziehen")
        .arg(muster())
        .args(["--zprj-stoff", "white latex 0.4mm"])
        .output()
        .expect("klon3d startet");
    assert!(!o.status.success(), "--zprj-stoff ohne --zprj darf nicht laufen");
    let t = String::from_utf8_lossy(&o.stderr);
    assert!(t.contains("--zprj"), "die Meldung nennt --zprj:\n{t}");
}

/// Two fabric sources at once: `--material` and `--zprj-stoff` both name a
/// fabric, and letting one silently win would make the report unable to say why
/// the garment has the numbers it has.
#[test]
fn zwei_stoffquellen_zusammen_werden_abgelehnt() {
    if projekt().is_none() {
        return;
    }
    let (ok, t) = anziehen_bis_ende(&[
        "--zprj-stoff",
        "white latex 0.4mm",
        "--material",
        "Radical",
    ]);
    assert!(!ok, "beide Quellen zusammen dürfen nicht laufen:\n{t}");
    assert!(
        t.contains("--material") && t.contains("--zprj-stoff"),
        "die Meldung nennt beide Optionen:\n{t}"
    );
}

/// `--zprj` alone keeps the built-in material - and says so. Without this line
/// "Material Latex_0.4mm_Radical_Rubber" next to a project reads as if the
/// project had supplied the fabric.
#[test]
fn ohne_stoffwahl_kommt_der_stoff_nicht_aus_dem_projekt() {
    if projekt().is_none() {
        return;
    }
    let (erreicht, t) = anziehen_bis_zeile(
        &[],
        "Material     ",
        Duration::from_secs(300),
    );
    assert!(erreicht, "der Bericht muss ein Material nennen:\n{t}");
    assert!(
        t.contains("Stoff         nicht aus"),
        "der Bericht muss sagen, dass der Stoff nicht aus dem Projekt kommt:\n{t}"
    );
    assert!(
        t.contains("Latex_0.4mm_Radical_Rubber"),
        "ohne Wahl gilt das eingebaute Profil:\n{t}"
    );
}
