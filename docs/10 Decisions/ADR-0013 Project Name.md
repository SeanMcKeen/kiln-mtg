---
type: adr
status: accepted
decided: 2026-10-22
supersedes:
tags: [adr, naming]
updated: 2026-10-22
---

# ADR-0013: Project Name

## Context
"mtg-rules-engine" was a working title (formerly Q1 in [[12 Open Questions]]). The project needs a short name that works as a crate, a CLI binary, an MCP server and Python/npm packages. It should avoid WotC trademarks such as "Magic" ([[Legal & Licensing]]). Names built on card puns (Labmaniac, Thoracle, Rhystic) were rejected as gimmicky.

## Options
1. **Kiln:** decks go in and get tested under fire across thousands of games. Short, serious, and not slang.
2. Ordeal, Anvil, Fourfold: the same idea, but less punchy.
3. Card-name puns: rejected.

## Decision
The project is **Kiln**. The bare `kiln` name is taken on crates.io, PyPI and npm, so packages use `kiln-mtg`:

| Surface | Name |
|---|---|
| User-facing crate (CLI) | `kiln-mtg`. `cargo install kiln-mtg` installs the `kiln` binary |
| Workspace crates | `kiln-*` (see [[Crate Layout]]). `kiln-core` and `kiln-cli` are taken, so the primitives crate is `kiln-types` and the CLI crate is `kiln-mtg` |
| CLI binary | `kiln` (e.g. `kiln sim run spec.yaml`) |
| MCP server / tools | `kiln` server with `kiln_*` tools ([[MCP Server Tool Catalog]]) |
| File formats | `kiln-spec/1`, `kiln-report/1`, `kiln-replay/1`, `kiln-scenario/1` |
| Python | `pip install kiln-mtg`, `import kiln_mtg` ([[Python Package]]) |
| npm (WASM) | `kiln-mtg` ([[WASM Build]]) |

All of these names were confirmed free on 2026-10-22. Nothing has been published to reserve them.

## Consequences
- The local folder and repo are still named `mtg rules engine`. Rename the GitHub repo to `kiln` when it's created ([[M0 Groundwork]]).
- Publish placeholder `kiln-mtg` crate, wheel and npm packages early if squatting becomes a concern.
- The `kiln-*` workspace crate names other than `kiln-mtg` aren't reserved, so check them again before the first publish.

**Accept by:** M0 (accepted)
