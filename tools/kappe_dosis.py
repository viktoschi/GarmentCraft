"""Writes the test copy `bluse2_v26_kappe40.sm2d` with 40.0 mm of cap ease.

Task #111 built `bluse2_v26_einhalte.sm2d`, a copy of the delivered blouse v26
whose cap seams are 15.0 mm longer than their armholes, and dressed it: the cap
still lay on the shoulder (0.9 mm of air against a 10 mm floor). The reason was
the armhole, which is cut 41.5 cm long around a 41 cm body and gives the surplus
somewhere to leave.

Cutting the armhole shorter does **not** help - the cap reads the armhole length
through `cutSplinePath` over `Line_AL0_ALB` / `Line_AL0_ALF`, so both sides lose
the same millimetres and the ease stays. What is open is the dose: 15 mm of ease
gave 0.9 mm of air. How much ease does a 10 mm stand-off need?

This script writes the second point of that curve. It reads the #111 copy and
changes **one formula** (`#CapEase` 0.75 -> 2 cm) plus the description. Both cap
seams end up 40.0 mm longer than their armholes; every other seam keeps its
length. Because no node id moves, the seam list and the placement of the #111
copy apply unchanged and are copied next to the new pattern with a header.

The delivered `bluse2_v26.sm2d` and the #111 copy are only read.

Console output stays German, like the other test scripts of the repository.
"""
import subprocess
import sys
from pathlib import Path

MUSTER = Path(__file__).resolve().parent.parent / "crates/klon3d-pattern/tests/muster"
SRC = MUSTER / "bluse2_v26_einhalte.sm2d"
ZIEL = MUSTER / "bluse2_v26_kappe40.sm2d"
HIER = Path(__file__).resolve().parent.parent
KLON3D = next(
    p for p in (HIER / "target/release/klon3d.exe", HIER / "target/debug/klon3d.exe") if p.exists()
)

# cm per cap half -> 2 x 2 cm = 40 mm of ease per cap seam
EASE_ALT = 0.75
EASE = 2.0

# The header of the side files, so nobody takes them for v26's own.
KOPF_NAEHTE = """# Seam list of bluse2_v26_kappe40.sm2d - the test copy with 40.0 mm of cap ease
# (#115). Content below is v26's seam list unchanged: the copy keeps every node
# id, so the same seam definitions apply.
"""
KOPF_LAGE = """# Placement of bluse2_v26_kappe40.sm2d - the test copy with 40.0 mm of cap ease
# (#115). Content below is v26's placement unchanged: the copy keeps every node
# id, so the same pieces find the same zones, anchors and heights.
"""

text = SRC.read_text(encoding="utf-8")

# 1. The description: the copy names itself and its dose.
alt = "<description>TESTKOPIE von v26 (Issues #111): "
neu = (
    "<description>TESTKOPIE von v26 (Issues #115): #CapEase = "
    f"{EASE} cm je Kappenhälfte, also {2 * EASE * 10:.1f} mm Mehrweite je "
    "Kappennnaht (Dosis-Wirkung zu #111, das 15 mm hatte). Nur für den Nachweis "
    "von Fehler 2 der Abnahme; das gelieferte Muster bluse2_v26.sm2d bleibt "
    "unverändert. --- #111: "
)
assert text.count(alt) == 1, text.count(alt)
text = text.replace(alt, neu)

# 2. The dose itself. #111 already routed #CapEase into the four cutSplinePath
#    intersections and the eight clamp-chain points, so changing its value is
#    enough - no second variable, no further formula site.
alt = f'formula="{EASE_ALT}" name="#CapEase"'
assert text.count(alt) == 1, ("#CapEase", text.count(alt))
text = text.replace(
    alt,
    'formula="%s" name="#CapEase"' % EASE,
)
# The variable's own note still describes the #111 dose.
alt = "Testkopie #111: extra ease per sleeve-cap half seam"
assert text.count(alt) == 1, text.count(alt)
text = text.replace(alt, "Testkopie #115 (Dosis-Wirkung zu #111): extra ease per sleeve-cap half seam")

ZIEL.write_text(text, encoding="utf-8")
print(f"geschrieben: {ZIEL}")

# 3. The side files, v26's content behind a header.
for name, kopf in (("naehte", KOPF_NAEHTE), ("lage", KOPF_LAGE)):
    quelle = MUSTER / f"bluse2_v26.{name}"
    ziel = MUSTER / f"bluse2_v26_kappe40.{name}"
    ziel.write_text(kopf + quelle.read_text(encoding="utf-8"), encoding="utf-8")
    print(f"geschrieben: {ziel}")

# 4. Counter-check with the splitter. The seam list sits next to the pattern.
out = subprocess.run(
    [str(KLON3D), "naehte", str(ZIEL)],
    capture_output=True,
    text=True,
    encoding="utf-8",
)
if out.returncode != 0 and "größte Abweichung" not in out.stdout:
    print("STDERR:", out.stderr.strip()[:800])
    sys.exit(out.returncode)

zeilen = {}
for zeile in out.stdout.splitlines():
    f = zeile.split()
    if not f or f[0] not in ("V05", "V06", "V07", "V13", "V14", "V15", "V16"):
        continue
    zahlen = [x for x in f[1:] if x.lstrip("+-").replace(".", "", 1).isdigit()]
    # `V13 Seite A: ABU1 bei 7.9381 ohne Gegenstück` is a note, not a seam row.
    if len(zahlen) < 3:
        continue
    zeilen[f[0]] = tuple(float(x) for x in zahlen[-3:])

print(f"{'Naht':5s} {'L_A':>9s} {'L_B':>9s} {'delta':>8s}")
for k, (a, b, _) in zeilen.items():
    print(f"{k:5s} {a:9.4f} {b:9.4f} {a - b:+8.4f}")

# Both cap seams carry 2 x #CapEase (in cm), i.e. 40 mm; everything else stays
# where #111 left it (V05/V06 -0.0342, V07 +0.0005, V15/V16 0.0000).
for k in ("V13", "V14"):
    assert abs((zeilen[k][0] - zeilen[k][1]) - 2 * EASE) < 0.002, zeilen[k]
assert abs((zeilen["V05"][0] - zeilen["V05"][1]) + 0.0342) < 0.01, zeilen["V05"]
assert abs((zeilen["V06"][0] - zeilen["V06"][1]) + 0.0342) < 0.01, zeilen["V06"]
for k in ("V15", "V16"):
    assert abs(zeilen[k][0] - zeilen[k][1]) <= 0.08, zeilen[k]
print("Gegenprüfung: Kappennähte +%.1f mm, alle anderen unverändert" % (2 * EASE * 10))
