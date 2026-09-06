"""repo_art.py -- deterministic SVG artwork for a repository's front page.

Two renderers, one design language, no binary blobs.

`header_svg` draws the identity card: a seeded aperture on pure black, with the
repository's name, what it does, and the words it works in. The aperture is the
one form the whole visual corpus returns to, a luminous core with fine radial
line-work resolving around it. Every repository gets the SAME form and a
DIFFERENT drawing of it, because the corona, the halftone screen and the hue
are all derived from a hash of the repository's own name. That is where the
sense of identity comes from: sibling projects, not a template applied twice.

`flow_svg` draws a workflow: cards on a calm ground, hairline connectors, and
color used only to say what a path means. It reads the same in a light or a
dark reader because it defines both and lets the reader's own setting pick.

The split between the two is deliberate and is the figure-ground rule made
concrete. Generative energy belongs where the art is the subject, so it is
contained inside the header. Where words have to be read it recedes to a
hairline, so the diagrams carry no texture at all.

Both renderers are pure functions of their spec: same spec, same bytes. That is
what lets a test re-render the committed art and fail on drift, rather than
trusting that whoever last touched the file also regenerated it.
"""
from __future__ import annotations

import colorsys
import hashlib
import random

# Pure black, because the corpus grounds on pure black rather than a soft
# near-black. The two inks are the warm bone the rest of the ecosystem uses.
VOID = "#07080A"
BONE = "#F2F4F1"
SOFT = "#8E9AA0"

# The hues a corona is allowed to take, drawn from the electric-neon-on-black
# and luminous-warm-core poles of the inspiration corpus. Angles in degrees.
HUES = (188, 168, 96, 44, 22, 286, 322)

GROTESK = "Hanken Grotesk, Segoe UI, ui-sans-serif, system-ui, sans-serif"
MONO = "Conso, ui-monospace, Cascadia Mono, Consolas, monospace"


def _num(value: float) -> str:
    """Two decimal places, no trailing noise. Byte-stability starts here."""
    return f"{value:.2f}".rstrip("0").rstrip(".") or "0"


def _esc(text: str) -> str:
    return (str(text).replace("&", "&amp;").replace("<", "&lt;")
            .replace(">", "&gt;").replace('"', "&quot;"))


def seed_for(name: str) -> int:
    """A stable seed from the repository's own name.

    sha256 rather than hash(): CPython randomises str hashing per process, so
    the built-in would give a different drawing on every run.
    """
    return int(hashlib.sha256(name.encode("utf-8")).hexdigest()[:8], 16)


def _hue_pair(rng: random.Random) -> tuple[str, str]:
    """The corona hue and its cooler companion, as hex."""
    # Anchor on one of the corpus hues, then drift up to 14 degrees off it.
    # Seven fixed hues collide across a dozen repositories; the drift keeps
    # two siblings that land on the same anchor from reading as one project.
    hue = (HUES[rng.randrange(len(HUES))] + rng.uniform(-14, 14)) % 360
    warm = colorsys.hls_to_rgb(hue / 360.0, 0.62, 0.92)
    cool = colorsys.hls_to_rgb(((hue + 34) % 360) / 360.0, 0.44, 0.70)
    return ("#%02X%02X%02X" % tuple(round(c * 255) for c in warm),
            "#%02X%02X%02X" % tuple(round(c * 255) for c in cool))


def _spokes(cx: float, cy: float, rng: random.Random, count: int = 300) -> str:
    """The corona, as a field of short radial dashes rather than long spokes.

    Continuous spokes read as a star. Broken ones read as line-work: the eye
    resolves a texture instead of counting rays, and the corona can be dense
    without becoming loud. Each angle walks outward emitting a dash, then a
    gap, until it runs out of reach, and the reach is set by a two-wave field
    so the ring thickens and thins instead of sitting perfectly even.

    Bucketed into five opacity groups, brightest nearest the aperture, so the
    whole corona is five path elements rather than a thousand.
    """
    import math

    buckets: list[list[str]] = [[] for _ in range(5)]
    for i in range(count):
        angle = (i / count) * math.tau + rng.uniform(-0.008, 0.008)
        field = (math.sin(angle * 3 + rng.random() * 0.3) * 0.22
                 + math.sin(angle * 7) * 0.13 + 0.68)
        cos_a, sin_a = math.cos(angle), math.sin(angle)
        limit = 92 + field * 52
        radius = 76 + rng.uniform(0, 9)
        while radius < limit:
            far = min(limit, radius + rng.uniform(3.0, 15.0))
            level = min(4, max(0, int((1.0 - (radius - 76) / 78.0) * 4.6)))
            buckets[level].append(
                f"M{_num(cx + cos_a * radius)} {_num(cy + sin_a * radius)}"
                f"L{_num(cx + cos_a * far)} {_num(cy + sin_a * far)}")
            radius = far + rng.uniform(2.5, 13.0)
    out = []
    for level, segs in enumerate(buckets):
        if not segs:
            continue
        out.append(f'<path d="{"".join(segs)}" stroke="url(#ray)" '
                   f'stroke-width="{_num(0.5 + level * 0.14)}" '
                   f'opacity="{_num(0.13 + level * 0.14)}"/>')
    return "".join(out)


