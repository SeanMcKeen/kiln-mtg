//! What the engine can execute (M3 plan, B-D2).
//!
//! [`check`] walks a definition's IR and rejects anything the engine can't
//! run yet, with a reason that names the missing feature and the milestone
//! that adds it. Each M3 phase widens it as features land; a card is never
//! partially supported.

use mtg_cards::ir::{
    Ability, ChoiceSpec, Condition, CostPart, DiscardSpec, Effect, ManaSpec, ObjectRef, ObjectSet,
    PlayerRef, Quantity, QuantityExpr, Recipient, RuleMod, SpellAbility, StaticAbility,
    StaticEffect, Timing,
};
use mtg_core::CardType;

use crate::card::{CardDef, Keywords, is_mana_ability};
use crate::effects::simple_mana;

/// Keywords whose rules the engine implements.
const IMPLEMENTED_KEYWORDS: [Keywords; 15] = [
    Keywords::FLASH,
    Keywords::HEXPROOF,
    Keywords::SHROUD,
    Keywords::INDESTRUCTIBLE,
    Keywords::DEATHTOUCH,
    Keywords::DEFENDER,
    Keywords::DOUBLE_STRIKE,
    Keywords::FIRST_STRIKE,
    Keywords::FLYING,
    Keywords::HASTE,
    Keywords::LIFELINK,
    Keywords::REACH,
    Keywords::TRAMPLE,
    Keywords::VIGILANCE,
    Keywords::MENACE,
];

type Check = Result<(), String>;

