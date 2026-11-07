"""Writes the test copy `bluse2_v26_einhalte.sm2d` with sleeve-cap ease.

The delivered pattern `bluse2_v26.sm2d` stays untouched. Showing fault 2 of the
acceptance list ("sleeve cap stands off") needs a cap with real surplus over the
armhole - 10 to 20 mm per cap seam.

The sleeve does not set its cap corner directly at the intersection AB1/AB2 but
through the clamp chain AB1T -> AB1S -> AB1B -> AB1C -> AB1K. AB1T/AB1S read
`Line_AL0_ALB` (armhole at the back up to the design point) and clamp at 1.2 mm.
For any ease above 1.2 mm to arrive, **both** places have to see it:

1. the four `cutSplinePath` intersections (front and back of the armhole),
2. the length **and** the selection condition of the clamp chain AB1T/AB1S
   (right sleeve: AB1RT/AB1RS).

The cap seam consists of two halves, so its effect is 2 x #CapEase.

Console output stays German, like the other test scripts of the repository.
"""
import subprocess
import sys
from pathlib import Path

MUSTER = Path(__file__).resolve().parent.parent / "crates/klon3d-pattern/tests/muster"
SRC = MUSTER / "bluse2_v26.sm2d"
HIER = Path(__file__).resolve().parent.parent
KLON3D = next(
    p for p in (HIER / "target/release/klon3d.exe", HIER / "target/debug/klon3d.exe") if p.exists()
)

EASE = 0.75  # cm per cap half -> 15 mm of ease per cap seam

text = SRC.read_text(encoding="utf-8")

# Mark the test copy so that nobody takes it for the delivered pattern.
alt = "<description>v26 (09.10.2026): "
neu = (
    "<description>TESTKOPIE von v26 (Issues #111): #CapEase = "
    f"{EASE} cm je Kappenhälfte, also {2 * EASE * 10:.1f} mm Mehrweite je "
    "Kappennnaht. Nur für den Nachweis von Fehler 2 der Abnahme; das gelieferte "
    "Muster bluse2_v26.sm2d bleibt unverändert. --- v26 (09.10.2026): "
)
assert text.count(alt) == 1, text.count(alt)
text = text.replace(alt, neu)

# 1. the new variable, right behind the cap construction
i = text.find('name="#CrownExt"')
assert i > 0
end = text.find("/>", i) + 2
variable = (
    f'<variable description="Testkopie #111: extra ease per sleeve-cap half seam '
    f'(cm); the seam has two halves, so the cap gains 2 x this over the armhole" '
    f'formula="{EASE}" name="#CapEase" />'
)
text = text[:end] + variable + text[end:]

# 2. the four intersections of the cap: armhole length + ease
for pid, formel in (
    ("6034", "Line_AL0_ALB"),
    ("6035", "Line_AL0_ALF"),
    ("6057", "Line_AL0R_ALFR"),
    ("6058", "Line_AL0R_ALBR"),
):
    alt = f'id="{pid}" length="{formel}"'
    assert text.count(alt) == 1, (pid, text.count(alt))
    text = text.replace(alt, f'id="{pid}" length="{formel} + #CapEase"')

# 3. length and selection of the clamp chain have to see the ease as well
for name, linie in (
    ("AB1T", "Line_AL0_ALB"),
    ("AB1S", "Line_AL0_ALB"),
    ("AB2T", "Line_AL0_ALF"),
    ("AB2S", "Line_AL0_ALF"),
    ("AB1RT", "Line_AL0R_ALFR"),
    ("AB1RS", "Line_AL0R_ALFR"),
    ("AB2RT", "Line_AL0R_ALBR"),
    ("AB2RS", "Line_AL0R_ALBR"),
):
    i = text.find(f'name="{name}"')
    assert i > 0, name
    s = text.rfind("<point", 0, i)
    e = text.find("/>", i) + 2
    punkt = text[s:e]
    # The same line appears twice in the point: as length and as selection.
    n = punkt.count(linie)
    assert n >= 1, (name, linie)
    text = text[:s] + punkt.replace(linie, f"({linie} + #CapEase)") + text[e:]
    print(f"{name}: {n}x {linie}")

dst = MUSTER / "bluse2_v26_einhalte.sm2d"
dst.write_text(text, encoding="utf-8")
print(f"geschrieben: {dst}")

# Counter-check: only V13/V14 may change. The splitter looks for the seam list
# next to the pattern; the copy has its own.
out = subprocess.run(
    [str(KLON3D), "naehte", str(dst)],
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
    if not f or f[0] not in ("V05", "V06", "V13", "V14", "V15", "V16"):
        continue
    zahlen = [x for x in f[1:] if x.lstrip("+-").replace(".", "", 1).isdigit()]
    assert len(zahlen) >= 3, zeile
    zeilen[f[0]] = tuple(float(x) for x in zahlen[-3:])

print(f"{'Naht':5s} {'L_A':>9s} {'L_B':>9s} {'delta':>8s}")
for k, (a, b, _) in zeilen.items():
    print(f"{k:5s} {a:9.4f} {b:9.4f} {a - b:+8.4f}")

# Both cap seams carry 2 x #CapEase (in cm), i.e. 15 mm; everything else stays.
for k in ("V13", "V14"):
    assert abs((zeilen[k][0] - zeilen[k][1]) - 2 * EASE) < 0.002, zeilen[k]
for k in ("V05", "V06", "V15", "V16"):
    assert abs(zeilen[k][0] - zeilen[k][1]) <= 0.08, zeilen[k]
print("Gegenprüfung: Kappennähte +%.1f mm, alle anderen unverändert" % (2 * EASE * 10))