def _rings(cx: float, cy: float, rng: random.Random) -> str:
    """Concentric broken arcs. This is the mesh half of the corona.

    Each ring is one circle with a seeded dash pattern, so a whole band of
    fine arc-work costs one element and varies per repository.
    """
    out = []
    for ring in range(14):
        radius = 80 + ring * 4.6
        fall = 1.0 - (ring / 14.0) ** 1.5
        dash = rng.uniform(2.2, 13.0)
        gap = dash * rng.uniform(0.7, 3.4)
        out.append(f'<circle cx="{_num(cx)}" cy="{_num(cy)}" r="{_num(radius)}" '
                   f'fill="none" stroke="url(#ray)" stroke-width="0.55" '
                   f'stroke-dasharray="{_num(dash)} {_num(gap)}" '
                   f'stroke-dashoffset="{_num(rng.uniform(0, 40))}" '
                   f'opacity="{_num(0.08 + fall * 0.3)}"/>')
    return "".join(out)


def _blades(cx: float, cy: float, rng: random.Random, accent: str) -> str:
    """A dot screen in a narrow band around the core, the way a halftone
    plate carries the shoulder of a highlight."""
    import math

    dots = []
    for ring in range(1, 7):
        radius = 41 + ring * 5.1
        n = max(10, int(radius * 0.85))
        for k in range(n):
            angle = (k / n) * math.tau + ring * 0.27
            fall = max(0.0, 1.0 - (ring / 7.0) ** 1.2)
            if rng.random() > fall * 0.86 + 0.1:
                continue
            dots.append(f'<circle cx="{_num(cx + math.cos(angle) * radius)}" '
                        f'cy="{_num(cy + math.sin(angle) * radius)}" '
                        f'r="{_num(0.45 + fall * 1.15)}" '
                        f'opacity="{_num(0.12 + fall * 0.5)}"/>')
    return f'<g fill="{accent}">{"".join(dots)}</g>'


