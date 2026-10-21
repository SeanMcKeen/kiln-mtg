---
type: architecture
status: proposed
tags: [architecture, decisions, api, core]
updated: 2026-10-20
---

# Decision & Action Model

> [!important] This is the most important interface in the project
> Policies, the search AI, the LLM bridge, the judge REPL, Python users and the MCP server all interact with games **only** through `PendingDecision → Choice`. Getting it right is what delivers "illegal actions are impossible, legal choices are all available."

## Core contract
1. The engine runs until some player must choose. It then returns a `PendingDecision`.
2. The `PendingDecision` holds **the complete set of legal options**, either enumerated or as a compact, validated constraint (see "Large option spaces" below).
3. A client returns a `Choice` that references an option. The engine **re-validates** it and rejects anything not offered. That rejection means a client bug. It is never a way to probe the rules.
4. Nothing else mutates the game. No "force" API exists outside test and scenario setup.

```rust
// Illustrative sketch, not final.
pub struct PendingDecision {
    pub id: DecisionId,            // monotonically increasing; Choice must echo it
    pub player: PlayerId,          // who decides (may differ from active player: CR 101.4, 723)
    pub kind: DecisionKind,
    pub context: DecisionContext,  // what caused it: source object, CR rule ref, prompt text
}

pub enum DecisionKind {
    Mulligan { hand_size: u8, mulligans_taken: u8 },                  // CR 103.5
    BottomCards { choose: u8, from: Vec<ObjectRef> },                 // London mulligan bottoming
    Priority { actions: Vec<PriorityAction> },                        // CR 117: cast, activate, play land, special action, pass
    ChooseModes { min: u8, max: u8, modes: Vec<ModeOption> },         // CR 700.2
    ChooseX { min: u64, max: Option<u64> },                           // CR 107.3
    ChooseTargets { slot: TargetSlot, legal: Vec<TargetRef> },        // one target slot at a time (factoring)
    PayCost { plan_options: Vec<PaymentPlan> },                       // mana + other costs (CR 601.2g-h)
    ActivateManaAbility { options: Vec<ManaAbilityRef> },             // during cost payment (CR 605.3)
    OrderTriggers { triggers: Vec<TriggerRef> },                      // CR 603.3b
    ChooseReplacement { effects: Vec<ReplacementRef>, event: EventRef }, // CR 616.1
    DeclareAttacker { candidate: ObjectRef, options: Vec<AttackTarget> }, // factored per creature (CR 508.1)
    DeclareBlocker { candidate: ObjectRef, options: Vec<ObjectRef> },     // factored per creature (CR 509.1)
    DivideDamage { amount: u64, among: Vec<DamageRecipient> },        // CR 510.1c-d, 601.2d
    YesNo { prompt: Prompt },                                         // "may" abilities, optional costs
    ChooseObjects { min: u32, max: u32, from: Vec<ObjectRef> },       // sacrifice, discard, search…
    ChoosePlayer { options: Vec<PlayerId> },
    ChooseOption { options: Vec<NamedOption> },                       // colors, types, card names, "unless" payments
    OrderObjects { objects: Vec<ObjectRef>, destination: ZoneRef },   // scry/surveil/bottom order
    ProposeShortcut { loop_desc: LoopDescription },                   // CR 732.2a: choose iteration count / accept / respond
    CommanderZoneChange { commander: ObjectRef, to: ZoneId },         // CR 903.9a/b
}
```

## Design rules
### R1: Factor large decisions
The joint space for "declare attackers" with 20 creatures and 3 opponents is 4^20. The engine therefore asks **one creature at a time**, and the legality of the full declaration (requirements and restrictions, CR 508.1c-d) is validated at the end. If the final declaration is illegal, the engine must prevent reaching that state, which means constraint-aware incremental offering. That's an open design problem logged in [[12 Open Questions]]. Factoring:
- keeps option lists small, which matters for MCTS branching and LLM context
- makes "decision" a uniform unit for metrics
- matches how the CR describes the procedures

### R2: Cost payment is plan-based, with expert mode
Mana payment has huge combinatorics, from multiple mana abilities, hybrid and Phyrexian symbols, and conditional mana. Two levels are offered:
- `PayCost` with **enumerated canonical payment plans**, de-duplicated by equivalent outcomes. This is the default.
- Full manual mode, which activates mana abilities step by step, for the judge tool and for exact-replay fidelity.

Both are legal CR behaviour (CR 601.2g–h, 605.3a). Plans are an engine-provided *enumeration*, not an auto-choice. Open: canonicalization rules ([[Costs & Mana]]).

### R3: Every choice carries context
`DecisionContext` names the source object, the CR rule that created the decision (e.g. `CR 603.3b`), and a human-readable prompt. LLM policies, judge explanations and replay viewers depend on this.

### R4: Simultaneous choices are sequential in APNAP order
CR 101.4 already sequences simultaneous choices. Hidden simultaneous choices (CR 101.4a) are made sequentially but stay hidden from other players until revealed ([[Hidden Information & Player Views]]).

### R5: Convenience is policy-side
"Always pass priority when I have nothing", "auto-order triggers", "auto-pay": these are **policy wrappers** ([[Policy Composition]]). The engine may offer *hints*, such as "this is the only legal option" or "these orders are outcome-equivalent", so that policies and the runner can skip trivial decisions cheaply.

### R6: Trivial-decision fast path
Most priority decisions in a cEDH game are "pass". The runner can auto-answer decisions with exactly one option, or ones covered by a policy's declared auto-pass predicate, without invoking the policy. That's a large throughput win, and it's still recorded in the choice log for replay.

### R7: Choices are serializable and stable
`Choice` references options by **index within the decision**, plus the decision ID. Logs store `(decision_id, option_index)` and replay re-derives the decision. That makes replay logs compact and validates determinism ([[Determinism & Replay]]).

## Actions available at priority (CR 117.1)
- Cast a spell (CR 601), including alternative and additional costs, which are chosen during casting via sub-decisions.
- Activate an activated ability (CR 602), including mana abilities outside payment (CR 605.3a).
- Play a land (CR 305.1, a special action, CR 116.2a).
- Other special actions (CR 116): morph face-up, suspend, companion to hand, etc.
- Pass priority.
- Concede (CR 104.3a). This is allowed at any time, but in simulations it's only offered if the spec enables concession.

## Open questions
- Constraint-aware factoring for attack/block requirements: see [[Combat]].
- Payment-plan canonicalization and equivalence: see [[Costs & Mana]].
- Whether "cast spell" itself is factored (choose card → choose mode → targets → costs) or partly bundled. The leaning is fully factored, following CR 601.2a–i in order.

## Related
[[Policy Interface]] · [[Game State Model]] · [[ADR-0004 Engine as Decision State Machine]] · [[Priority & the Stack]]
