"""repo_card.py -- the artifact a tool hands back, drawn field by field.

Every command here writes a receipt, and until now the README said so in a
sentence. A reader deciding whether to trust the tool wants to see the thing:
what fields come back, which one carries the verdict, and how they would check
each field themselves. This draws that from a spec, so the picture is data in
the repository and a gate can hold it against a receipt the tool actually
emits.

Color still says one thing. Exactly one row carries the verdict and takes the
verified green; a row that reports drift takes the drift iris. Every other row
is ink and a hairline, because a field is structure and structure is not news.

The value column shows a literal only where the literal is stable. A hash or a
byte count changes with the checkout, so those rows carry the shape of the
value instead: how many entries, how many keys. A picture that shows a hash is
a picture that is wrong by the next commit.

Widths come from face-metrics.json, which holds advances measured off the font
files themselves. An earlier version of this module guessed a width from a
character's class, gave every lowercase letter one number, and handed that same
guess to the check meant to catch text running off the page. The two agreed
with each other and four drawings shipped past the rule anyway. The file next
door is the second opinion. It is generated from the faces, and the check reads
it rather than asking this module how wide its own text is.
"""
from __future__ import annotations

import json
from pathlib import Path

from repo_art import GROTESK, MONO, _esc, _num

W = 960
PAD = 44
TOP = 142
KEY_W = 186
VAL_W = 258
GUTTER = 26
NOTE_X = PAD + KEY_W + GUTTER + VAL_W + GUTTER
NOTE_W = W - PAD - NOTE_X

# The same two palettes the schematics use, so the whole set reads as one hand.
STYLE = """
  :root{ --void:#f4f3ef; --bone:#0b0c0e; --muted:#43474e;
    --hairline:rgba(11,12,14,.16); --card:rgba(255,255,255,.66);
    --verified:#1f7a52; --drift:#3a2bd6; }
  @media (prefers-color-scheme: dark){
    :root{ --void:#0b0e0f; --bone:#eef1ee; --muted:#9aa39c;
      --hairline:rgba(238,241,238,.18); --card:rgba(255,255,255,.05);
      --verified:#5fae93; --drift:#a99cf5; } }
  .bg{ fill:var(--void); }
  .row{ fill:var(--card); stroke:var(--hairline); stroke-width:1.2; }
  .key{ fill:var(--bone); font-size:13px; font-weight:650; }
  .val{ fill:var(--muted); font-size:12px; }
  .s{ fill:var(--muted); font-size:11.5px; }
  .k{ fill:var(--muted); font-size:11px; letter-spacing:.16em; }
  .h{ fill:var(--bone); font-size:21px; font-weight:700; }
  .thin{ stroke:var(--hairline); stroke-width:1.2; fill:none; }
"""

TONE = {"verified": "var(--verified)", "drift": "var(--drift)",
        "none": "var(--hairline)"}

# The column heads say what a row of this drawing is. A receipt reads as a
# field and what comes back in it; something else in the repository reads as
# something else, so a spec may name its own three and these are the default.
HEADS = ("field", "what comes back", "how you check it")

METRICS_FILE = Path(__file__).with_name("face-metrics.json")
_METRICS = json.loads(METRICS_FILE.read_text(encoding="utf-8"))
FIRST, LAST = _METRICS["range"]


def _widest(group: str, weight: str) -> tuple[float, ...]:
    """Per character, the widest advance any measured face gives it, in em.

    A reader's machine resolves one face out of the stack and we do not get to
    know which. Taking the maximum means the drawing fits whichever one it
    lands in, at the cost of a line that sometimes stops short of the rule.
    """
    faces = list(_METRICS[group][weight].values())
    return tuple(max(face[i] for face in faces) / 1000.0
                 for i in range(LAST - FIRST + 1))


SANS = _widest("sans", "regular")
SANS_BOLD = _widest("sans", "bold")
MONO_REG = _widest("mono", "regular")
MONO_BOLD = _widest("mono", "bold")
_OFF_TABLE = max(max(SANS), max(SANS_BOLD), max(MONO_REG), max(MONO_BOLD))

# Every text element on the card: the table that draws it, its size in pixels,
# and its letter-spacing in em. The names follow what the element is rather
# than its CSS class, because two of them override the family on the element
# itself and the class no longer tells you the face.
ROLE = {
    "title": (SANS_BOLD, 21.0, 0.0),
    "note": (SANS, 11.5, 0.0),
    "foot": (SANS, 11.5, 0.0),
    "kicker": (MONO_REG, 11.0, 0.16),
    "head": (MONO_REG, 11.0, 0.16),
    "source": (MONO_REG, 11.5, 0.0),
    "key": (MONO_BOLD, 13.0, 0.0),
    "value": (MONO_REG, 12.0, 0.0),
}


def element_width(text: str, role: str) -> float:
    """What one element draws, in pixels, in the widest face it can land in."""
    table, size, tracking = ROLE[role]
    total = sum(table[ord(c) - FIRST] if FIRST <= ord(c) <= LAST else _OFF_TABLE
                for c in text)
    return total * size + tracking * size * len(text)


