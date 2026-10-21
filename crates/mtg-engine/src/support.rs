//! What the engine can execute (M3 plan, B-D2).
//!
//! [`check`] walks a definition's IR and rejects anything the engine can't
//! run yet, with a reason that names the missing feature. Each M3 phase widens
//! it as features land; a card is never partially supported.

use mtg_cards::ir::{
    Ability, CostPart, Effect, ManaSpec, ObjectRef, ObjectSet, PlayerRef, Quantity, Recipient,
    SpellAbility, TargetKind, Timing,
};
use mtg_core::CardType;

use crate::card::{CardDef, Keywords, is_mana_ability};

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

/// `Ok` if the engine can execute every ability of `def`.
pub(crate) fn check(def: &CardDef) -> Result<(), String> {
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
                        mana: ManaSpec::Fixed(_),
                        spend_only_on: None
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

fn spell(s: &SpellAbility) -> Result<(), String> {
    if !s.alternative_costs.is_empty() || !s.additional_costs.is_empty() {
        return Err("alternative and additional costs (M3 B3)".into());
    }
    let creature_target = |k: &TargetKind| match k {
        TargetKind::Object(f) => {
            *f == mtg_cards::ir::ObjectFilter {
                types: vec!["creature".into()],
                ..Default::default()
            }
        }
        _ => false,
    };
    let fixed = |q: &Quantity| matches!(q, Quantity::Fixed(n) if *n >= 0);
    let ok = match (s.targets.as_slice(), &s.effect) {
        (
            [t],
            Effect::DealDamage {
                to: Recipient::Target(0),
                amount,
            },
        ) => t.up_to.is_none() && t.what == TargetKind::Any && fixed(amount),
        ([t], Effect::Destroy(ObjectSet::One(ObjectRef::Target(0)))) => {
            t.up_to.is_none() && creature_target(&t.what)
        }
        (
            [],
            Effect::Draw {
                who: PlayerRef::You,
                amount,
            }
            | Effect::GainLife {
                who: PlayerRef::You,
                amount,
            },
        ) => fixed(amount),
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err("spell effects beyond the M2 set (M3 B2)".into())
    }
}
