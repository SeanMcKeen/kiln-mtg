---
type: adr
status: accepted
decided: 2026-10-20
tags: [adr, docs]
updated: 2026-10-20
---

# ADR-0003: Docs as an Obsidian Vault in the Repo

## Decision
All design docs live in `docs/` as an Obsidian-compatible vault. That means wikilinks, frontmatter (`type`, `status`, `tags`, `updated`), and `.base` dashboards. It's versioned with the code. Roadmap tracking happens in milestone notes now and is mirrored to GitHub issues once the repo exists.

## Consequences
Docs and code evolve together and diffs are reviewable. Note filenames must be unique, because wikilinks resolve by name. Run a link checker in CI (M1).