/// `Ok` if the engine can execute every ability of `def`.
pub(crate) fn check(def: &CardDef) -> Check {
    permanent_kind(def)?;
    let instant_or_sorcery = def.is(CardType::Instant) || def.is(CardType::Sorcery);
    let mut spells = 0;
    for a in &def.abilities {
        // "That card" / "that player" mean something only in a triggered
        // ability, or (the affected object or player) in a replacement.
        if !matches!(a, Ability::Triggered(_) | Ability::Replacement(_))
            && serde_json::to_string(a).is_ok_and(|j| {
                j.contains("\"triggering\"")
                    || j.contains("\"triggering_amount\"")
                    || j.contains("\"type_produced_by_trigger\"")
            })
        {
            return Err("a reference to a trigger event outside a triggered ability".into());
        }
        match a {
            // CR 702.124a: partner only modifies deck construction; it does nothing in play.
            Ability::Keyword(mtg_cards::ir::Keyword::Partner) => {}
            // CR 702.24a: `CardDef::new` adds it as a triggered ability (checked below).
            Ability::Keyword(mtg_cards::ir::Keyword::CumulativeUpkeep(_)) => {}
            // CR 702.21a: added as a triggered ability (checked below).
            Ability::Keyword(mtg_cards::ir::Keyword::Ward(_)) => {}
            // CR 702.40a: added as a triggered ability; 702.61: checked while offering actions.
            Ability::Keyword(
                mtg_cards::ir::Keyword::Storm | mtg_cards::ir::Keyword::SplitSecond,
            ) => {}
            // CR 702.33: an optional additional cost.
            Ability::Keyword(mtg_cards::ir::Keyword::Kicker(c)) => cast_cost(c)?,
            // CR 702.11d: checked while choosing targets.
            Ability::Keyword(mtg_cards::ir::Keyword::HexproofFrom(_)) => {}
            // CR 702.109: an alternative cost for a permanent card.
            Ability::Keyword(mtg_cards::ir::Keyword::Dash(c)) => cast_cost(c)?,
            // Works in exile (CR 113.6f); checked while offering casts.
            Ability::Static(StaticAbility {
                effect: StaticEffect::Rule(RuleMod::MayCastFromExile),
                condition: None,
            }) => {}
            // CR 702.5, 702.6: attachment (equip is added as an activated ability).
            Ability::Keyword(
                mtg_cards::ir::Keyword::Enchant(_) | mtg_cards::ir::Keyword::Equip(_),
            ) => {}
            // CR 702.16: targeting, blocking and damage (Auras and Equipment in M5).
            Ability::Keyword(mtg_cards::ir::Keyword::Protection(_)) => {}
            Ability::Keyword(k) => match Keywords::of_ir(k) {
                Some(bit) if IMPLEMENTED_KEYWORDS.contains(&bit) => {}
                _ => return Err(keyword_gap(k)),
            },
            Ability::Spell(s) => {
                spells += 1;
                spell(s)?;
            }
            Ability::Activated(x) if is_mana_ability(x) => {
                // CR 605: mana abilities resolve immediately and can't be restricted
                // to sorcery timing in the IR's sense; their output was checked
                // when the definition was built.
                // "Activate only as an instant" is met by offering it only
                // with priority (never mid-payment; LED).
                if x.timing == Timing::Sorcery || x.once_per_turn {
                    return Err("mana abilities with timing restrictions".into());
                }
                activated_extras(x)?;
                cast_cost(&x.cost)?;
                effect(&x.effect)?;
            }
            Ability::Activated(x) => {
                activated_extras(x)?;
                if matches!(x.effect, Effect::Modal { .. }) {
                    return Err("modal activated abilities".into());
                }
                if x.targets.iter().any(|t| t.up_to == Some(0)) {
                    return Err("a target slot that allows zero targets".into());
                }
                cast_cost(&x.cost)?;
                effect(&x.effect)?;
            }
            Ability::Triggered(t) => {
                trigger(&t.trigger)?;
                if let Some(c) = &t.condition {
                    condition(c)?;
                }
                if let Effect::Modal { min, max, modes } = &t.effect
                    && (min > max || *max as usize > modes.len())
                {
                    return Err("modal triggered ability with impossible mode counts".into());
                }
                if t.targets.iter().any(|s| s.up_to == Some(0)) {
                    return Err("a target slot that allows zero targets".into());
                }
                effect(&t.effect)?;
            }
            Ability::Static(StaticAbility {
                effect: StaticEffect::Rule(RuleMod::CantBeCountered),
                condition: None,
            }) => {}
            Ability::Static(s) => rule_static(s)?,
            Ability::Replacement(r) => replacement(r)?,
            Ability::OpeningHand(o) => effect(&o.effect)?,
            Ability::AsEnters { choose, .. } => match choose {
                mtg_cards::ir::ChoiceSpec::Color | mtg_cards::ir::ChoiceSpec::Number { .. } => {
                    if def.is(mtg_core::CardType::Land) {
                        return Err("lands with an \"as this enters, choose\" ability".into());
                    }
                }
                mtg_cards::ir::ChoiceSpec::CardName | mtg_cards::ir::ChoiceSpec::Opponent => {
                    return Err("choosing a card name or player as a permanent enters".into());
                }
            },
            // Whole-ability escapes need a hook per ability kind; none is needed yet.
            Ability::Escape(e) => {
                return Err(format!(
                    "ability-level escape {:?} (no ability escapes yet)",
                    e.function
                ));
            }
        }
    }
    match (instant_or_sorcery, spells) {
        (true, 1) | (false, 0) => Ok(()),
        (true, _) => Err("an instant or sorcery needs exactly one spell ability".into()),
        (false, _) => Err("only instants and sorceries have spell abilities".into()),
    }
}

/// Why a keyword isn't supported, without the IR's internals.
fn keyword_gap(k: &mtg_cards::ir::Keyword) -> String {
    use mtg_cards::ir::Keyword as K;
    match k {
        // CR 702.124j: "partner with" includes a "when this enters" trigger.
        K::PartnerWith(_) => "keyword partner with (M4: triggered abilities)".into(),
        other => format!("keyword {other:?} isn't implemented yet"),
    }
}

