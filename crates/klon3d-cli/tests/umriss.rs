//! `klon3d umriss` shows the seam line Seamly2D builds, the one seams and
//! mesh use — not the drawn curves.

use std::path::PathBuf;
use std::process::Command;

/// Bolero v37: the bishop sleeve carries notches on a curve named several
/// times. On the drawn curves its edge ran in a zigzag, 2142.7 mm; Seamly2D's
/// own export gives 1418.58 mm (`klon3d-pattern/tests/leser_erweitert.rs`).
#[test]
fn bishop_sleeve_as_in_seamly2d() {
    let muster = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../klon3d-pattern/tests/muster/hexen_2026_10_27/bolero_v37.sm2d");
    let o = Command::new(env!("CARGO_BIN_EXE_klon3d")).arg("umriss").arg(&muster).output().expect("klon3d starts");
    let t = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{t}");
    let zeilen: Vec<&str> = t.lines().filter(|z| z.contains("Bischofaermel")).collect();
    assert_eq!(zeilen.len(), 2, "{t}");
    for z in zeilen {
        assert!(z.contains("1418.6 mm Naht"), "{z}");
    }
    assert!(t.contains("Rueckenteil 1x") && t.contains("1342.8 mm Naht"), "{t}");
}
