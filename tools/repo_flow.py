"""repo_flow.py -- a workflow diagram rendered from a spec, not hand-placed.

The picture a reader actually needs is what happens to one piece of work as it
moves through the tool: what it passes through, what can send it back, and what
it ends up as. This draws that from a list of stages, so the diagram is data in
the repository and stays correctable by editing a sentence rather than by
nudging coordinates in a drawing program.

Color says one thing here and nothing else. The forward path is the verified
green, the edge that sends work back is the drift iris, and everything that is
merely structure is a hairline. Both a light and a dark palette are defined and
the reader's own setting picks between them, so the diagram is legible in a
README either way without shipping two files.

Cards carry a 3px corner. A full round would read as a capsule, which is the
default shape of every generated interface and says nothing about what the
thing is; a small radius reads as drawn.
"""
from __future__ import annotations

from repo_art import GROTESK, MONO, _esc, _num

W = 960
PAD = 44
GAP = 26
CARD_H = 96
PER_ROW = 4
ROW_GAP = 96

# The two palettes, matching the tokens the repository's existing schematics
# already use so the whole set reads as one hand.
STYLE = """
  :root{ --void:#f4f3ef; --bone:#0b0c0e; --muted:#43474e;
    --hairline:rgba(11,12,14,.16); --card:rgba(255,255,255,.66);
    --verified:#1f7a52; --drift:#3a2bd6; }
  @media (prefers-color-scheme: dark){
    :root{ --void:#0b0e0f; --bone:#eef1ee; --muted:#9aa39c;
      --hairline:rgba(238,241,238,.18); --card:rgba(255,255,255,.05);
      --verified:#5fae93; --drift:#a99cf5; } }
  .bg{ fill:var(--void); }
  .card{ fill:var(--card); stroke:var(--hairline); stroke-width:1.4; }
  .n{ fill:var(--bone); font-size:15px; font-weight:650; }
  .s{ fill:var(--muted); font-size:11.5px; }
  .k{ fill:var(--muted); font-size:11px; letter-spacing:.16em; }
  .h{ fill:var(--bone); font-size:21px; font-weight:700; }
  .fwd{ stroke:var(--verified); stroke-width:2; fill:none; }
  .back{ stroke:var(--drift); stroke-width:1.8; fill:none; stroke-dasharray:5 4; }
  .thin{ stroke:var(--hairline); stroke-width:1.4; fill:none; }
  .step{ fill:var(--muted); font-size:11px; font-weight:700; letter-spacing:.1em; }
"""


def _wrap(text: str, width: int = 30) -> list[str]:
    lines: list[str] = []
    line = ""
    for word in text.split():
        candidate = f"{line} {word}".strip()
        if len(candidate) > width and line:
            lines.append(line)
            line = word
        else:
            line = candidate
    if line:
        lines.append(line)
    return lines[:3]


def _card_box(index: int) -> tuple[float, float, float]:
    """Left edge, top edge and width for the card at `index`."""
    width = (W - PAD * 2 - GAP * (PER_ROW - 1)) / PER_ROW
    row, col = divmod(index, PER_ROW)
    return (PAD + col * (width + GAP), 110 + row * (CARD_H + ROW_GAP), width)


def _card(index: int, stage: dict) -> str:
    x, y, w = _card_box(index)
    notes = "".join(
        f'<text class="s" x="{_num(x + 14)}" y="{_num(y + 58 + i * 15)}">'
        f"{_esc(line)}</text>"
        for i, line in enumerate(_wrap(stage.get("note", ""))))
    return (f'<g><rect class="card" x="{_num(x)}" y="{_num(y)}" '
            f'width="{_num(w)}" height="{CARD_H}" rx="3"/>'
            f'<text class="step" x="{_num(x + 14)}" y="{_num(y - 9)}">'
            f"{index + 1:02d}</text>"
            f'<text class="n" x="{_num(x + 14)}" y="{_num(y + 32)}">'
            f'{_esc(stage["title"])}</text>{notes}</g>')


