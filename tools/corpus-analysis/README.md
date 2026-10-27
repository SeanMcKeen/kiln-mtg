# corpus-analysis

Reproduces the figures in `docs/05 Card Definitions/Oracle Text Corpus Analysis.md` from a Scryfall `oracle_cards` bulk file. Standard-library Python only.

```bash
# The bulk file is downloaded data, so run Python in isolated mode (-I).
python3 -I tools/corpus-analysis/corpus_stats.py docs/_external/oracle-cards.jsonl.gz
python3 -I tools/corpus-analysis/corpus_stats.py docs/_external/oracle-cards.jsonl.gz --short-names --json
```

The input can be JSONL or a JSON array, gzipped or not. `mtg data fetch` saves the file as `data/cache/oracle_cards.jsonl`.

## What it reports
- Pool sizes: all objects, objects without tokens/emblems/art series/planes/schemes/vanguards, and the legal count per format.
- Layout counts.
- Ability-line reuse over Commander-legal cards. Text is split per face on newlines, the card's own name is replaced with `~`, and reminder text is stripped. The output covers distinct lines, lines unique to one card, top-N coverage, and the most common lines.

## Reconciliation with the vault note (2026-10-20 data)
The M0 analysis was run ad hoc, and this script replaces those scripts.

- **Pool sizes and layouts match exactly.**
- **Ability lines:** the default run gives 36,247 distinct lines, against 36,037 in the vault. With `--short-names`, which also replaces a legendary card's short name ("Kinnan" for "Kinnan, Bonder Prodigy"), the run gives 36,032 distinct lines and 32,119 lines unique to one card. The vault has 36,037 and 32,115. Every top-N coverage figure matches within 0.1 percentage points. The M0 run evidently also replaced short names. The remaining ±5 lines come from small normalization differences, so the note's conclusions are unaffected.