fn spell(s: &SpellAbility) -> Check {
    for alt in &s.alternative_costs {
        cast_cost(&alt.cost)?;
        if let Some(c) = &alt.condition {
            condition(c)?;
        }
    }
    cast_cost(&s.additional_costs)?;
    cast_cost(&s.per_extra_target)?;
    if s.targets.len() > 32 {
        return Err("more than 32 target slots".into());
    }
    if let Effect::Modal { min, max, modes } = &s.effect
        && (min > max || *max as usize > modes.len())
    {
        return Err("modal spell with impossible mode counts".into());
    }
    for (i, t) in s.targets.iter().enumerate() {
        if t.up_to == Some(0) {
            return Err(format!("target slot {i} allows zero targets"));
        }
    }
    effect(&s.effect)
}

/// Casting costs: mana may use any symbol the cast procedure pays (CR 601.2b, 601.2f).
fn cast_cost(parts: &[CostPart]) -> Check {
    for p in parts {
        if let CostPart::Mana(s) = p {
            let ok = s
                .parse::<mtg_core::ManaCost>()
                .is_ok_and(|c| c.symbols().iter().all(crate::card::payable_symbol));
            if !ok {
                return Err(format!("mana cost {s} with snow or non-tournament symbols"));
            }
        } else {
            cost(std::slice::from_ref(p))?;
        }
    }
    Ok(())
}

