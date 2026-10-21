//! Card definitions: the **temporary M2 IR** (M2 plan, D5).
//!
//! Just enough to play lands, vanilla and French-vanilla creatures, and a few
//! simple spells. ADR-0007 (M3) replaces it with the real card IR. The engine
//! never looks at card names; behaviour comes only from these definitions.

use std::collections::BTreeMap;
use std::fmt;

use mtg_core::{
    CardType, Color, Colors, ManaCost, ManaSymbol, OracleId, PrintedStat, Supertype, TypeLine,
};
use mtg_data::CardRecord;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Index of a definition in a [`CardPool`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DefId(pub u32);

/// The six types of mana (CR 106.1a/106.1b): five colors plus colorless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ManaType {
    White,
    Blue,
    Black,
    Red,
    Green,
    Colorless,
}

impl ManaType {
    pub const ALL: [ManaType; 6] = [
        ManaType::White,
        ManaType::Blue,
        ManaType::Black,
        ManaType::Red,
        ManaType::Green,
        ManaType::Colorless,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn of_color(c: Color) -> Self {
        match c {
            Color::White => ManaType::White,
            Color::Blue => ManaType::Blue,
            Color::Black => ManaType::Black,
            Color::Red => ManaType::Red,
            Color::Green => ManaType::Green,
        }
    }

    pub fn symbol(self) -> &'static str {
        ["{W}", "{U}", "{B}", "{R}", "{G}", "{C}"][self.index()]
    }
}

impl fmt::Display for ManaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.symbol())
    }
}

/// Keyword abilities the M2 engine implements, as a bitset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Keywords(u16);

impl Keywords {
    /// CR 702.2
    pub const DEATHTOUCH: Keywords = Keywords(1 << 0);
    /// CR 702.3
    pub const DEFENDER: Keywords = Keywords(1 << 1);
    /// CR 702.4
    pub const DOUBLE_STRIKE: Keywords = Keywords(1 << 2);
    /// CR 702.7
    pub const FIRST_STRIKE: Keywords = Keywords(1 << 3);
    /// CR 702.9
    pub const FLYING: Keywords = Keywords(1 << 4);
    /// CR 702.10
    pub const HASTE: Keywords = Keywords(1 << 5);
    /// CR 702.15
    pub const LIFELINK: Keywords = Keywords(1 << 6);
    /// CR 702.17
    pub const REACH: Keywords = Keywords(1 << 7);
    /// CR 702.19
    pub const TRAMPLE: Keywords = Keywords(1 << 8);
    /// CR 702.20
    pub const VIGILANCE: Keywords = Keywords(1 << 9);
    /// CR 702.111
    pub const MENACE: Keywords = Keywords(1 << 10);

    const NAMES: [(&'static str, Keywords); 11] = [
        ("deathtouch", Self::DEATHTOUCH),
        ("defender", Self::DEFENDER),
        ("double strike", Self::DOUBLE_STRIKE),
        ("first strike", Self::FIRST_STRIKE),
        ("flying", Self::FLYING),
        ("haste", Self::HASTE),
        ("lifelink", Self::LIFELINK),
        ("reach", Self::REACH),
        ("trample", Self::TRAMPLE),
        ("vigilance", Self::VIGILANCE),
        ("menace", Self::MENACE),
    ];

    pub fn has(self, k: Keywords) -> bool {
        self.0 & k.0 == k.0
    }

    pub fn with(self, k: Keywords) -> Keywords {
        Keywords(self.0 | k.0)
    }

    pub fn from_name(name: &str) -> Option<Keywords> {
        Self::NAMES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, k)| *k)
    }

    pub fn names(self) -> impl Iterator<Item = &'static str> {
        Self::NAMES
            .iter()
            .filter(move |(_, k)| self.has(*k))
            .map(|(n, _)| *n)
    }
}

/// What a spell's target slot accepts (CR 115).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TargetSpec {
    /// "any target": a creature, player, planeswalker or battle (CR 115.4). M2 has creatures and players.
    Any,
    /// "target creature"
    Creature,
}

/// One-shot effects of instants and sorceries (M2 subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Effect {
    /// Deal N damage to the target (CR 120).
    DealDamage(u32),
    /// Destroy the target (CR 701.8).
    Destroy,
    /// The controller draws N cards (CR 121).
    Draw(u32),
    /// The controller gains N life (CR 119).
    GainLife(u32),
}