def _forward(index: int) -> str:
    """The edge from card `index` to card `index + 1`."""
    x0, y0, w = _card_box(index)
    x1, y1, _ = _card_box(index + 1)
    mid0, mid1 = y0 + CARD_H / 2, y1 + CARD_H / 2
    if y0 == y1:
        return (f'<path class="fwd" marker-end="url(#af)" '
                f'd="M{_num(x0 + w + 4)} {_num(mid0)}H{_num(x1 - 7)}"/>')
    # The wrap between rows, routed through the gutter so it never crosses a
    # card: out to the right margin, back across the empty band, then in.
    gut = _num(y0 + CARD_H + ROW_GAP - 26)
    return (f'<path class="fwd" marker-end="url(#af)" '
            f'd="M{_num(x0 + w + 4)} {_num(mid0)}H{W - 20}V{gut}'
            f'H20V{_num(mid1)}H{_num(x1 - 7)}"/>')


def _return(edge: dict) -> str:
    """A dashed edge that sends work back, dipping below the row it leaves."""
    x0, y0, w0 = _card_box(edge["from"])
    x1, y1, w1 = _card_box(edge["to"])
    dip = y0 + CARD_H + 22
    label_x = (x0 + w0 / 2 + x1 + w1 / 2) / 2
    return (f'<path class="back" marker-end="url(#ab)" '
            f'd="M{_num(x0 + w0 / 2)} {_num(y0 + CARD_H)}V{_num(dip)}'
            f'H{_num(x1 + w1 / 2)}V{_num(y1 + CARD_H + 7)}"/>'
            f'<text class="k" x="{_num(label_x)}" y="{_num(dip + 17)}" '
            f'text-anchor="middle" fill="var(--drift)">'
            f'{_esc(edge["label"])}</text>')


def _outcomes(items: list[dict], top: float, source: int) -> str:
    x0, y0, w0 = _card_box(source)
    span = (W - PAD * 2 - GAP * (len(items) - 1)) / len(items)
    trunk = x0 + w0 / 2
    tone = {"verified": "var(--verified)", "drift": "var(--drift)",
            "none": "var(--muted)"}
    out = [f'<path class="thin" d="M{_num(trunk)} {_num(y0 + CARD_H)}'
           f'V{_num(top - 24)}"/>']
    for i, item in enumerate(items):
        x = PAD + i * (span + GAP)
        out.append(
            f'<path class="thin" d="M{_num(trunk)} {_num(top - 24)}'
            f'H{_num(x + span / 2)}V{_num(top)}"/>'
            f'<rect x="{_num(x)}" y="{_num(top)}" width="{_num(span)}" '
            f'height="46" rx="3" fill="none" stroke="{tone[item["tone"]]}" '
            f'stroke-width="1.6"/>'
            f'<text class="n" x="{_num(x + 14)}" y="{_num(top + 21)}" '
            f'font-size="13">{_esc(item["label"])}</text>'
            f'<text class="s" x="{_num(x + 14)}" y="{_num(top + 37)}" '
            f'font-size="10.5">{_esc(item["note"])}</text>')
    return "".join(out)


def flow_svg(spec: dict) -> str:
    """Stages, the edges between them, and what the work ends up as."""
    stages = spec["stages"]
    rows = (len(stages) + PER_ROW - 1) // PER_ROW
    body = 110 + rows * CARD_H + (rows - 1) * ROW_GAP
    top = body + 74
    height = top + 46 + 46
    cards = "".join(_card(i, s) for i, s in enumerate(stages))
    edges = "".join(_forward(i) for i in range(len(stages) - 1))
    backs = "".join(_return(e) for e in spec.get("returns", []))
    ends = _outcomes(spec["outcomes"], top, len(stages) - 1)
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {_num(height)}" '
        f'width="{W}" height="{_num(height)}" font-family="{GROTESK}" role="img" '
        f'aria-label="{_esc(spec["alt"])}">'
        f"<style>{STYLE}</style>"
        '<marker id="af" markerWidth="9" markerHeight="9" refX="7" refY="3" '
        'orient="auto"><path d="M0,0 L7,3 L0,6 Z" fill="var(--verified)"/></marker>'
        '<marker id="ab" markerWidth="9" markerHeight="9" refX="7" refY="3" '
        'orient="auto"><path d="M0,0 L7,3 L0,6 Z" fill="var(--drift)"/></marker>'
        f'<rect class="bg" width="{W}" height="{_num(height)}"/>'
        f'<text class="k" x="{PAD}" y="40" font-family="{MONO}">'
        f'{_esc(spec["kicker"].upper())}</text>'
        f'<text class="h" x="{PAD}" y="72">{_esc(spec["title"])}</text>'
        f"{edges}{backs}{cards}{ends}"
        f'<text class="s" x="{PAD}" y="{_num(height - 18)}">'
        f'{_esc(spec["footnote"])}</text>'
        "</svg>")
