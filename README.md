# Kiln

**A Magic: The Gathering rules engine and simulation lab that your AI can drive.**

> "Evaluate deck X in a cEDH pod against Y, Z and C."
> → thousands of seeded, rules-exact games → win rates with confidence intervals, game-shape stats, per-card analytics, and replayable logs.

Crate and packages: `kiln-mtg` (crates.io, PyPI, npm). CLI binary: `kiln`.

## Status

**Milestone M8 (agent integration) is in review.** The N-player Commander engine runs 278 verified real cards (the curated cEDH pool). The simulation lab runs declarative experiments (`kiln sim run spec.yaml`) and reports win rates with 95% intervals, game shape, mulligans, per-card statistics, A/B comparisons and validity warnings. Decks can be piloted by AI-written Rhai playbooks, and agents drive everything through an MCP server (`kiln mcp serve`) or the CLI; see [docs/agents/AGENT_GUIDE.md](docs/agents/AGENT_GUIDE.md). Search AI arrives in M9. All scope, research, architecture and decisions live in the Obsidian vault under [`docs/`](docs/00%20Home.md). Start at **[docs/00 Home.md](docs/00%20Home.md)**.

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

Cards are `verified` (hand-written definitions checked by scenarios), `parsed` (from the Oracle-text parser) or `unsupported`. Experiments refuse unverified cards unless the spec allows them; the engine never pretends a card works.

```bash
# Run an experiment: four verified Commander decks, 400 games, greedy pilots
./target/release/kiln sim run examples/specs/m6-pod.yaml --threads 2
# Goldfish speed test, opening-hand analysis, and an A/B card swap
./target/release/kiln sim run examples/specs/najeela-goldfish.yaml --threads 2
./target/release/kiln sim run examples/specs/kinnan-opening-hands.yaml --threads 2
./target/release/kiln sim run examples/specs/m6-ab.yaml --threads 2
# Look at one game of a run again, event by event
./target/release/kiln game inspect runs/m6-pod 247
# Fuzz the engine: random 4-player games with every invariant checked
./target/release/kiln dev fuzz --games 10000 --invariants --views --replay-every 100
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