fn effect(e: &Effect) -> Check {
    match e {
        Effect::Seq(v) => v.iter().try_for_each(effect),
        Effect::Modal { modes, .. } => modes.iter().try_for_each(effect),
        Effect::May(x) => effect(x),
        Effect::IfDoes {
            who,
            action,
            then,
            otherwise,
        } => {
            player(who)?;
            effect(action)?;
            effect(then)?;
            otherwise.as_deref().map_or(Ok(()), effect)
        }
        Effect::If {
            condition: c,
            then,
            otherwise,
        } => {
            condition(c)?;
            effect(then)?;
            otherwise.as_deref().map_or(Ok(()), effect)
        }
        Effect::Unless {
            payer,
            cost: c,
            effect: x,
        } => {
            player(payer)?;
            cost(c)?;
            effect(x)
        }
        Effect::ForEachPlayer { effect: x, .. } => effect(x),
        Effect::Pay { who, cost: c } => {
            player(who)?;
            cost(c)
        }
        Effect::DealDamage { to, amount } => {
            match to {
                Recipient::Target(_) => {}
                Recipient::Player(p) => player(p)?,
                Recipient::Object(o) => object(o)?,
            }
            quantity(amount)
        }
        Effect::Destroy(s) => {
            if matches!(s, ObjectSet::ChosenFrom { .. }) {
                return Err("destroying chosen objects".into());
            }
            set(s)
        }
        Effect::Exile { what, .. } => set(what),
        Effect::Move { what, .. }
        | Effect::Pump { what, .. }
        | Effect::Tap(what)
        | Effect::Untap(what)
        | Effect::TapOrUntap(what) => set(what),
        Effect::RevealUntil { who, .. } | Effect::Shuffle(who) => player(who),
        Effect::Reveal(what) => set(what),
        Effect::Sacrifice { who, what } => {
            player(who)?;
            set(what)
        }
        // A targeted spell, or "that spell or ability" (ward, CR 702.21a).
        Effect::Counter(ObjectSet::One(ObjectRef::Target(_) | ObjectRef::Triggering))
        | Effect::CounterAndExile(ObjectSet::One(ObjectRef::Target(_))) => Ok(()),
        Effect::CounterAndExile(_) => Err("countering anything but a targeted spell".into()),
        Effect::LookAtHand(p) => player(p),
        Effect::Counter(_) => Err("countering anything but a targeted or triggering spell".into()),
        Effect::Draw { who, amount }
        | Effect::GainLife { who, amount }
        | Effect::LoseLife { who, amount }
        | Effect::Scry { who, amount } => {
            player(who)?;
            quantity(amount)
        }
        Effect::Discard { who, what } => {
            player(who)?;
            discard(what)
        }
        Effect::AddMana {
            mana,
            spend_only_on,
            uncounterable,
            to,
        } => {
            if let Some(p) = to {
                player(p)?;
            }
            if *uncounterable && spend_only_on.is_none() {
                return Err("an uncounterable rider without a spending restriction".into());
            }
            match mana {
                // Only meaningful in a triggered ability (checked in `check`).
                ManaSpec::TypeProducedByTrigger => Ok(()),
                ManaSpec::Colorless(q) | ManaSpec::AnyOneColor(q) | ManaSpec::AnyColor(q) => {
                    quantity(q)
                }
                ManaSpec::ColorOf(r) => object(r),
                ManaSpec::Times { mana, times } => {
                    crate::card::mana_output(&ManaSpec::Fixed(mana.clone()))?;
                    quantity(times)
                }
                _ => crate::card::mana_output(mana).map(|_| ()),
            }
        }
        Effect::Search { who, count, .. } => {
            player(who)?;
            quantity(count)
        }
        Effect::LookAtTop { who, count, .. } | Effect::MoveCards { who, count, .. } => {
            player(who)?;
            quantity(count)
        }
        Effect::Choose { what, .. } => match what {
            ChoiceSpec::Number { min, max } if max < min || max - min > 1000 => {
                Err("a number choice with more than 1,000 options".into())
            }
            _ => Ok(()),
        },
        Effect::WinGame(who) => player(who),
        Effect::CopyCard { what, .. } => set(what),
        // Only as the instead-effect of "if this would enter" (checked there).
        Effect::EnterAsCopy { except, .. } | Effect::CreateTokenCopy { except, .. } => {
            for w in &except.types {
                if !CardType::ALL
                    .iter()
                    .any(|t| t.word().eq_ignore_ascii_case(w))
                {
                    return Err(format!("unknown card type {w}"));
                }
            }
            if let Effect::CreateTokenCopy { of, .. } = e {
                set(of)?;
            }
            Ok(())
        }
        Effect::CopySpell {
            what,
            count,
            controller,
            ..
        } => {
            if let Some(q) = count {
                quantity(q)?;
            }
            if let Some(p) = controller {
                player(p)?;
            }
            set(what)
        }
        Effect::ChangeTargets(r) => object(r),
        Effect::EndTheTurn | Effect::CopyTriggeringAbility { .. } => Ok(()),
        Effect::Flicker(what) => set(what),
        Effect::AdditionalCombatPhase => Ok(()),
        Effect::ControlNextTurn(who) => player(who),
        Effect::Vote { choices, .. } => {
            if choices.is_empty() || choices.len() > 8 {
                return Err("a vote with no choices or more than 8".into());
            }
            choices.iter().try_for_each(|c| effect(&c.per_vote))
        }
        Effect::Become {
            what,
            base_pt,
            add_subtypes,
        } => {
            if base_pt.is_none() && add_subtypes.is_empty() {
                return Err("a \"becomes\" effect that changes nothing".into());
            }
            set(what)
        }
        Effect::MayCastThisTurn(what) => set(what),
        Effect::Repeat(x) => effect(x),
        Effect::GainControl { what, .. } => set(what),
        Effect::ExchangeControl { a, b, otherwise } => {
            object(a)?;
            object(b)?;
            otherwise.as_deref().map_or(Ok(()), effect)
        }
        Effect::Attach { to } => object(to),
        Effect::PhaseOut { what } => set(what),
        Effect::PlayerRule { who, .. } => player(who),
        Effect::ReturnAttached => Ok(()),
        Effect::GainKeyword { what, keyword } => match Keywords::of_ir(keyword) {
            Some(bit) if IMPLEMENTED_KEYWORDS.contains(&bit) => set(what),
            Some(_) if matches!(keyword, mtg_cards::ir::Keyword::HexproofFrom(_)) => set(what),
            _ => Err(format!("granting {}", keyword_gap(keyword))),
        },
        Effect::Cast {
            what,
            without_paying: true,
        } => set(what),
        Effect::Cast { .. } => Err("casting a spell during resolution and paying for it".into()),
        Effect::EntersTapped | Effect::EntersWithCounters { .. } => {
            Err("\"enters tapped\" is a replacement effect (M4)".into())
        }
        Effect::CreateToken {
            token,
            count,
            controller,
            ..
        } => {
            crate::card::token_def(token).map_err(|e| e.reason)?;
            player(controller)?;
            quantity(count)
        }
        Effect::PutCounters { what, amount, .. } | Effect::RemoveCounters { what, amount, .. } => {
            set(what)?;
            quantity(amount)
        }
        Effect::Amass { amount, .. } => quantity(amount),
        Effect::Delayed { at, effect: x } => {
            trigger(at)?;
            effect(x)
        }
        Effect::LoseGame(who) => player(who),
        Effect::Escape(x) => match crate::escapes::lookup(&x.function) {
            Some(_) => Ok(()),
            None => Err(format!("escape function {:?} isn't registered", x.function)),
        },
    }
}