impl Effect {
    pub fn target(self) -> Option<TargetSpec> {
        match self {
            Effect::DealDamage(_) => Some(TargetSpec::Any),
            Effect::Destroy => Some(TargetSpec::Creature),
            Effect::Draw(_) | Effect::GainLife(_) => None,
        }
    }
}

/// A card definition in the M2 IR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDef {
    pub name: String,
    pub oracle_id: Option<OracleId>,
    /// `None` = no mana cost (CR 202.1b): can't be cast normally (lands aren't cast).
    pub mana_cost: Option<ManaCost>,
    pub type_line: TypeLine,
    pub colors: Colors,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub keywords: Keywords,
    /// `{T}: Add <type>` abilities, one entry per ability (CR 605.1a).
    /// Lands with basic land types have these intrinsically (CR 305.6).
    pub mana_abilities: SmallVec<[ManaType; 2]>,
    /// The effect of an instant or sorcery.
    pub spell_effect: Option<Effect>,
}

impl CardDef {
    pub fn is(&self, t: CardType) -> bool {
        self.type_line.has_type(t)
    }

    pub fn is_permanent_card(&self) -> bool {
        // CR 110.4: artifact, battle, creature, enchantment, land, planeswalker.
        [
            CardType::Artifact,
            CardType::Battle,
            CardType::Creature,
            CardType::Enchantment,
            CardType::Land,
            CardType::Planeswalker,
        ]
        .iter()
        .any(|t| self.is(*t))
    }

    pub fn is_legendary(&self) -> bool {
        self.type_line.has_supertype(Supertype::Legendary)
    }

    pub fn mana_value(&self) -> u32 {
        self.mana_cost.as_ref().map_or(0, ManaCost::mana_value)
    }
}

/// Why a card can't be expressed in the M2 IR.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{name}: not supported by the M2 card IR: {reason}")]
pub struct Unsupported {
    pub name: String,
    pub reason: String,
}

impl CardDef {
    /// Builds a definition from an ingested record when it is a basic-type
    /// land, a vanilla creature, or a French-vanilla creature whose rules text
    /// is only M2 keywords. Anything else is an error, never a blank card.
    pub fn from_record(rec: &CardRecord) -> Result<CardDef, Unsupported> {
        let unsupported = |reason: &str| Unsupported {
            name: rec.name.clone(),
            reason: reason.to_owned(),
        };
        if rec.faces.len() != 1 {
            return Err(unsupported("multi-face layouts arrive in M3"));
        }
        let f = rec.front();
        let t = &f.type_line;
        if !t.other.is_empty() {
            return Err(unsupported("non-standard type line"));
        }
        let simple = |s: &ManaSymbol| {
            matches!(
                s,
                ManaSymbol::Generic(_) | ManaSymbol::Colored(_) | ManaSymbol::Colorless
            )
        };
        if f.mana_cost
            .as_ref()
            .is_some_and(|c| !c.symbols().iter().all(simple))
        {
            return Err(unsupported(
                "mana cost needs hybrid/Phyrexian/X/snow payment (M3)",
            ));
        }
        let mut keywords = Keywords::default();
        let mut mana_abilities = SmallVec::new();
        for line in f.reminder_stripped.lines() {
            for part in line.split(',') {
                let word = part.trim().trim_end_matches('.');
                match Keywords::from_name(word) {
                    Some(k) => keywords = keywords.with(k),
                    None => return Err(unsupported(&format!("rules text {line:?}"))),
                }
            }
        }
        let is_land = t.has_type(CardType::Land);
        let is_creature = t.has_type(CardType::Creature);
        if is_land {
            if t.card_types.len() > 1 {
                return Err(unsupported("land with other card types"));
            }
            // CR 305.6: intrinsic mana abilities from basic land types.
            for (_, color) in t.basic_land_types() {
                mana_abilities.push(ManaType::of_color(color));
            }
            if mana_abilities.is_empty() && !f.reminder_stripped.is_empty() {
                return Err(unsupported("land rules text"));
            }
        } else if !is_creature || t.card_types.len() > 1 {
            return Err(unsupported("only lands and creatures convert in M2"));
        }
        let stat = |s: &Option<PrintedStat>| match s {
            Some(PrintedStat::Fixed(n)) => Ok(Some(*n)),
            None => Ok(None),
            Some(other) => Err(unsupported(&format!("printed stat {other}"))),
        };
        let power = stat(&f.power)?;
        let toughness = stat(&f.toughness)?;
        if is_creature && (power.is_none() || toughness.is_none()) {
            return Err(unsupported("creature without fixed power/toughness"));
        }
        Ok(CardDef {
            name: rec.name.clone(),
            oracle_id: Some(rec.oracle_id),
            mana_cost: f.mana_cost.clone(),
            type_line: t.clone(),
            colors: f.colors,
            power,
            toughness,
            keywords,
            mana_abilities,
            spell_effect: None,
        })
    }
}