def _core(cx: float, cy: float, cool: str) -> str:
    """The aperture itself, and the one spectral flare.

    The middle is a void with an incandescent rim, not a bright ball. That
    is the form the whole visual corpus keeps returning to, and it is also
    the more honest picture: what the tool gives you is an opening onto
    something, with the light at its edge.

    The flare is an anamorphic streak, white where the light is hottest and
    splitting to red at one end and blue at the other, which is what a lens
    does at the edge of a bright source. It is the single hot mark the art
    is allowed and the only place the full spectrum appears.
    """
    ticks = "".join(
        f'<path d="M{_num(cx + dx * 150)} {_num(cy + dy * 150)}'
        f'L{_num(cx + dx * 160)} {_num(cy + dy * 160)}"/>'
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
    return (
        f'<circle cx="{_num(cx)}" cy="{_num(cy)}" r="132" fill="url(#halo)"/>'
        f'<circle cx="{_num(cx)}" cy="{_num(cy)}" r="36" fill="url(#pupil)"/>'
        f'<circle cx="{_num(cx)}" cy="{_num(cy)}" r="36" fill="none" '
        f'stroke="url(#ray)" stroke-width="6" opacity="0.32"/>'
        f'<circle cx="{_num(cx)}" cy="{_num(cy)}" r="36" fill="none" '
        f'stroke="#FFFFFF" stroke-width="1.5" opacity="0.9"/>'
        f'<circle cx="{_num(cx)}" cy="{_num(cy)}" r="14" fill="none" '
        f'stroke="{cool}" stroke-width="0.7" opacity="0.55"/>'
        f'<rect x="{_num(cx - 168)}" y="{_num(cy - 1.2)}" width="336" '
        f'height="2.4" fill="url(#flare)"/>'
        f'<rect x="{_num(cx - 0.8)}" y="{_num(cy - 88)}" width="1.6" '
        f'height="176" fill="url(#flare)" opacity="0.38"/>'
        # The reticle: a measured circle and four ticks, so the aperture
        # reads as something being observed rather than admired.
        f'<g stroke="{BONE}" stroke-width="0.7" opacity="0.32" fill="none">'
        f'<circle cx="{_num(cx)}" cy="{_num(cy)}" r="150" opacity="0.4"/>'
        f"{ticks}</g>")


def _defs(accent: str, cool: str, seed: int) -> str:
    """Gradients, the scanline screen, the grain, and the wash that keeps
    text legible.

    feTurbulence carries an explicit seed for the same reason the corona
    does: an unseeded filter is a different image on every render, and a
    test that compares bytes would never pass twice.
    """
    return (
        "<defs>"
        f'<radialGradient id="halo"><stop offset="0" stop-color="{accent}" '
        f'stop-opacity="0.34"/><stop offset="0.4" stop-color="{accent}" '
        f'stop-opacity="0.08"/><stop offset="1" stop-color="{VOID}" '
        'stop-opacity="0"/></radialGradient>'
        '<radialGradient id="core"><stop offset="0" stop-color="#FFFFFF"/>'
        '<stop offset="0.36" stop-color="#FFFFFF" stop-opacity="0.92"/>'
        f'<stop offset="0.74" stop-color="{accent}" stop-opacity="0.6"/>'
        f'<stop offset="1" stop-color="{accent}" stop-opacity="0"/></radialGradient>'
        f'<radialGradient id="pupil"><stop offset="0" stop-color="{VOID}"/>'
        f'<stop offset="0.62" stop-color="{VOID}"/>'
        f'<stop offset="1" stop-color="{accent}" stop-opacity="0.3"/>'
        '</radialGradient>'
        '<linearGradient id="ray" x1="0" y1="0" x2="1" y2="1">'
        f'<stop offset="0" stop-color="{accent}"/>'
        f'<stop offset="1" stop-color="{cool}"/></linearGradient>'
        # The refraction: red at one end, white where it is hottest, blue at
        # the other. One mark, and the only full spectrum in the kit.
        '<linearGradient id="flare" x1="0" y1="0" x2="1" y2="0">'
        '<stop offset="0" stop-color="#FF3B57" stop-opacity="0"/>'
        '<stop offset="0.22" stop-color="#FF3B57" stop-opacity="0.42"/>'
        '<stop offset="0.5" stop-color="#FFFFFF" stop-opacity="0.9"/>'
        '<stop offset="0.78" stop-color="#3B7BFF" stop-opacity="0.42"/>'
        '<stop offset="1" stop-color="#3B7BFF" stop-opacity="0"/></linearGradient>'
        '<linearGradient id="veil" x1="0" y1="0" x2="1" y2="0">'
        f'<stop offset="0" stop-color="{VOID}" stop-opacity="0.96"/>'
        f'<stop offset="0.4" stop-color="{VOID}" stop-opacity="0.86"/>'
        f'<stop offset="0.64" stop-color="{VOID}" stop-opacity="0"/></linearGradient>'
        '<pattern id="scan" width="4" height="3" patternUnits="userSpaceOnUse">'
        f'<rect width="4" height="1" fill="{VOID}"/></pattern>'
        '<filter id="grain" x="0" y="0" width="100%" height="100%">'
        '<feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="3" '
        f'seed="{seed % 9973}" result="n"/>'
        '<feColorMatrix in="n" type="saturate" values="0"/></filter>'
        "</defs>")


def header_svg(spec: dict) -> str:
    """The identity card. 1280x340, the proportion a README header wants."""
    name = spec["name"]
    seed = seed_for(name)
    rng = random.Random(seed)
    accent, cool = _hue_pair(rng)
    cx, cy = 1012.0, 168.0
    words = "  /  ".join(w.upper() for w in spec.get("words", []))
    meta = f'{spec.get("publisher", "ZENTROPY LABS")}  /  {spec["role"].upper()}'
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1280 340" '
        f'width="1280" height="340" role="img" '
        f'aria-label="{_esc(name)}: {_esc(spec["tagline"])}">'
        f"{_defs(accent, cool, seed)}"
        f'<rect width="1280" height="340" fill="{VOID}"/>'
        f'<g>{_rings(cx, cy, rng)}{_spokes(cx, cy, rng)}'
        f'{_blades(cx, cy, rng, accent)}{_core(cx, cy, cool)}</g>'
        # Scanline screen, then grain. Both are whispers: they sit the art
        # behind glass instead of decorating it.
        f'<rect width="1280" height="340" fill="url(#scan)" opacity="0.22"/>'
        f'<rect width="1280" height="340" fill="url(#veil)"/>'
        f'<rect width="1280" height="340" filter="url(#grain)" opacity="0.05"/>'
        f'<circle cx="70" cy="70" r="4" fill="{accent}"/>'
        f'<text x="88" y="75" font-family="{MONO}" font-size="13" '
        f'letter-spacing="2.6" fill="{SOFT}">{_esc(meta)}</text>'
        f'<text x="64" y="188" font-family="{GROTESK}" font-size="78" '
        f'font-weight="800" letter-spacing="1" fill="{BONE}">'
        f"{_esc(name.upper())}</text>"
        f'<text x="66" y="232" font-family="{GROTESK}" font-size="21" '
        f'fill="#C6CFD3">{_esc(spec["tagline"])}</text>'
        f'<path d="M64 268H700" stroke="{BONE}" stroke-width="0.8" opacity="0.2"/>'
        f'<text x="66" y="297" font-family="{MONO}" font-size="12.5" '
        f'letter-spacing="2.2" fill="{SOFT}">{_esc(words)}</text>'
        f'<text x="1216" y="297" font-family="{MONO}" font-size="12" '
        f'text-anchor="end" letter-spacing="1.4" fill="{SOFT}" '
        f'opacity="0.75">SEED {seed % 100000:05d}</text>'
        "</svg>")
