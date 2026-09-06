"""Check that the front-page artwork still tells the truth, and say so in a receipt.

    python tools/check_repo_art.py           # a readable summary
    python tools/check_repo_art.py --json    # the same run as a receipt

A picture in a README is never diffed, so it drifts from the text silently:
somebody edits a stage name, nobody re-renders, and the diagram now describes a
version of the tool that no longer exists. Here the picture is a pure function
of a spec that IS diffable, so this re-renders it and compares bytes.

The sibling repositories run these same gates from their own test runners. This
one is a Rust project, so there is no Python suite to hang them on: the script IS
the check, and CI runs it as its own job on every push and pull request.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import render_repo_art as RENDER  # noqa: E402
import repo_art as ART_LIB  # noqa: E402
import check_repo_card as CARD_GATE  # noqa: E402
import check_repo_flow as FLOW_GATE  # noqa: E402
import repo_flow as FLOW  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
ART = ROOT / "docs" / "art"
SCHEMA = "buildlang.repo-art/v1"

# Where an illustration lives. .github/assets/ and docs/brand/ are deliberately
# outside this set: they hold the social-preview source and the flagship heroes,
# which other gates already cover.
SHOWN_DIRS = ("docs/art",)

EM_DASH = "—"


def _rel(path: Path) -> str:
    return str(path.relative_to(ROOT)).replace("\\", "/")


def _specs() -> list[Path]:
    return sorted(ART.glob("*.art.json"))


def _loaded() -> list[dict]:
    return [json.loads(p.read_text(encoding="utf-8")) for p in _specs()]


def check_spec_present(specs: list[Path]) -> list[str]:
    return [] if specs else ["docs/art holds no *.art.json spec"]


def check_artwork_matches_spec(specs: list[Path]) -> list[str]:
    bad = []
    for spec_path in specs:
        for path, text in RENDER.rendered(spec_path).items():
            if not path.exists():
                bad.append(f"{_rel(path)} was never rendered")
            elif path.read_text(encoding="utf-8") != text + "\n":
                bad.append(f"{_rel(path)} is stale; run python tools/render_repo_art.py")
    return bad


def check_render_is_deterministic(specs: list[Path]) -> list[str]:
    """The corona is random draws. Seeded ones, or this fails."""
    return [f"{p.name} renders differently every time"
            for p in specs if RENDER.rendered(p) != RENDER.rendered(p)]


def check_identity_per_repository(_unused: list[Path]) -> list[str]:
    """The identity claim, checked rather than asserted in a doc."""
    names = ["gather", "flywheel", "crucible", "index", "forum", "telos",
             "learn", "emet", "relay", "mneme", "plexus", "buildlang"]
    marks = {n: ART_LIB.header_svg(
        {"name": n, "role": "x", "tagline": "y", "words": ["z"]}) for n in names}
    bad = []
    if len({ART_LIB.seed_for(n) for n in names}) != len(names):
        bad.append("two repositories share a seed")
    bodies = {n: re.sub(r"[A-Z]{3,}", "", svg) for n, svg in marks.items()}
    if len(set(bodies.values())) != len(names):
        bad.append("two repositories drew alike")
    return bad


def check_seed_is_recorded(_unused: list[Path]) -> list[str]:
    """A generated mark carries the seed that made it."""
    svg = ART_LIB.header_svg(
        {"name": "buildlang", "role": "x", "tagline": "y", "words": []})
    stamp = f"SEED {ART_LIB.seed_for('buildlang') % 100000:05d}"
    return [] if stamp in svg else ["the mark does not record its own seed"]


def check_no_local_paths_or_em_dashes(_unused: list[Path]) -> list[str]:
    bad = []
    for path in sorted(ART.glob("*.svg")):
        text = path.read_text(encoding="utf-8")
        if EM_DASH in text:
            bad.append(f"{path.name} carries an em-dash")
        if re.search(r"[A-Z]:[\\/]", text):
            bad.append(f"{path.name} names a local path")
    return bad


def check_spec_words_reach_the_drawing(specs: list[Path]) -> list[str]:
    """Guards against a diagram that renders but silently drops content."""
    bad = []
    for spec_path in specs:
        spec = json.loads(spec_path.read_text(encoding="utf-8"))
        drawn = "".join(RENDER.rendered(spec_path).values())
        if spec["header"]["tagline"] not in drawn:
            bad.append(f"{spec_path.name}: the tagline never reaches the drawing")
        for flow in spec.get("flows", []):
            for stage in flow["stages"]:
                if stage["title"] not in drawn:
                    bad.append(f'{spec_path.name}: {stage["title"]} is missing')
    return bad


def check_note_survives_the_wrapper(_unused: list[Path]) -> list[str]:
    """Card notes wrap to three lines and the wrapper drops the rest, so an
    edited sentence can lose its ending in the drawing while reading fine
    in the spec."""
    return FLOW_GATE.notes_the_wrapper_cuts(_loaded())


def check_return_edge_stays_on_its_row(_unused: list[Path]) -> list[str]:
    """A backward edge is routed under the row it leaves, so a cross-row one
    would be drawn straight through whatever cards sit in between."""
    bad = []
    for spec in _loaded():
        for flow in spec.get("flows", []):
            for edge in flow.get("returns", []):
                if edge["from"] // FLOW.PER_ROW != edge["to"] // FLOW.PER_ROW:
                    bad.append(
                        f'return {edge["from"]}->{edge["to"]} crosses a row break')
    return bad


def check_every_illustration_is_shown(_unused: list[Path]) -> list[str]:
    """No orphans. An image nobody links to is an image nobody sees."""
    haystack = (ROOT / "README.md").read_text(encoding="utf-8")
    haystack += "".join(p.read_text(encoding="utf-8", errors="ignore")
                        for p in ROOT.glob("docs/**/*.md"))
    images = sorted(p for d in SHOWN_DIRS for p in (ROOT / d).glob("*")
                    if p.suffix.lower() in {".svg", ".png"})
    return [f"committed but never shown: {_rel(p)}"
            for p in images if _rel(p) not in haystack]


def check_tagline_stays_inside_its_rule(_unused: list[Path]) -> list[str]:
    """The tagline is one unwrapped line under a rule that ends at x=700. Past
    that it runs on toward the aperture and nothing about the render fails."""
    return FLOW_GATE.taglines_that_overrun(_loaded())


def check_outcome_fits_its_box(_unused: list[Path]) -> list[str]:
    """An outcome box is one unwrapped label over one unwrapped note, and
    neither is clipped, so an over-long note runs into the next box."""
    return FLOW_GATE.outcomes_that_overflow(_loaded())


def check_the_gate_can_fail(_unused: list[Path]) -> list[str]:
    """A gate that cannot fail is not a gate. Every check with a budget in it
    gets handed input it has to reject, and anything that passes is named."""
    return FLOW_GATE.control_failures() + CARD_GATE.control_failures()


CHECKS = [
    ("spec.present", check_spec_present),
    ("art.matches_spec", check_artwork_matches_spec),
    ("art.render_is_deterministic", check_render_is_deterministic),
    ("art.identity_per_repository", check_identity_per_repository),
    ("art.seed_is_recorded", check_seed_is_recorded),
    ("art.no_local_paths_or_em_dashes", check_no_local_paths_or_em_dashes),
    ("art.spec_words_reach_the_drawing", check_spec_words_reach_the_drawing),
    ("art.note_survives_the_wrapper", check_note_survives_the_wrapper),
    ("art.return_edge_stays_on_its_row", check_return_edge_stays_on_its_row),
    ("art.every_illustration_is_shown", check_every_illustration_is_shown),
    ("art.tagline_stays_inside_its_rule", check_tagline_stays_inside_its_rule),
    ("art.outcome_fits_its_box", check_outcome_fits_its_box),
] + CARD_GATE.checks() + [
    ("art.the_gate_can_fail", check_the_gate_can_fail),
]


def _outputs(specs: list[Path]) -> list[dict]:
    seen = []
    for spec_path in specs:
        for path in RENDER.rendered(spec_path):
            body = path.read_bytes() if path.exists() else b""
            seen.append({
                "file": _rel(path),
                "spec": _rel(spec_path),
                "bytes": len(body),
                "sha256": hashlib.sha256(body).hexdigest(),
            })
    return sorted(seen, key=lambda item: item["file"])


def receipt() -> dict:
    specs = _specs()
    results = [{"name": name, "passed": not failures, "failures": failures}
               for name, failures in ((n, f(specs)) for n, f in CHECKS)]
    return {
        "schema": SCHEMA,
        "mode": "check",
        "specs": [_rel(p) for p in specs],
        "outputs": _outputs(specs),
        "checks": results,
        "passed": all(item["passed"] for item in results),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true",
                        help="emit the run as a receipt instead of a summary")
    args = parser.parse_args(argv)
    report = receipt()
    if args.json:
        print(json.dumps(report, indent=2))
        return 0 if report["passed"] else 1
    for item in report["checks"]:
        print(f"{'ok  ' if item['passed'] else 'FAIL'} {item['name']}")
        for failure in item["failures"]:
            print(f"       {failure}")
    print(f"{len(report['outputs'])} files from "
          f"{len(report['specs'])} spec files")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