/// An immutable set of definitions shared by every clone of a game.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardPool {
    defs: Vec<CardDef>,
    by_name: BTreeMap<String, DefId>,
}

impl CardPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a definition (or returns the existing id for that name).
    pub fn add(&mut self, def: CardDef) -> DefId {
        if let Some(id) = self.by_name.get(&def.name) {
            return *id;
        }
        let id = DefId(u32::try_from(self.defs.len()).unwrap_or(u32::MAX));
        self.by_name.insert(def.name.clone(), id);
        self.defs.push(def);
        id
    }

    pub fn get(&self, id: DefId) -> &CardDef {
        &self.defs[id.0 as usize]
    }

    pub fn by_name(&self, name: &str) -> Option<DefId> {
        self.by_name.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (DefId, &CardDef)> {
        self.defs
            .iter()
            .enumerate()
            .map(|(i, d)| (DefId(u32::try_from(i).unwrap_or(u32::MAX)), d))
    }

    /// The synthetic M2 test set: made-up cards covering every M2 rule.
    pub fn test_set() -> CardPool {
        let mut pool = CardPool::new();
        for (name, sub, color) in [
            ("Test Plains", "Plains", Color::White),
            ("Test Island", "Island", Color::Blue),
            ("Test Swamp", "Swamp", Color::Black),
            ("Test Mountain", "Mountain", Color::Red),
            ("Test Forest", "Forest", Color::Green),
        ] {
            pool.add(CardDef {
                name: name.into(),
                oracle_id: None,
                mana_cost: None,
                type_line: tl(&format!("Basic Land — {sub}")),
                colors: Colors::COLORLESS,
                power: None,
                toughness: None,
                keywords: Keywords::default(),
                mana_abilities: SmallVec::from_slice(&[ManaType::of_color(color)]),
                spell_effect: None,
            });
        }
        pool.add(CardDef {
            name: "Test Wastes".into(),
            oracle_id: None,
            mana_cost: None,
            type_line: tl("Land"),
            colors: Colors::COLORLESS,
            power: None,
            toughness: None,
            keywords: Keywords::default(),
            mana_abilities: SmallVec::from_slice(&[ManaType::Colorless]),
            spell_effect: None,
        });
        let creatures: [TestCreature; 16] = [
            ("Test Bear", "{1}{G}", "Creature — Bear", 2, 2, &[]),
            ("Test Ogre", "{2}{R}", "Creature — Ogre", 3, 2, &[]),
            (
                "Test Wall",
                "{1}{W}",
                "Creature — Wall",
                0,
                4,
                &[Keywords::DEFENDER],
            ),
            (
                "Test Bird",
                "{1}{U}",
                "Creature — Bird",
                1,
                1,
                &[Keywords::FLYING],
            ),
            (
                "Test Archer",
                "{1}{G}",
                "Creature — Elf Archer",
                1,
                2,
                &[Keywords::REACH],
            ),
            (
                "Test Knight",
                "{1}{W}{W}",
                "Creature — Human Knight",
                2,
                2,
                &[Keywords::FIRST_STRIKE],
            ),
            (
                "Test Duelist",
                "{2}{R}{W}",
                "Creature — Human Warrior",
                2,
                2,
                &[Keywords::DOUBLE_STRIKE],
            ),
            (
                "Test Assassin",
                "{1}{B}",
                "Creature — Human Assassin",
                1,
                1,
                &[Keywords::DEATHTOUCH],
            ),
            (
                "Test Vampire",
                "{1}{B}",
                "Creature — Vampire",
                2,
                1,
                &[Keywords::LIFELINK],
            ),
            (
                "Test Rhino",
                "{3}{G}",
                "Creature — Rhino",
                4,
                3,
                &[Keywords::TRAMPLE],
            ),
            (
                "Test Sentry",
                "{2}{W}",
                "Creature — Human Soldier",
                2,
                3,
                &[Keywords::VIGILANCE],
            ),
            (
                "Test Raider",
                "{1}{R}",
                "Creature — Goblin",
                2,
                1,
                &[Keywords::HASTE],
            ),
            (
                "Test Brute",
                "{2}{B}",
                "Creature — Ogre",
                3,
                2,
                &[Keywords::MENACE],
            ),
            (
                "Test Wurm",
                "{4}{G}{G}",
                "Creature — Wurm",
                6,
                6,
                &[Keywords::TRAMPLE],
            ),
            (
                "Test Drake",
                "{2}{U}",
                "Creature — Drake",
                2,
                2,
                &[Keywords::FLYING, Keywords::FIRST_STRIKE],
            ),
            ("Test Golem", "{3}", "Artifact Creature — Golem", 2, 2, &[]),
        ];
        for (name, cost, types, p, t, kws) in creatures {
            let cost: ManaCost = cost.parse().unwrap_or_default();
            pool.add(CardDef {
                name: name.into(),
                oracle_id: None,
                colors: cost.colors(),
                mana_cost: Some(cost),
                type_line: tl(types),
                power: Some(p),
                toughness: Some(t),
                keywords: kws.iter().fold(Keywords::default(), |a, k| a.with(*k)),
                mana_abilities: SmallVec::new(),
                spell_effect: None,
            });
        }
        pool.add(CardDef {
            name: "Test Elf".into(),
            oracle_id: None,
            mana_cost: Some("{G}".parse().unwrap_or_default()),
            type_line: tl("Creature — Elf Druid"),
            colors: Colors::single(Color::Green),
            power: Some(1),
            toughness: Some(1),
            keywords: Keywords::default(),
            mana_abilities: SmallVec::from_slice(&[ManaType::Green]),
            spell_effect: None,
        });
        pool.add(CardDef {
            name: "Test Champion".into(),
            oracle_id: None,
            mana_cost: Some("{2}{W}".parse().unwrap_or_default()),
            type_line: tl("Legendary Creature — Human Soldier"),
            colors: Colors::single(Color::White),
            power: Some(3),
            toughness: Some(3),
            keywords: Keywords::default(),
            mana_abilities: SmallVec::new(),
            spell_effect: None,
        });
        for (name, cost, types, effect) in [
            ("Test Bolt", "{R}", "Instant", Effect::DealDamage(3)),
            ("Test Blast", "{2}{R}", "Sorcery", Effect::DealDamage(5)),
            ("Test Murder", "{1}{B}{B}", "Instant", Effect::Destroy),
            ("Test Insight", "{1}{U}", "Instant", Effect::Draw(2)),
            ("Test Study", "{2}{U}", "Sorcery", Effect::Draw(3)),
            ("Test Renewal", "{W}", "Instant", Effect::GainLife(4)),
        ] {
            let cost: ManaCost = cost.parse().unwrap_or_default();
            pool.add(CardDef {
                name: name.into(),
                oracle_id: None,
                colors: cost.colors(),
                mana_cost: Some(cost),
                type_line: tl(types),
                power: None,
                toughness: None,
                keywords: Keywords::default(),
                mana_abilities: SmallVec::new(),
                spell_effect: Some(effect),
            });
        }
        pool
    }
}

