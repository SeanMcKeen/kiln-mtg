---
name: kiln-sim
description: Run Magic: The Gathering Commander simulations with Kiln and read the results with confidence intervals. Use when asked how a Commander or cEDH deck performs in a pod, how fast it wins (goldfish), how keepable its opening hands are, whether a card swap helps (A/B), or to compare decks or pilots.
---

# Kiln simulations

Kiln plays thousands of rules-exact Commander games from decklists and an experiment spec, and reports results with 95% intervals. Read `docs/agents/AGENT_GUIDE.md` for the full guide. This skill is the short path through the CLI; if the `kiln` MCP server is connected, the `kiln_*` tools do the same steps.

## Steps

1. **Build once**: `cargo build --release`, plus `./target/release/kiln data fetch && ./target/release/kiln data build` if there's no card DB yet.
2. **Check each deck**: `./target/release/kiln deck check <deck.txt> --json`. It must be `legal`; tell the user about any card that isn't `verified` (the coverage gate refuses them unless the spec sets `limits: { coverage: warn }`).
3. **Write a spec** (`kiln-spec/1`, YAML). Start from `examples/specs/`:
   - pod: `m6-pod.yaml`, `us1-kinnan-pod.yaml`
   - goldfish speed: `najeela-goldfish.yaml`
   - opening hands: `kinnan-opening-hands.yaml`
   - card swap: `m6-ab.yaml`
   - playbook pilots: `m8-playbook-pod.yaml`

   Deck paths are relative to the spec file. Use 2,000–5,000 games for decisions (±1.2 pp at 5,000).
4. **Run**: `./target/release/kiln sim run <spec.yaml> --threads 2`. This prints the markdown report and writes `runs/<name>/`. The same name resumes; pick a new name for a new experiment.
5. **Read**: `./target/release/kiln sim report runs/<name> --format concise` (JSON) or `--format markdown`. Look at one game with `kiln game inspect runs/<name> <index>`.
6. **Optional playbooks** (Rhai scripts that pilot a deck): write one per deck from `examples/playbooks/`, then check it with `./target/release/kiln playbook check <deck.txt> <playbook.rhai>`.

## Reporting to the user

- Give win rates **with their intervals**. If intervals overlap, say the decks can't be ranked from this run.
- Pass on the report's warnings: baseline pilots (greedy/random aren't expert play), capped games, unverified cards, few games.
- For "should I play X?", run `mode: ab`. Per-card numbers are correlational.
- Win turn means the deck's own turn number.
