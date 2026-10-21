---
type: design
status: draft
tags: [card-data, model]
updated: 2026-10-20
---

# Card Data Model

## Card record (ingested, rules-relevant)
```rust
pub struct CardRecord {
    pub oracle_id: OracleId,           // primary key (Scryfall oracle_id)
    pub name: String,
    pub layout: Layout,                // normal, transform, modal_dfc, split, adventure, flip, meld, saga, class, prepare, …
    pub faces: SmallVec<[FaceRecord; 2]>,
    pub color_identity: Colors,        // CR 903.4 (from source; recomputed and cross-checked)
    pub legalities: FormatLegalities,  // per format: legal / banned / restricted / not_legal
    pub game_changer: bool,            // Commander brackets
    pub tags: Vec<TagId>,              // Scryfall oracle_tags (functional categories)
    pub rulings: Vec<RulingRef>,       // keyed by oracle_id
}
pub struct FaceRecord {
    pub name: String, pub mana_cost: ManaCost, pub type_line: TypeLine,
    pub oracle_text: String, pub reminder_stripped: String,
    pub power: Option<PtValue>, pub toughness: Option<PtValue>, pub loyalty: Option<..>, pub defense: Option<..>,
    pub colors: Colors, pub color_indicator: Option<Colors>, pub keywords: Vec<String>,
}
```

## Card definition (behaviour)
```rust
pub struct CardDefinition {
    pub oracle_id: OracleId,
    pub faces: SmallVec<[FaceDefinition; 2]>,   // IR: abilities, static effects, replacement effects, costs…
    pub provenance: Provenance,                 // Parser { grammar_version } | Override { file, author } | Escape { fn_name }
    pub coverage: Coverage,
}
pub enum Coverage {
    Unsupported { reasons: Vec<UnsupportedSpan> },   // which text couldn't be handled
    Parsed,                                          // has IR, not yet verified by tests
    Verified { tests: Vec<TestId>, since: CardDbVersion },
}
```

## Coverage rules
- **`Verified`** requires at least one scenario test per ability, exercising its main behaviour (see [[CR-Cited Scenario Tests]]). Cards in the [[Curated cEDH Core Pool]] also require their Tricky Interactions entries to pass.
- Errata (an Oracle text change) or a bug report demotes a card to **`Parsed`**.
- **Experiments gate on coverage** (FR-S8). By default they refuse non-verified cards. `--allow-unverified` runs anyway, but stamps the report with a warning and the list of cards ([[Validity & Caveats]]).

## Related
[[Ingestion Pipeline]] · [[ADR-0007 Card Definition Approach]] · [[Deck Import]]
