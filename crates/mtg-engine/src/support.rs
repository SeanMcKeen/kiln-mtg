//! What the engine can execute (M3 plan, B-D2).
//!
//! [`check`] walks a definition's IR and rejects anything the engine can't
//! run yet, with a reason that names the missing feature and the milestone
//! that adds it. Each M3 phase widens it as features land; a card is never
//! partially supported.

use mtg_cards::ir::{
    Ability, ChoiceSpec, Condition, CostPart, DiscardSpec, Effect, ObjectRef, ObjectSet, PlayerRef,
    Quantity, QuantityExpr, Recipient, SpellAbility, TargetKind, Timing,
};
use mtg_core::CardType;

use crate::card::{CardDef, Keywords, is_mana_ability};
use crate::effects::simple_mana;

/// Keywords whose rules the engine implements.
const IMPLEMENTED_KEYWORDS: [Keywords; 11] = [
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
            Ability::Keyword(k) => match Keywords::of_ir(k) {
                Some(bit) if IMPLEMENTED_KEYWORDS.contains(&bit) => {}
                _ => return Err(format!("keyword {k:?} isn't implemented yet")),
            },
            Ability::Spell(s) => {
                spells += 1;
                spell(s)?;
            }
            Ability::Activated(x) if is_mana_ability(x) => {
                if x.cost != [CostPart::Tap] || x.timing != Timing::Any || x.once_per_turn {
                    return Err("mana abilities with costs other than {T} (M3 B4)".into());
                }
                let simple = matches!(
                    &x.effect,
                    Effect::AddMana {
                        spend_only_on: None,
                        ..
                    }
                );
                if !simple || !def.mana_abilities.iter().all(|m| m.single_type().is_some()) {
                    return Err("mana abilities that make more than one fixed mana (M3 B4)".into());
                }
            }
            Ability::Activated(_) => return Err("non-mana activated abilities (M3 B4)".into()),
            Ability::Triggered(_) => return Err("triggered abilities (M4)".into()),
            Ability::Static(_) => return Err("static abilities (M5)".into()),
            Ability::Replacement(_) => return Err("replacement effects (M4)".into()),
            Ability::Escape(e) => return Err(format!("escape function {:?}", e.function)),
        }
    }
    match (instant_or_sorcery, spells) {
        (true, 1) | (false, 0) => Ok(()),
        (true, _) => Err("an instant or sorcery needs exactly one spell ability".into()),
        (false, _) => Err("only instants and sorceries have spell abilities".into()),
    }
}

fn spell(s: &SpellAbility) -> Check {
    if !s.alternative_costs.is_empty() || !s.additional_costs.is_empty() {
        return Err("alternative and additional costs (M3 B3)".into());
    }
    match s.targets.as_slice() {
        [] => {}
        [t] => {
            if t.up_to.is_some() {
                return Err("\"up to N\" targets (M3 B3)".into());
            }
            if matches!(t.what, TargetKind::Spell(_)) {
                return Err("spell targets (M3 B3)".into());
            }
        }
        _ => return Err("more than one target slot (M3 B3)".into()),
    }
    effect(&s.effect)
}

fn effect(e: &Effect) -> Check {
    match e {
        Effect::Seq(v) => v.iter().try_for_each(effect),
        Effect::Modal { .. } => Err("modal spells (M3 B3)".into()),
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
        Effect::Counter(_) => Err("countering needs spell targets (M3 B3)".into()),
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
        Effect::CreateToken { .. } => Err("tokens (M3 B6)".into()),
        Effect::Amass { .. } => Err("amass (later)".into()),
        Effect::Delayed { .. } => Err("delayed triggered abilities (M4)".into()),
        Effect::Escape(x) => Err(format!("escape function {:?}", x.function)),
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
            QuantityExpr::PowerOf(o) | QuantityExpr::ManaValueOf(o) => object(o),
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
