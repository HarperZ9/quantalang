"""Render a repository's front-page artwork from its spec.

    python tools/render_repo_art.py            # write the SVGs
    python tools/render_repo_art.py --check    # fail if any is stale

The check mode is the point. Committed artwork drifts from the words it
illustrates the moment someone edits one and not the other, and nobody
notices, because a picture in a README is never diffed. Here the picture is
a pure function of a spec that IS diffable, so a test can re-render it and
compare the result against what is committed.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from repo_art import header_svg  # noqa: E402
from repo_card import card_svg  # noqa: E402
from repo_flow import flow_svg  # noqa: E402

ART = Path(__file__).resolve().parents[1] / "docs" / "art"


def rendered(spec_path: Path) -> dict[Path, str]:
    """Every file one spec produces, as path to text."""
    spec = json.loads(spec_path.read_text(encoding="utf-8"))
    stem = spec_path.name.removesuffix(".art.json")
    out = {spec_path.parent / f"{stem}-header.svg": header_svg(spec["header"])}
    for flow in spec.get("flows", []):
        out[spec_path.parent / flow["file"]] = flow_svg(flow)
    for card in spec.get("cards", []):
        out[spec_path.parent / card["file"]] = card_svg(card)
    return out


def specs() -> list[Path]:
    return sorted(ART.glob("*.art.json"))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true",
                        help="report stale artwork instead of rewriting it")
    args = parser.parse_args(argv)

    stale: list[str] = []
    for spec_path in specs():
        for path, text in rendered(spec_path).items():
            body = text + "\n"
            if args.check:
                current = path.read_text(encoding="utf-8") if path.exists() else ""
                if current != body:
                    stale.append(str(path.relative_to(ART.parents[1])))
                continue
            # newline="" so a Windows run writes the same bytes a Linux
            # run does. The whole point of this file is that committed
            # artwork and a fresh render are comparable.
            path.write_text(body, encoding="utf-8", newline="")
            print(f"wrote {path.relative_to(ART.parents[1])} ({len(body)} bytes)")

    if stale:
        print("stale artwork, re-run tools/render_repo_art.py:", file=sys.stderr)
        for name in stale:
            print(f"  {name}", file=sys.stderr)
        return 1
    if args.check:
        print(f"artwork matches its spec ({len(specs())} spec files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
