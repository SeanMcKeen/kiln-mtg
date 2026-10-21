# mtg-rules-engine *(working title)*

**A Magic: The Gathering rules engine and simulation lab that your AI can drive.**

> "Evaluate deck X in a cEDH pod against Y, Z and C."
> → thousands of seeded, rules-exact games → win rates with confidence intervals, game-shape stats, per-card analytics, and replayable logs.

## Status

**Milestone M1 (Foundations & Data) is in review.** The Rust workspace, core types, the card database pipeline, Commander deck validation, and CR tooling all work. The game engine starts in M2. All scope, research, architecture and decisions live in the Obsidian vault under [`docs/`](docs/00%20Home.md). Start at **[docs/00 Home.md](docs/00%20Home.md)**.

## Quick start

```bash
cargo build --release
# Card data is downloaded, never bundled (Scryfall bulk data; cached ≥ 24 h)
./target/release/mtg data fetch
./target/release/mtg data build
# Validate a Commander deck: legality and per-card engine coverage
./target/release/mtg deck check tests/fixtures/decks/kinnan.txt
./target/release/mtg deck check my-deck.txt --commander "Kinnan, Bonder Prodigy" --json
```

Every card currently reports `unsupported` coverage, because card behaviour arrives in M3. The engine refuses to pretend otherwise.

## What this is

- **A rules-exact engine** written in Rust. Illegal actions are impossible to take, and every choice the Comprehensive Rules give a player is exposed and can be controlled, including trigger ordering, replacement ordering, targets, modes, X, and mana payment.
- **Commander-first.** It is built for N players from day one, with cEDH 4-player pods as the headline use case.
- **A simulation lab.** Experiments are seeded and parallel, use pluggable player policies (baseline, AI-authored playbooks, search AI, live LLM), and produce flag-controlled metrics.
- **Agent-first interfaces.** An MCP server, a CLI with JSON output, and a Rust library, plus Python and WASM later.
- **Also a rules judge.** You can set up a board, step through actions, and get explanations that cite CR rule numbers.

## What this is not

It is not a playable game client or UI. Other people can build one on the library.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

## Legal

This is unofficial Fan Content permitted under the [Wizards of the Coast Fan Content Policy](https://company.wizards.com/en/legal/fancontentpolicy). It is not approved or endorsed by Wizards. Portions of the materials used are property of Wizards of the Coast. ©Wizards of the Coast LLC.

The repository **does not bundle** card data, card images, or the Comprehensive Rules. Card data is downloaded from [Scryfall](https://scryfall.com/docs/api) and/or [MTGJSON](https://mtgjson.com) at build or run time, and their terms apply.