def text_width(text: str) -> float:
    """What one line of note or footnote prose draws, in pixels."""
    return element_width(text, "note")


# One line of the note column, in pixels, and the lines a row has room for.
NOTE_BUDGET = NOTE_W
NOTE_LINES = 3

# The footnote runs the width of the page at the same size, so it holds more.
FOOT_BUDGET = W - PAD * 2
FOOT_LINES = 4

LEADING = 15
FOOT_LEADING = 16
# A row holding two note lines. A third line adds one line of leading to it.
ROW_H = 46


def _wrap(text: str, width: float = NOTE_BUDGET,
          limit: int = NOTE_LINES) -> list[str]:
    """Greedy wrap by drawn width, cut to the lines the caller has room for.

    A word wider than the whole budget is left alone on its line and draws past
    the edge. Wrapping cannot fix that, so the check reads the width of every
    line it gets back rather than only whether text was cut.
    """
    lines: list[str] = []
    line = ""
    for word in text.split():
        candidate = f"{line} {word}".strip()
        if text_width(candidate) > width and line:
            lines.append(line)
            line = word
        else:
            line = candidate
    if line:
        lines.append(line)
    return lines[:limit]


def row_height(fields: list[dict]) -> float:
    """How tall every row on this card is: what its longest note needs."""
    lines = max(len(_wrap(field["note"])) for field in fields)
    return ROW_H + LEADING * max(0, lines - 2)


def _row_y(index: int, row_h: float = ROW_H) -> float:
    return TOP + index * row_h


def _row(index: int, field: dict, row_h: float = ROW_H) -> str:
    """One field: its name, what comes back in it, and how to check it."""
    y = _row_y(index, row_h)
    tone = TONE[field.get("tone", "none")]
    accent = field.get("tone", "none") != "none"
    notes = "".join(
        f'<text class="s" x="{_num(NOTE_X)}" y="{_num(y + 20 + i * LEADING)}">'
        f"{_esc(line)}</text>"
        for i, line in enumerate(_wrap(field["note"])))
    rule = (f'<rect x="{_num(PAD)}" y="{_num(y)}" width="3" '
            f'height="{_num(row_h - 8)}" fill="{tone}"/>') if accent else ""
    return (f'<g><rect class="row" x="{_num(PAD)}" y="{_num(y)}" '
            f'width="{W - PAD * 2}" height="{_num(row_h - 8)}" rx="3"/>{rule}'
            f'<text class="key" x="{_num(PAD + 16)}" y="{_num(y + 24)}" '
            f'font-family="{MONO}">{_esc(field["key"])}</text>'
            f'<text class="val" x="{_num(PAD + KEY_W + GUTTER)}" '
            f'y="{_num(y + 24)}" font-family="{MONO}"'
            f'{f" style={chr(34)}fill:{tone}{chr(34)}" if accent else ""}>'
            f'{_esc(field["value"])}</text>{notes}</g>')


def _column_heads(labels: tuple[str, str, str] = HEADS) -> str:
    columns = (PAD + 16, PAD + KEY_W + GUTTER, NOTE_X)
    return "".join(
        f'<text class="k" x="{_num(x)}" y="{_num(TOP - 14)}" '
        f'font-family="{MONO}">{_esc(label.upper())}</text>'
        for label, x in zip(labels, columns))


def _footnote(text: str, top: float) -> str:
    return "".join(
        f'<text class="s" x="{PAD}" y="{_num(top + i * FOOT_LEADING)}">'
        f"{_esc(line)}</text>"
        for i, line in enumerate(_wrap(text, FOOT_BUDGET, FOOT_LINES)))


def card_svg(spec: dict) -> str:
    """A receipt drawn field by field, with the source that produced it."""
    fields = spec["fields"]
    row_h = row_height(fields)
    foot = _wrap(spec["footnote"], FOOT_BUDGET, FOOT_LINES)
    rule = _row_y(len(fields), row_h) + 12
    height = rule + 22 + len(foot) * FOOT_LEADING
    rows = "".join(_row(i, f, row_h) for i, f in enumerate(fields))
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {_num(height)}" '
        f'width="{W}" height="{_num(height)}" font-family="{GROTESK}" role="img" '
        f'aria-label="{_esc(spec["alt"])}">'
        f"<style>{STYLE}</style>"
        f'<rect class="bg" width="{W}" height="{_num(height)}"/>'
        f'<text class="k" x="{PAD}" y="40" font-family="{MONO}">'
        f'{_esc(spec["kicker"].upper())}</text>'
        f'<text class="h" x="{PAD}" y="72">{_esc(spec["title"])}</text>'
        f'<text class="s" x="{PAD}" y="94" font-family="{MONO}" '
        f'font-size="11.5">$ {_esc(spec["source"])}</text>'
        f'{_column_heads(tuple(spec.get("heads", HEADS)))}{rows}'
        f'<path class="thin" d="M{PAD} {_num(rule)}H{W - PAD}"/>'
        f'{_footnote(spec["footnote"], rule + 22)}'
        "</svg>")