/// Kinds of permanent whose own rules (and state-based actions) aren't
/// implemented yet; a card of one of them can't be run correctly even with
/// supported abilities (M4 4.5).
fn permanent_kind(def: &CardDef) -> Check {
    let t = &def.type_line;
    if def.is(CardType::Planeswalker) && def.loyalty.is_none() {
        return Err("a planeswalker without a printed loyalty (CR 306.5b)".into());
    }
    if def.is(CardType::Battle) {
        return Err("battles: defense and protectors (CR 310.4b, 704.5v-x)".into());
    }
    if t.has_supertype(mtg_core::Supertype::World) {
        return Err("the world rule (CR 704.5k)".into());
    }
    // Equipment and Fortifications stay: without an equip ability (not in
    // the IR yet) they are never attached, and unattached ones do nothing.
    for (sub, why) in [
        (
            "Saga",
            "Sagas: lore counters and chapters (CR 714.2, 704.5s; M5)",
        ),
        ("Role", "Roles (CR 704.5y)"),
    ] {
        if t.has_subtype(sub) {
            return Err(why.into());
        }
    }
    Ok(())
}

/// The rule statics M4 implements as engine checks (M4 plan R5).
fn rule_static(s: &StaticAbility) -> Check {
    if crate::layers::is_layer_static(&s.effect) {
        if let Some(c) = &s.condition {
            condition(c)?;
        }
        return layer_static(&s.effect);
    }
    // Restrictions the engine checks while offering actions (CR 613.11).
    if crate::layers::is_rule_static(&s.effect) {
        if let Some(c) = &s.condition {
            condition(c)?;
        }
        return Ok(());
    }
    if s.condition.is_some() {
        return Err("static abilities (M5)".into());
    }
    match &s.effect {
        StaticEffect::DoesntUntap(_) => Ok(()),
        StaticEffect::SkipStep {
            step: mtg_cards::ir::StepName::Draw,
            ..
        } => Ok(()),
        StaticEffect::SkipStep { .. } => Err("skipping a step other than the draw step".into()),
        _ => Err("static abilities (M5)".into()),
    }
}

