# Kiln

**A Magic: The Gathering rules engine and simulation lab that your AI can drive.**

> "Evaluate deck X in a cEDH pod against Y, Z and C."
> → thousands of seeded, rules-exact games → win rates with confidence intervals, game-shape stats, per-card analytics, and replayable logs.

Crate and packages: `kiln-mtg` (crates.io, PyPI, npm). CLI binary: `kiln`.

## Status

**Milestone M2 (Core Game Loop) is in review.** The N-player engine plays complete games: lands, mana with payment plans, casting, the stack, combat (first and double strike, trample, deathtouch, lifelink, menace, flying, reach), state-based actions, mulligans, player views, deterministic replays, and a parallel fuzz runner. Cards are a small synthetic test set plus real vanilla creatures and basic lands. Real card behaviour arrives in M3. All scope, research, architecture and decisions live in the Obsidian vault under [`docs/`](docs/00%20Home.md). Start at **[docs/00 Home.md](docs/00%20Home.md)**.

## Quick start

```bash
cargo build --release
# Card data is downloaded, never bundled (Scryfall bulk data; cached ≥ 24 h)
./target/release/kiln data fetch
./target/release/kiln data build
# Validate a Commander deck: legality and per-card engine coverage
./target/release/kiln deck check tests/fixtures/decks/kinnan.txt
./target/release/kiln deck check my-deck.txt --commander "Kinnan, Bonder Prodigy" --json
```

Every card currently reports `unsupported` coverage, because card behaviour arrives in M3. The engine refuses to pretend otherwise.

```bash
# Play 10,000 random 4-player games with every invariant checked
./target/release/kiln dev fuzz --games 10000 --invariants --views --replay-every 100
# The same with real vanilla creatures and basic lands from the card DB
./target/release/kiln dev fuzz --games 10000 --real
```

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
