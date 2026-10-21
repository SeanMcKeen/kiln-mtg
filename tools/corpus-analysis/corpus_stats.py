#!/usr/bin/env python3
"""Oracle text corpus statistics for `docs/05 Card Definitions/Oracle Text Corpus Analysis.md`.

Usage:
    python3 -I tools/corpus-analysis/corpus_stats.py docs/_external/oracle-cards.jsonl.gz [--json]

Reads a Scryfall `oracle_cards` bulk file (JSONL or JSON array, optionally gzipped).
Standard library only. The input is downloaded data: run with `-I` (isolated mode).
"""

import argparse
import collections
import gzip
import json
import re
import sys

NON_DECK_LAYOUTS = {"token", "double_faced_token", "emblem", "art_series", "planar", "scheme", "vanguard"}
FORMATS = ["commander", "legacy", "modern", "pauper", "standard"]
TOP_N = [10, 100, 500, 1000, 5000, 10000, 20000]
REMINDER = re.compile(r"\([^()]*\)")


def read_cards(path):
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rt", encoding="utf-8") as f:
        head = f.read(1)
        while head and head.isspace():
            head = f.read(1)
        if head == "[":
            yield from json.loads(head + f.read())
            return
        first = head + f.readline()
        for line in [first] + list(f):
            line = line.strip().rstrip(",")
            if line:
                yield json.loads(line)


def faces(card):
    return card.get("card_faces") or [card]


SHORT_NAMES = False


def ability_lines(card):
    """Per face: split on newlines, replace the card's own name with ~, strip reminder text.

    With --short-names, a legendary short name ("Kinnan" for "Kinnan, Bonder Prodigy")
    is replaced too, as `mtg-data` does for its `self_ref_text` field.
    """
    out = []
    for face in faces(card):
        text = face.get("oracle_text") or ""
        names = sorted({card["name"], face.get("name", card["name"])}, key=len, reverse=True)
        for name in names:
            text = text.replace(name, "~")
        if SHORT_NAMES:
            for name in names:
                short = name.split(", ", 1)[0]
                if short != name and len(short) > 2:
                    text = re.sub(r"(?<![\w])" + re.escape(short) + r"(?![\w])", "~", text)
        for line in text.split("\n"):
            line = REMINDER.sub("", line)
            line = " ".join(line.split())
            if line:
                out.append(line)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("bulk", help="Scryfall oracle_cards file")
    ap.add_argument("--json", action="store_true", help="print JSON instead of a table")
    ap.add_argument("--short-names", action="store_true", help="also replace legendary short names with ~")
    args = ap.parse_args()
    global SHORT_NAMES
    SHORT_NAMES = args.short_names

    cards = list(read_cards(args.bulk))
    layouts = collections.Counter(c["layout"] for c in cards)
    deck = [c for c in cards if c["layout"] not in NON_DECK_LAYOUTS]
    legal = {f: [c for c in deck if c.get("legalities", {}).get(f) == "legal"] for f in FORMATS}
    commander = legal["commander"]
    vanilla = [c for c in commander if not any((fc.get("oracle_text") or "").strip() for fc in faces(c))]

    lines = [line for c in commander for line in ability_lines(c)]
    counts = collections.Counter(lines)
    # Distinct lines appearing on exactly one card (not just once overall).
    cards_per_line = collections.Counter()
    for c in commander:
        for line in set(ability_lines(c)):
            cards_per_line[line] += 1
    unique_to_one_card = sum(1 for n in cards_per_line.values() if n == 1)
    ranked = [n for _, n in counts.most_common()]
    total = len(lines)
    coverage = {n: round(100 * sum(ranked[:n]) / total, 1) for n in TOP_N}

    result = {
        "objects": len(cards),
        "excluding_non_deck_layouts": len(deck),
        "legal": {f: len(v) for f, v in legal.items()},
        "vanilla_commander_legal": len(vanilla),
        "layouts": dict(layouts.most_common()),
        "ability_lines": {
            "total": total,
            "distinct": len(counts),
            "distinct_on_exactly_one_card": unique_to_one_card,
            "distinct_on_exactly_one_card_pct": round(100 * unique_to_one_card / len(counts)),
            "coverage_by_top_n_pct": coverage,
            "most_common": counts.most_common(20),
        },
    }
    if args.json:
        json.dump(result, sys.stdout, indent=2, ensure_ascii=False)
        print()
        return
    print(f"Oracle objects: {result['objects']:,}")
    print(f"Excluding tokens/emblems/art series/planes/schemes/vanguards: {result['excluding_non_deck_layouts']:,}")
    for f, n in result["legal"].items():
        print(f"{f.capitalize()}-legal: {n:,}")
    print(f"Vanilla (no rules text), Commander-legal: {result['vanilla_commander_legal']:,}")
    print("Layouts: " + " · ".join(f"{k} {v:,}" for k, v in layouts.most_common()))
    al = result["ability_lines"]
    print(f"Ability lines: {al['total']:,} total, {al['distinct']:,} distinct, "
          f"{al['distinct_on_exactly_one_card']:,} on exactly one card ({al['distinct_on_exactly_one_card_pct']}%)")
    print("Coverage by top-N distinct lines: " + ", ".join(f"top {n:,}: {p}%" for n, p in coverage.items()))
    print("Most common: " + ", ".join(f"{line!r} ({n:,})" for line, n in al["most_common"][:12]))


if __name__ == "__main__":
    main()