/// Static abilities the layers apply (CR 613; M5 plan L5).
fn layer_static(e: &StaticEffect) -> Check {
    match e {
        StaticEffect::Multi(v) => v.iter().try_for_each(|e| match e {
            StaticEffect::Multi(_) => Err("nested multi-part statics".into()),
            e => layer_static(e),
        }),
        StaticEffect::AddTypes { types, .. } => {
            for w in types {
                if !CardType::ALL
                    .iter()
                    .any(|t| t.word().eq_ignore_ascii_case(w))
                {
                    return Err(format!("unknown card type {w}"));
                }
            }
            Ok(())
        }
        StaticEffect::Grant { keyword, .. } => match Keywords::of_ir(keyword) {
            Some(bit) if IMPLEMENTED_KEYWORDS.contains(&bit) => Ok(()),
            Some(_) if matches!(keyword, mtg_cards::ir::Keyword::HexproofFrom(_)) => Ok(()),
            _ => Err(format!("granting {}", keyword_gap(keyword))),
        },
        StaticEffect::LandType { .. }
        | StaticEffect::RemoveTypes { .. }
        | StaticEffect::LoseAllAbilities { .. }
        | StaticEffect::SetBasePt { .. }
        | StaticEffect::Pump { .. } => Ok(()),
        _ => Err("static abilities (M5)".into()),
    }
}

/// Replacement effects the engine applies (M4 plan R1–R4).
/// "Exile it instead": the zone-change replacements the move itself applies.
pub(crate) fn is_exile_instead(e: &Effect) -> bool {
    matches!(
        e,
        Effect::Exile {
            what: ObjectSet::One(ObjectRef::Triggering),
            ..
        }
    )
}

fn replacement(r: &mtg_cards::ir::ReplacementAbility) -> Check {
    use mtg_cards::ir::ReplaceEvent as R;
    match (&r.event, &r.instead) {
        (R::WouldEnter(_), Effect::EntersTapped) if r.condition.is_none() => Ok(()),
        (R::WouldEnter(f), Effect::EntersWithCounters { amount, .. })
            if f.this && r.condition.is_none() =>
        {
            quantity(amount)
        }
        (R::WouldEnter(_), Effect::EntersTapped) => {
            Err("conditional \"enters tapped\" (later in M4)".into())
        }
        (R::WouldEnter(f), instead) if f.this && r.condition.is_none() => effect(instead),
        // "If [an object] would enter, exile it instead" (Containment Priest).
        (R::WouldEnter(_), instead) if is_exile_instead(instead) && r.condition.is_none() => Ok(()),
        (R::WouldEnter(_), _) => Err("replacing other permanents entering (later)".into()),
        (R::WouldDraw { .. }, instead) => {
            if let Some(c) = &r.condition {
                condition(c)?;
            }
            effect(instead)
        }
        // "... exile it instead" (Rest in Peace, Leyline of the Void).
        (R::WouldGoToGraveyard(_), instead)
            if is_exile_instead(instead) && r.condition.is_none() =>
        {
            Ok(())
        }
        (R::WouldGoToGraveyard(_), _) => {
            Err("graveyard replacements other than \"exile it instead\"".into())
        }
        // Applied by the search itself when a player controls the search
        // (Opposition Agent, CR 723); the replacement records what it does.
        (R::WouldFindWhileSearching(_), instead) => effect(instead),
    }
}

/// Trigger events the engine detects (CR 603.2).
fn trigger(t: &mtg_cards::ir::Trigger) -> Check {
    use mtg_cards::ir::Trigger as T;
    match t {
        T::AnyOf(v) => v.iter().try_for_each(trigger),
        T::Enters(_)
        | T::TapsForMana { .. }
        | T::BecomesTarget { .. }
        | T::GainsLife { .. }
        | T::LosesLife { .. }
        | T::Dies(_)
        | T::LeavesBattlefield(_)
        | T::Cast { .. }
        | T::Draws { .. }
        | T::Discards { .. }
        | T::BeginningOf { .. }
        | T::Attacks(_)
        | T::BecomesTapped(_)
        | T::Searches { .. }
        | T::Activates { .. }
        | T::DealsCombatDamageToPlayer(_) => Ok(()),
    }
}

fn object(o: &ObjectRef) -> Check {
    match o {
        // Only triggered abilities may say "that card" (checked in `check`).
        ObjectRef::Triggering
        | ObjectRef::This
        | ObjectRef::Target(_)
        | ObjectRef::Remembered(_)
        | ObjectRef::AttachedTo => Ok(()),
    }
}