/// (name, cost, type line, power, toughness, keywords)
type TestCreature = (
    &'static str,
    &'static str,
    &'static str,
    i32,
    i32,
    &'static [Keywords],
);

fn tl(s: &str) -> TypeLine {
    match s.parse() {
        Ok(t) => t,
        Err(never) => match never {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords() {
        let k = Keywords::FLYING.with(Keywords::FIRST_STRIKE);
        assert!(k.has(Keywords::FLYING) && !k.has(Keywords::REACH));
        assert_eq!(
            k.names().collect::<Vec<_>>(),
            vec!["first strike", "flying"]
        );
        assert_eq!(
            Keywords::from_name("Double Strike"),
            Some(Keywords::DOUBLE_STRIKE)
        );
    }

    #[test]
    fn test_set_is_consistent() {
        let pool = CardPool::test_set();
        assert!(pool.len() >= 30);
        let forest = pool.get(pool.by_name("Test Forest").unwrap_or(DefId(0)));
        assert_eq!(forest.mana_abilities.as_slice(), &[ManaType::Green]);
        let bolt = pool.get(pool.by_name("Test Bolt").unwrap_or(DefId(0)));
        assert_eq!(
            bolt.spell_effect.and_then(Effect::target),
            Some(TargetSpec::Any)
        );
        assert!(
            pool.iter()
                .all(|(_, d)| d.is(CardType::Land) || d.mana_cost.is_some())
        );
    }
}
