# CLAUDE.md: orientation for AI sessions

## Project
This is a Rust MTG rules engine plus a simulation lab. The headline use case: an AI agent runs "deck X vs Y, Z, C in a cEDH pod" for thousands of games and gets statistics. Commander-first, with N players from day one. **Current phase: M1 (Foundations & Data) in review; M2 (core game loop) is next, pending the ADR-0012 phase.rs decision.** Check `docs/09 Roadmap/Roadmap.md` for the active milestone.

## Where things are
- `crates/` is the Cargo workspace (layout in `docs/03 Architecture/Crate Layout.md`). The CLI binary is `mtg` (`crates/mtg-cli`).
- `tests/fixtures/` holds a **synthetic** card fixture (`cards-mini.jsonl`, made-up cards in Scryfall's shape) and decklists. Real card data stays in `data/cache/` (gitignored).
- `tools/corpus-analysis/` has Python scripts for Oracle-text statistics. Run them with `python3 -I`.
- `docs/` is an Obsidian vault and the source of truth for scope and design. The hub is `docs/00 Home.md`.
- Decisions are in `docs/10 Decisions/` (ADRs). **Don't contradict an Accepted ADR. Supersede it with a new ADR instead.**
- Open questions are in `docs/12 Open Questions.md`.
- `docs/_external/` is gitignored and holds local copies of the Comprehensive Rules (`cr-lf.txt`) and Scryfall bulk data. Never commit WotC, Scryfall, or MTGJSON content.

## Conventions
- **Cite CR rule numbers** (e.g. `CR 603.3b`) in specs, code comments, and tests. Verify numbers against `docs/_external/cr-lf.txt`, not from memory. The current CR is effective 2026-06-19.
- **Legality by construction.** The engine only offers legal choices. Never silently auto-resolve a choice the CR gives a player; auto-resolve must be an explicit, opt-in policy.
- Vault notes need frontmatter with `type`, `status`, `tags`, and `updated`. Link with `[[wikilinks]]`. New notes start from `docs/_templates/`.
- Research notes end with a `## Sources` section.
- Update `docs/12 Open Questions.md` and the relevant milestone checklist whenever you resolve or discover something.
- Commits only when asked. Branch: `main` (no remote yet).
- No `HashMap` iteration, wall-clock time or floats in anything that affects output bytes or game logic (ADR-0005). Use `BTreeMap` or sorted `Vec`s.

## Build, test, check
```bash
cargo build --release
cargo nextest run --workspace          # real-data tests skip unless data/cache/current or MTG_CARD_DB exists
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
./target/release/mtg dev cr-check      # every `CR x.y` citation in docs/ and crates/ must exist in the CR
./target/release/mtg data build --input docs/_external/oracle-cards.jsonl.gz --source-date 2026-10-20
./target/release/mtg deck check tests/fixtures/decks/kinnan.txt --json
```

## Refreshing local reference data
```bash
# Comprehensive Rules (update the date when WotC publishes a new version)
curl -sSL -o "docs/_external/cr.txt" "https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.txt"
tr -d '\r' < docs/_external/cr.txt | sed 's/^\xEF\xBB\xBF//' > docs/_external/cr-lf.txt
# Scryfall oracle cards: get jsonl_download_uri from https://api.scryfall.com/bulk-data (send User-Agent + Accept headers)
```