fn player(p: &PlayerRef) -> Check {
    match p {
        PlayerRef::ControllerOf(o) | PlayerRef::OwnerOf(o) => object(o),
        PlayerRef::Triggering
        | PlayerRef::You
        | PlayerRef::Target(_)
        | PlayerRef::Each
        | PlayerRef::Chosen(_) => Ok(()),
    }
}

fn set(s: &ObjectSet) -> Check {
    match s {
        ObjectSet::Zone { owner, .. } => player(owner),
        ObjectSet::One(o) => object(o),
        ObjectSet::All(_) => Ok(()),
        ObjectSet::TopOfLibrary { owner, count } => {
            player(owner)?;
            quantity(count)
        }
        ObjectSet::ChosenFrom { who, count, .. } => {
            player(who)?;
            quantity(count)
        }
    }
}

fn quantity(q: &Quantity) -> Check {
    match q {
        Quantity::Fixed(_) => Ok(()),
        Quantity::Expr(e) => match e {
            QuantityExpr::PowerOf(o)
            | QuantityExpr::ManaValueOf(o)
            | QuantityExpr::CountersOn { what: o, .. } => object(o),
            QuantityExpr::CardsInLibrary(p) | QuantityExpr::CountOf { controller: p, .. } => {
                player(p)
            }
            QuantityExpr::Plus(q, _) | QuantityExpr::Neg(q) | QuantityExpr::HalfRoundedUp(q) => {
                quantity(q)
            }
            QuantityExpr::OpponentsDealtCombatDamageThisTurn
            | QuantityExpr::TriggeringAmount
            | QuantityExpr::X
            | QuantityExpr::Count(_)
            | QuantityExpr::Devotion(_)
            | QuantityExpr::MostDiscardedThisWay
            | QuantityExpr::SpellsCastThisTurn { .. }
            | QuantityExpr::MostSpellsCastByAnOpponent
            | QuantityExpr::LifeOf(_)
            | QuantityExpr::TopOfLibraryManaValue(_)
            | QuantityExpr::Var(_) => Ok(()),
        },
    }
}

/// An activated ability's restrictions beyond cost and timing.
fn activated_extras(x: &mtg_cards::ir::ActivatedAbility) -> Check {
    if let Some(c) = &x.condition {
        condition(c)?;
    }
    match x.zone {
        None | Some(mtg_cards::ir::ZoneName::Hand) => {}
        Some(z) => return Err(format!("abilities that work from the {z:?} zone")),
    }
    if let Some(q) = &x.cost_reduction {
        quantity(q)?;
    }
    Ok(())
}

fn condition(c: &Condition) -> Check {
    match c {
        Condition::ControlCommander => Ok(()),
        Condition::AtLeast(a, b) => {
            quantity(a)?;
            quantity(b)
        }
        Condition::Not(c) => condition(c),
        Condition::TopOfLibraryMatches { who, .. } => player(who),
        Condition::YourTurn
        | Condition::ThisTapped
        | Condition::InCombat
        | Condition::UnusedThisTurn
        | Condition::Kicked
        | Condition::TargetMatches { .. } => Ok(()),
    }
}

fn discard(d: &DiscardSpec) -> Check {
    match d {
        DiscardSpec::Hand => Ok(()),
        DiscardSpec::Chosen { count, .. } | DiscardSpec::Random(count) => quantity(count),
    }
}

fn cost(parts: &[CostPart]) -> Check {
    parts.iter().try_for_each(|p| match p {
        CostPart::Mana(s) | CostPart::ManaRepeated { mana: s, .. } if !simple_mana(s) => Err(
            format!("mana cost {s} needs hybrid/Phyrexian/X payment (M3 B3)"),
        ),
        CostPart::ManaRepeated { times, .. } => quantity(times),
        CostPart::PayLife(q) => quantity(q),
        CostPart::Sacrifice(s) => set(s),
        CostPart::Discard(d) => discard(d),
        _ => Ok(()),
    })
}
