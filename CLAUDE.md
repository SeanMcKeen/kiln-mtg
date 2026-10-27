# CLAUDE.md: orientation for AI sessions

## Project
**Kiln** (crate `kiln-mtg`, CLI binary `kiln`) is a Rust MTG rules engine plus a simulation lab. Crates use the `kiln-` prefix, MCP tools the `kiln_` prefix (see `docs/10 Decisions/ADR-0013 Project Name.md`). **The code still uses the pre-rename names** (`mtg-*` crates, `mtg` binary); renaming it to match ADR-0013 is pending. The headline use case: an AI agent runs "deck X vs Y, Z, C in a cEDH pod" for thousands of games and gets statistics. Commander-first, with N players from day one. **Current phase: M6 (Commander rules, multiplayer, the curated cEDH pool) complete and in review; M7 next.** The engine executes card IR v1; the card library (ADR-0007 hybrid, accepted) supports 3,074 Commander-legal cards, 278 of them verified. Four verified 100-card Commander test decks are in `tests/fixtures/decks/`. Check `docs/09 Roadmap/Roadmap.md` for the active milestone.

## Where things are
- `crates/` is the Cargo workspace (layout in `docs/03 Architecture/Crate Layout.md`). The CLI binary is `mtg` (`crates/mtg-cli`).
- The engine (`crates/mtg-engine`) is a decision state machine: `Game::pending()` → `Game::apply(choice)`. Rules procedures are tasks on a stack in the state (`engine.rs`); each rule module (`cast.rs`, `combat.rs`, `sba.rs`, `mana.rs`, `counters.rs`, `targets.rs`) adds `impl Game` methods. Card behaviour comes only from `mtg_cards::ir` abilities on each `CardDef` (`card.rs`), executed by the resumable interpreter in `effects.rs`; `support.rs` rejects IR the engine can't run yet, naming the missing feature.
- `cards/defs/*.yaml` are card definitions (IR only, never Oracle text). `tests/scenarios-real/*.yaml` verify real cards (`verifies:` lists) and need the local card DB; `mtg dev verify-cards` reports coverage.
- `tests/scenarios/*.yaml` are CR-cited rules scenarios on synthetic cards (format: `docs/08 Testing/Scenario File Format.md`); `tests/golden/*.json` are golden replays (re-bless with `MTG_BLESS=1 cargo test -p mtg-sim --test golden` and say why).
- `tests/fixtures/` holds **synthetic** card fixtures (`cards-mini.jsonl` in Scryfall's shape; `synthetic-ir-cards.yaml`, made-up IR cards for engine tests) and decklists. Real card data stays in `data/cache/` (gitignored).
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
- Commits only when asked. Branch: `main`, remote `origin` = https://github.com/SeanMcKeen/kiln (public).
- No `HashMap` iteration, wall-clock time or floats in anything that affects output bytes or game logic (ADR-0005). Use `BTreeMap` or sorted `Vec`s.

## Build, test, check
```bash
cargo build --release
cargo nextest run --workspace          # real-data tests skip unless data/cache/current or MTG_CARD_DB exists
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
./target/release/mtg dev cr-check      # every `CR x.y` citation in docs/ and crates/ must exist in the CR
./target/release/mtg data build --input docs/_external/oracle-cards.jsonl.gz --source-date 2026-10-20
./target/release/mtg deck check tests/fixtures/decks/kinnan.txt --json
./target/release/mtg dev fuzz --games 10000 --invariants --views --replay-every 100   # engine fuzzing (--verified / --library for real cards; --commander, --concede N)
./target/release/mtg dev fuzz --verified --decks tests/fixtures/decks/{kinnan-verified,thrasios-tymna,najeela,rograkh-silas}.txt --policy mixed   # real decks
./target/release/mtg data coverage                                                    # card library coverage and top gaps
./target/release/mtg dev verify-cards --list                                          # real-card verification (needs the card DB)
cargo bench -p mtg-engine                                                             # clone / enumeration / game benchmarks
```

## Refreshing local reference data
```bash
# Comprehensive Rules (update the date when WotC publishes a new version)
curl -sSL -o "docs/_external/cr.txt" "https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.txt"
tr -d '\r' < docs/_external/cr.txt | sed 's/^\xEF\xBB\xBF//' > docs/_external/cr-lf.txt
# Scryfall oracle cards: get jsonl_download_uri from https://api.scryfall.com/bulk-data (send User-Agent + Accept headers)
```
