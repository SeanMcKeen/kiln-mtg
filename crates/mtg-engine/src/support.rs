//! What the engine can execute (M3 plan, B-D2).
//!
//! [`check`] walks a definition's IR and rejects anything the engine can't
//! run yet, with a reason that names the missing feature and the milestone
//! that adds it. Each M3 phase widens it as features land; a card is never
//! partially supported.

use mtg_cards::ir::{
    Ability, ChoiceSpec, Condition, CostPart, DiscardSpec, Effect, ObjectRef, ObjectSet, PlayerRef,
    Quantity, QuantityExpr, Recipient, RuleMod, SpellAbility, StaticAbility, StaticEffect, Timing,
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
    let instant_or_sorcery = def.is(CardType::Instant) || def.is(CardType::Sorcery);
    let mut spells = 0;
    for a in &def.abilities {
        match a {
            // CR 702.124a: partner only modifies deck construction; it does nothing in play.
            Ability::Keyword(mtg_cards::ir::Keyword::Partner) => {}
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
                if x.timing != Timing::Any || x.once_per_turn {
                    return Err("mana abilities with timing restrictions".into());
                }
                cast_cost(&x.cost)?;
                effect(&x.effect)?;
            }
            Ability::Activated(x) => {
                if matches!(x.effect, Effect::Modal { .. }) {
                    return Err("modal activated abilities".into());
                }
                if x.targets.iter().any(|t| t.up_to == Some(0)) {
                    return Err("a target slot that allows zero targets".into());
                }
                cast_cost(&x.cost)?;
                effect(&x.effect)?;
            }
            Ability::Triggered(_) => return Err("triggered abilities (M4)".into()),
            Ability::Static(StaticAbility {
                effect: StaticEffect::Rule(RuleMod::CantBeCountered),
                condition: None,
            }) => {}
            Ability::Static(_) => return Err("static abilities (M5)".into()),
            Ability::Replacement(_) => return Err("replacement effects (M4)".into()),
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
        K::Ward(_) => "keyword ward (M4: triggered abilities)".into(),
        K::Protection(_) => "keyword protection (M4: damage prevention)".into(),
        K::CumulativeUpkeep(_) => "keyword cumulative upkeep (M4: triggered abilities)".into(),
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
        Effect::Exile {
            what, face_down, ..
        } => {
            if *face_down {
                return Err("face-down exile (M5)".into());
            }
            set(what)
        }
        Effect::Move { what, .. }
        | Effect::Pump { what, .. }
        | Effect::Tap(what)
        | Effect::Untap(what) => set(what),
        Effect::RevealUntil { who, .. } | Effect::Shuffle(who) => player(who),
        Effect::Sacrifice { who, what } => {
            player(who)?;
            set(what)
        }
        Effect::Counter(ObjectSet::One(ObjectRef::Target(_))) => Ok(()),
        Effect::Counter(_) => Err("countering anything but a targeted spell".into()),
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
        } => {
            if spend_only_on.is_some() {
                return Err("mana with spending restrictions (M5)".into());
            }
            crate::card::mana_output(mana).map(|_| ())
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
        Effect::EntersTapped => Err("\"enters tapped\" is a replacement effect (M4)".into()),
        Effect::CreateToken {
            token,
            count,
            controller,
        } => {
            crate::card::token_def(token).map_err(|e| e.reason)?;
            player(controller)?;
            quantity(count)
        }
        Effect::PutCounters { what, amount, .. } | Effect::RemoveCounters { what, amount, .. } => {
            set(what)?;
            quantity(amount)
        }
        Effect::Amass { .. } => Err("amass (later)".into()),
        Effect::Delayed { .. } => Err("delayed triggered abilities (M4)".into()),
        Effect::Escape(x) => match crate::escapes::lookup(&x.function) {
            Some(_) => Ok(()),
            None => Err(format!("escape function {:?} isn't registered", x.function)),
        },
    }
}

fn object(o: &ObjectRef) -> Check {
    match o {
        ObjectRef::Triggering => Err("references to a trigger's object (M4)".into()),
        ObjectRef::This | ObjectRef::Target(_) | ObjectRef::Remembered(_) => Ok(()),
    }
}

fn player(p: &PlayerRef) -> Check {
    match p {
        PlayerRef::Triggering => Err("references to a trigger's player (M4)".into()),
        PlayerRef::ControllerOf(o) | PlayerRef::OwnerOf(o) => object(o),
        PlayerRef::You | PlayerRef::Target(_) | PlayerRef::Each => Ok(()),
    }
}

fn set(s: &ObjectSet) -> Check {
    match s {
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
            QuantityExpr::CardsInLibrary(p) => player(p),
            QuantityExpr::Plus(q, _) => quantity(q),
            QuantityExpr::OpponentsDealtCombatDamageThisTurn => {
                Err("combat damage history (M4)".into())
            }
            QuantityExpr::X
            | QuantityExpr::Count(_)
            | QuantityExpr::Devotion(_)
            | QuantityExpr::MostDiscardedThisWay
            | QuantityExpr::Var(_) => Ok(()),
        },
    }
}

fn condition(c: &Condition) -> Check {
    match c {
        Condition::ControlCommander => Err("commanders (M6)".into()),
        Condition::AtLeast(a, b) => {
            quantity(a)?;
            quantity(b)
        }
        Condition::Not(c) => condition(c),
        Condition::YourTurn | Condition::ThisTapped => Ok(()),
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
        CostPart::Mana(s) if !simple_mana(s) => Err(format!(
            "mana cost {s} needs hybrid/Phyrexian/X payment (M3 B3)"
        )),
        CostPart::PayLife(q) => quantity(q),
        CostPart::Sacrifice(s) => set(s),
        CostPart::Discard(d) => discard(d),
        _ => Ok(()),
    })
}
