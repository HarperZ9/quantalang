"""check_repo_flow.py -- the budget checks behind the lane diagram.

Every one of these guards the same failure: text that reads fine in the spec
and comes out clipped, overrun, or on top of its neighbour in the drawing.
Each takes the specs it judges as an argument, so the control below can hand
it input it has to reject.

Kept beside the art gates rather than inside them so neither file outgrows
what one person can hold at once.
"""
from __future__ import annotations

import repo_flow as FLOW

# The widest tagline that has been looked at on a rendered page. It counts
# characters rather than measuring glyphs, so it cannot tell "mmmm" from
# "iiii": a guardrail, not a typographic fact.
TAGLINE_BUDGET = 70


def notes_the_wrapper_cuts(specs: list[dict]) -> list[str]:
    bad = []
    for spec in specs:
        for flow in spec.get("flows", []):
            for stage in flow["stages"]:
                drawn = " ".join(FLOW._wrap(stage["note"]))
                if drawn != " ".join(stage["note"].split()):
                    bad.append(f'{stage["title"]}: the drawing cuts off at '
                               f'"{drawn}"')
    return bad


def taglines_that_overrun(specs: list[dict]) -> list[str]:
    bad = []
    for spec in specs:
        tagline = spec["header"]["tagline"]
        if len(tagline) > TAGLINE_BUDGET:
            bad.append(f"{len(tagline)} characters runs past the rule: "
                       f"{tagline!r}")
    return bad


def outcome_budgets(count: int) -> tuple[int, int]:
    """Label and note budgets for one box in a band of `count` boxes."""
    span = (FLOW.W - FLOW.PAD * 2 - FLOW.GAP * (count - 1)) / count
    usable = span - 14 - 10
    return int(usable / 7.0), int(usable / 5.4)


def outcomes_that_overflow(specs: list[dict]) -> list[str]:
    bad = []
    for spec in specs:
        for flow in spec.get("flows", []):
            label_budget, note_budget = outcome_budgets(len(flow["outcomes"]))
            for item in flow["outcomes"]:
                if len(item["label"]) > label_budget:
                    bad.append(f'{item["label"]!r} is wider than its box')
                if len(item["note"]) > note_budget:
                    bad.append(f'the note under {item["label"]} is wider than '
                               f'its box: {item["note"]!r}')
    return bad


# A spec built to break all three budgets at once. Every other check reports
# clean against it, which says the checks ran and not that they work.
CONTROL = [{
    "header": {"tagline": "x" * (TAGLINE_BUDGET + 1)},
    "flows": [{
        "stages": [{"title": "CARD", "note": "word " * 60}],
        "outcomes": [{"label": "OK", "note": "x" * 200},
                     {"label": "y" * 200, "note": "short"}],
    }],
}]


def control_failures() -> list[str]:
    """Feed each budget check input it has to reject, and say what got past."""
    return [f"the gate missed {what}" for caught, what in (
        (len(notes_the_wrapper_cuts(CONTROL)) == 1, "a truncated note"),
        (len(taglines_that_overrun(CONTROL)) == 1, "a tagline past its rule"),
        (len(outcomes_that_overflow(CONTROL)) == 2,
         "an over-wide label and an over-long note"),
    ) if not caught]
