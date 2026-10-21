//! Card definitions: characteristics plus card IR v1 abilities (M3 plan, B-D1).
//!
//! The engine never looks at card names; behaviour comes only from the
//! abilities in [`CardDef::abilities`], which are `mtg_cards::ir` values
//! produced by any front-end. A definition is built only if the engine can
//! execute every ability on it ([`crate::support`], B-D2); anything else is
//! a loud [`Unsupported`] error, never a blank card.

use std::collections::BTreeMap;
use std::fmt;

use mtg_cards::ir::{
    self, Ability, ActivatedAbility, CostPart, FaceIr, ManaSpec, Quantity, SpellAbility,
    TargetKind, TargetSpec,
};
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

    /// This type as a one-bit [`ManaUnit`] mask.
    pub fn bit(self) -> ManaUnit {
        1 << self.index()
    }
}

impl fmt::Display for ManaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.symbol())
    }
}

/// One unit of mana an ability produces, as a mask of the [`ManaType`]s the
/// player may choose for it (bit = `ManaType::index`).
pub type ManaUnit = u8;

/// Any of the five colors (CR 105.1).
pub const ANY_COLOR: ManaUnit = 0b1_1111;

/// Keyword abilities the engine implements, as a bitset derived from the IR.
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
    /// CR 702.8
    pub const FLASH: Keywords = Keywords(1 << 11);
    /// CR 702.11
    pub const HEXPROOF: Keywords = Keywords(1 << 12);
    /// CR 702.18
    pub const SHROUD: Keywords = Keywords(1 << 13);
    /// CR 702.12
    pub const INDESTRUCTIBLE: Keywords = Keywords(1 << 14);

    const NAMES: [(&'static str, Keywords); 15] = [
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
        ("flash", Self::FLASH),
        ("hexproof", Self::HEXPROOF),
        ("shroud", Self::SHROUD),
        ("indestructible", Self::INDESTRUCTIBLE),
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

    /// The bit for an IR keyword, if it's one the bitset tracks.
    pub fn of_ir(k: &ir::Keyword) -> Option<Keywords> {
        use ir::Keyword as K;
        Some(match k {
            K::Deathtouch => Self::DEATHTOUCH,
            K::Defender => Self::DEFENDER,
            K::DoubleStrike => Self::DOUBLE_STRIKE,
            K::FirstStrike => Self::FIRST_STRIKE,
            K::Flying => Self::FLYING,
            K::Haste => Self::HASTE,
            K::Lifelink => Self::LIFELINK,
            K::Reach => Self::REACH,
            K::Trample => Self::TRAMPLE,
            K::Vigilance => Self::VIGILANCE,
            K::Menace => Self::MENACE,
            K::Flash => Self::FLASH,
            K::Hexproof => Self::HEXPROOF,
            K::Shroud => Self::SHROUD,
            K::Indestructible => Self::INDESTRUCTIBLE,
            _ => return None,
        })
    }

    /// The IR keyword for a single bit.
    pub fn to_ir(self) -> Option<ir::Keyword> {
        use ir::Keyword as K;
        let all = [
            (Self::DEATHTOUCH, K::Deathtouch),
            (Self::DEFENDER, K::Defender),
            (Self::DOUBLE_STRIKE, K::DoubleStrike),
            (Self::FIRST_STRIKE, K::FirstStrike),
            (Self::FLYING, K::Flying),
            (Self::HASTE, K::Haste),
            (Self::LIFELINK, K::Lifelink),
            (Self::REACH, K::Reach),
            (Self::TRAMPLE, K::Trample),
            (Self::VIGILANCE, K::Vigilance),
            (Self::MENACE, K::Menace),
            (Self::FLASH, K::Flash),
            (Self::HEXPROOF, K::Hexproof),
            (Self::SHROUD, K::Shroud),
            (Self::INDESTRUCTIBLE, K::Indestructible),
        ];
        all.into_iter().find(|(b, _)| *b == self).map(|(_, k)| k)
    }
}

/// A mana ability (CR 605.1a) of a definition, with its output precomputed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaAbility {
    /// Index into [`CardDef::abilities`].
    pub ability: u8,
    /// Ways the ability's mana can come out: one alternative is chosen per
    /// activation, and each unit of it may be any type in its mask.
    pub alternatives: SmallVec<[SmallVec<[ManaUnit; 2]>; 1]>,
    /// The cost is exactly {T}, so automatic payment plans may use it (B-D8).
    pub tap_only: bool,
}

impl ManaAbility {
    /// The single mana type this ability makes, if it always makes exactly
    /// one mana of one type.
    pub fn single_type(&self) -> Option<ManaType> {
        match self.alternatives.as_slice() {
            [alt] => match alt.as_slice() {
                [unit] if unit.count_ones() == 1 => {
                    ManaType::ALL.into_iter().find(|t| t.bit() == *unit)
                }
                _ => None,
            },
            _ => None,
        }
    }
}

/// Printed characteristics (CR 109.3) other than abilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Characteristics {
    pub name: String,
    pub oracle_id: Option<OracleId>,
    /// `None` = no mana cost (CR 202.1b): can't be cast normally (lands aren't cast).
    pub mana_cost: Option<ManaCost>,
    pub type_line: TypeLine,
    pub colors: Colors,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
}

/// A card definition: characteristics plus IR abilities, with derived indexes.
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
    /// Abilities in printed order, then intrinsic ones (CR 305.6).
    pub abilities: Vec<Ability>,
    /// Derived: the keyword bitset, for fast combat checks.
    pub keywords: Keywords,
    /// Derived: the mana abilities (CR 605.1a), in ability order.
    pub mana_abilities: SmallVec<[ManaAbility; 2]>,
    /// Derived: for a modal spell, a bitmask per mode of the target slots it
    /// refers to (CR 601.2c: only the chosen modes' targets are required).
    pub mode_slots: SmallVec<[u32; 4]>,
}

/// Bitmask of the target slots an effect refers to.
pub(crate) fn target_slots(e: &ir::Effect) -> u32 {
    fn walk(v: &serde_json::Value, mask: &mut u32) {
        match v {
            serde_json::Value::Object(m) => {
                if m.len() == 1
                    && let Some(n) = m.get("target").and_then(serde_json::Value::as_u64)
                    && n < 32
                {
                    *mask |= 1 << n;
                }
                for x in m.values() {
                    walk(x, mask);
                }
            }
            serde_json::Value::Array(a) => {
                for x in a {
                    walk(x, mask);
                }
            }
            _ => {}
        }
    }
    let mut mask = 0;
    if let Ok(v) = serde_json::to_value(e) {
        walk(&v, &mut mask);
    }
    mask
}

impl CardDef {
    /// Builds a definition and checks that the engine supports all of it (B-D2).
    pub fn new(ch: Characteristics, mut abilities: Vec<Ability>) -> Result<CardDef, Unsupported> {
        let unsupported = |reason: String| Unsupported {
            name: ch.name.clone(),
            reason,
        };
        // CR 305.6: lands with basic land types have the matching mana abilities.
        if ch.type_line.has_type(CardType::Land) {
            for (_, color) in ch.type_line.basic_land_types() {
                abilities.push(tap_for(ManaType::of_color(color)));
            }
        }
        let mut keywords = Keywords::default();
        let mut mana_abilities = SmallVec::new();
        for (i, a) in abilities.iter().enumerate() {
            match a {
                Ability::Keyword(k) => {
                    if let Some(bit) = Keywords::of_ir(k) {
                        keywords = keywords.with(bit);
                    }
                }
                Ability::Activated(x) if is_mana_ability(x) => {
                    let alternatives = ability_mana_output(&x.effect).map_err(unsupported)?;
                    mana_abilities.push(ManaAbility {
                        ability: u8::try_from(i)
                            .map_err(|_| unsupported("more than 255 abilities".into()))?,
                        alternatives,
                        tap_only: x.cost == [CostPart::Tap],
                    });
                }
                _ => {}
            }
        }
        let mut mode_slots = SmallVec::new();
        for a in &abilities {
            if let Ability::Spell(s) = a
                && let ir::Effect::Modal { modes, .. } = &s.effect
            {
                mode_slots.extend(modes.iter().map(target_slots));
            }
        }
        let def = CardDef {
            name: ch.name,
            oracle_id: ch.oracle_id,
            mana_cost: ch.mana_cost,
            type_line: ch.type_line,
            colors: ch.colors,
            power: ch.power,
            toughness: ch.toughness,
            abilities,
            keywords,
            mana_abilities,
            mode_slots,
        };
        crate::support::check(&def).map_err(|reason| Unsupported {
            name: def.name.clone(),
            reason,
        })?;
        Ok(def)
    }

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

    /// "This spell can't be countered."
    pub fn cant_be_countered(&self) -> bool {
        self.abilities.iter().any(|a| {
            matches!(
                a,
                Ability::Static(ir::StaticAbility {
                    effect: ir::StaticEffect::Rule(ir::RuleMod::CantBeCountered),
                    ..
                })
            )
        })
    }

    /// The spell ability of an instant or sorcery (CR 113.3a).
    pub fn spell(&self) -> Option<&SpellAbility> {
        self.abilities.iter().find_map(|a| match a {
            Ability::Spell(s) => Some(s),
            _ => None,
        })
    }
}

/// CR 605.1a: an activated ability is a mana ability if it has no targets and
/// could add mana (loyalty abilities aren't in the IR yet).
pub fn is_mana_ability(a: &ActivatedAbility) -> bool {
    a.targets.is_empty() && could_add_mana(&a.effect)
}

fn could_add_mana(e: &ir::Effect) -> bool {
    use ir::Effect as E;
    match e {
        E::AddMana { .. } => true,
        E::Seq(v) => v.iter().any(could_add_mana),
        E::May(x) => could_add_mana(x),
        _ => false,
    }
}

/// Ways the mana of a mana ability or spell can come out.
pub type ManaAlternatives = SmallVec<[SmallVec<[ManaUnit; 2]>; 1]>;

/// The output of a mana ability's effect. Only a single unconditional
/// `AddMana` is supported so far.
fn ability_mana_output(e: &ir::Effect) -> Result<ManaAlternatives, String> {
    use ir::Effect as E;
    let adds: Vec<&ManaSpec> = match e {
        E::AddMana { mana, .. } => vec![mana],
        E::Seq(v) => v
            .iter()
            .filter_map(|x| match x {
                E::AddMana { mana, .. } => Some(mana),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let [spec] = adds.as_slice() else {
        return Err("mana ability that doesn't add mana exactly once, unconditionally".into());
    };
    mana_output(spec)
}

/// The alternatives a mana spec produces, for fixed amounts of 1 to 16 mana.
pub(crate) fn mana_output(spec: &ManaSpec) -> Result<ManaAlternatives, String> {
    let fixed = |q: &Quantity| match q {
        Quantity::Fixed(n) if (1..=16).contains(n) => usize::try_from(*n).ok(),
        _ => None,
    };
    let mut out = SmallVec::new();
    match spec {
        ManaSpec::Fixed(s) => out.push(parse_mana_units(s)?),
        ManaSpec::OneOf(v) => {
            for s in v {
                out.push(parse_mana_units(s)?);
            }
        }
        ManaSpec::AnyColor(q) => {
            let n = fixed(q).ok_or("variable amount of mana")?;
            out.push(SmallVec::from_elem(ANY_COLOR, n));
        }
        ManaSpec::AnyOneColor(q) => {
            let n = fixed(q).ok_or("variable amount of mana")?;
            for c in Color::ALL {
                out.push(SmallVec::from_elem(ManaType::of_color(c).bit(), n));
            }
        }
        ManaSpec::CommanderIdentity => {
            return Err(
                "mana of the commander's color identity needs the command zone (M6)".into(),
            );
        }
        ManaSpec::ColorOf(_) | ManaSpec::TypeProducedByTrigger => {
            return Err("mana whose type depends on another object (M4)".into());
        }
    }
    Ok(out)
}

/// Parses produced mana such as `{C}{C}` or `{B}{B}{B}` into units.
fn parse_mana_units(s: &str) -> Result<SmallVec<[ManaUnit; 2]>, String> {
    let cost: ManaCost = s.parse().map_err(|e| format!("mana {s:?}: {e}"))?;
    cost.symbols()
        .iter()
        .map(|sym| match sym {
            ManaSymbol::Colored(c) => Ok(ManaType::of_color(*c).bit()),
            ManaSymbol::Colorless => Ok(ManaType::Colorless.bit()),
            other => Err(format!("can't produce mana symbol {other:?}")),
        })
        .collect()
}

/// Mana symbols the engine can pay (CR 107.4): everything except snow and
/// non-tournament symbols.
pub(crate) fn payable_symbol(s: &ManaSymbol) -> bool {
    !matches!(s, ManaSymbol::Snow | ManaSymbol::Unsupported(_))
        && !matches!(s, ManaSymbol::Variable(c) if *c != 'X')
}

/// `{T}: Add <type>.`
pub fn tap_for(t: ManaType) -> Ability {
    Ability::Activated(ActivatedAbility {
        cost: vec![CostPart::Tap],
        targets: Vec::new(),
        effect: ir::Effect::AddMana {
            mana: ManaSpec::Fixed(t.symbol().to_owned()),
            spend_only_on: None,
        },
        timing: ir::Timing::Any,
        once_per_turn: false,
    })
}

/// Why a card can't be used by the engine yet.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{name}: not supported by the engine yet: {reason}")]
pub struct Unsupported {
    pub name: String,
    pub reason: String,
}

impl Characteristics {
    /// The front face's characteristics from an ingested record.
    pub fn from_record(rec: &CardRecord) -> Result<Characteristics, Unsupported> {
        let unsupported = |reason: &str| Unsupported {
            name: rec.name.clone(),
            reason: reason.to_owned(),
        };
        if rec.faces.len() != 1 {
            return Err(unsupported("multi-face layouts"));
        }
        let f = rec.front();
        if !f.type_line.other.is_empty() {
            return Err(unsupported("non-standard type line"));
        }
        if f.mana_cost
            .as_ref()
            .is_some_and(|c| !c.symbols().iter().all(payable_symbol))
        {
            return Err(unsupported("mana cost with snow or non-tournament symbols"));
        }
        let stat = |s: &Option<PrintedStat>| match s {
            Some(PrintedStat::Fixed(n)) => Ok(Some(*n)),
            None => Ok(None),
            Some(other) => Err(unsupported(&format!("printed stat {other}"))),
        };
        let power = stat(&f.power)?;
        let toughness = stat(&f.toughness)?;
        if f.type_line.has_type(CardType::Creature) && (power.is_none() || toughness.is_none()) {
            return Err(unsupported("creature without fixed power/toughness"));
        }
        Ok(Characteristics {
            name: rec.name.clone(),
            oracle_id: Some(rec.oracle_id),
            mana_cost: f.mana_cost.clone(),
            type_line: f.type_line.clone(),
            colors: f.colors,
            power,
            toughness,
        })
    }
}

impl CardDef {
    /// Builds a definition from an ingested record whose rules text is only
    /// keywords the engine knows: basic-type lands, vanilla and French-vanilla
    /// creatures. Cards with other text need IR from a front-end
    /// ([`CardDef::from_record_ir`]).
    pub fn from_record(rec: &CardRecord) -> Result<CardDef, Unsupported> {
        let unsupported = |reason: &str| Unsupported {
            name: rec.name.clone(),
            reason: reason.to_owned(),
        };
        let ch = Characteristics::from_record(rec)?;
        let t = &ch.type_line;
        let is_land = t.has_type(CardType::Land);
        if is_land {
            if t.card_types.len() > 1 {
                return Err(unsupported("land with other card types"));
            }
            if t.basic_land_types().next().is_none() {
                return Err(unsupported("land without a basic land type"));
            }
        } else if !t.has_type(CardType::Creature) || t.card_types.len() > 1 {
            return Err(unsupported("only lands and creatures convert without IR"));
        }
        let mut abilities = Vec::new();
        for line in rec.front().reminder_stripped.lines() {
            for part in line.split(',') {
                let word = part.trim().trim_end_matches('.');
                match Keywords::from_name(word).and_then(Keywords::to_ir) {
                    Some(k) if !is_land => abilities.push(Ability::Keyword(k)),
                    _ => return Err(unsupported(&format!("rules text {line:?}"))),
                }
            }
        }
        CardDef::new(ch, abilities)
    }

    /// Builds a definition from an ingested record and IR for its front face.
    pub fn from_record_ir(rec: &CardRecord, face: &FaceIr) -> Result<CardDef, Unsupported> {
        CardDef::new(Characteristics::from_record(rec)?, face.abilities.clone())
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

    /// Adds a synthetic definition, panicking if the engine can't support it
    /// (test sets are code, so this is a programming error).
    fn add_synthetic(&mut self, ch: Characteristics, abilities: Vec<Ability>) -> DefId {
        match CardDef::new(ch, abilities) {
            Ok(def) => self.add(def),
            Err(e) => panic!("synthetic test card: {e}"),
        }
    }

    /// The synthetic M2 test set: made-up cards covering every M2 rule. Its
    /// contents and order are frozen because the M2 golden replays use it.
    pub fn test_set() -> CardPool {
        let mut pool = CardPool::new();
        for (name, sub) in [
            ("Test Plains", "Plains"),
            ("Test Island", "Island"),
            ("Test Swamp", "Swamp"),
            ("Test Mountain", "Mountain"),
            ("Test Forest", "Forest"),
        ] {
            pool.add_synthetic(
                synthetic(name, None, &format!("Basic Land — {sub}"), None),
                Vec::new(),
            );
        }
        pool.add_synthetic(
            synthetic("Test Wastes", None, "Land", None),
            vec![tap_for(ManaType::Colorless)],
        );
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
            pool.add_synthetic(
                synthetic(name, Some(cost), types, Some((p, t))),
                kws.iter()
                    .filter_map(|k| k.to_ir())
                    .map(Ability::Keyword)
                    .collect(),
            );
        }
        pool.add_synthetic(
            synthetic(
                "Test Elf",
                Some("{G}"),
                "Creature — Elf Druid",
                Some((1, 1)),
            ),
            vec![tap_for(ManaType::Green)],
        );
        pool.add_synthetic(
            synthetic(
                "Test Champion",
                Some("{2}{W}"),
                "Legendary Creature — Human Soldier",
                Some((3, 3)),
            ),
            Vec::new(),
        );
        for (name, cost, types, spell) in [
            ("Test Bolt", "{R}", "Instant", damage_any(3)),
            ("Test Blast", "{2}{R}", "Sorcery", damage_any(5)),
            ("Test Murder", "{1}{B}{B}", "Instant", destroy_creature()),
            ("Test Insight", "{1}{U}", "Instant", draw(2)),
            ("Test Study", "{2}{U}", "Sorcery", draw(3)),
            ("Test Renewal", "{W}", "Instant", gain_life(4)),
        ] {
            pool.add_synthetic(
                synthetic(name, Some(cost), types, None),
                vec![Ability::Spell(spell)],
            );
        }
        pool
    }
}

impl CardPool {
    /// The M2 test set plus the made-up cards in
    /// `tests/fixtures/synthetic-ir-cards.yaml`, which exercise the M3 effect
    /// interpreter. M2 definitions keep their ids.
    pub fn m3_test_set() -> CardPool {
        let mut pool = CardPool::test_set();
        let text = include_str!("../../../tests/fixtures/synthetic-ir-cards.yaml");
        let cards = mtg_cards::defs::parse_synthetic(text, "synthetic-ir-cards.yaml")
            .unwrap_or_else(|e| panic!("{e}"));
        for c in cards {
            let ch = synthetic(
                &c.name,
                c.cost.as_deref(),
                &c.types,
                c.pt.map(|[p, t]| (p, t)),
            );
            pool.add_synthetic(ch, c.abilities);
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

/// Characteristics of a made-up card. Colors come from the cost (CR 202.2).
pub fn synthetic(
    name: &str,
    cost: Option<&str>,
    types: &str,
    pt: Option<(i32, i32)>,
) -> Characteristics {
    let cost: Option<ManaCost> = cost.map(|c| c.parse().unwrap_or_default());
    Characteristics {
        name: name.into(),
        oracle_id: None,
        colors: cost.as_ref().map_or(Colors::COLORLESS, ManaCost::colors),
        mana_cost: cost,
        type_line: tl(types),
        power: pt.map(|(p, _)| p),
        toughness: pt.map(|(_, t)| t),
    }
}

fn spell(targets: Vec<TargetSpec>, effect: ir::Effect) -> SpellAbility {
    SpellAbility {
        targets,
        effect,
        alternative_costs: Vec::new(),
        additional_costs: Vec::new(),
    }
}

fn one_target(what: TargetKind) -> Vec<TargetSpec> {
    vec![TargetSpec { what, up_to: None }]
}

/// "~ deals N damage to any target."
fn damage_any(n: i64) -> SpellAbility {
    spell(
        one_target(TargetKind::Any),
        ir::Effect::DealDamage {
            to: ir::Recipient::Target(0),
            amount: Quantity::Fixed(n),
        },
    )
}

/// "Destroy target creature."
fn destroy_creature() -> SpellAbility {
    spell(
        one_target(TargetKind::Object(ir::ObjectFilter {
            types: vec!["creature".into()],
            ..Default::default()
        })),
        ir::Effect::Destroy(ir::ObjectSet::One(ir::ObjectRef::Target(0))),
    )
}

/// "Draw N cards."
fn draw(n: i64) -> SpellAbility {
    spell(
        Vec::new(),
        ir::Effect::Draw {
            who: ir::PlayerRef::You,
            amount: Quantity::Fixed(n),
        },
    )
}

/// "You gain N life."
fn gain_life(n: i64) -> SpellAbility {
    spell(
        Vec::new(),
        ir::Effect::GainLife {
            who: ir::PlayerRef::You,
            amount: Quantity::Fixed(n),
        },
    )
}

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
        for (_, bit) in Keywords::NAMES {
            let k = bit.to_ir().unwrap_or(ir::Keyword::Partner);
            assert_eq!(Keywords::of_ir(&k), Some(bit));
        }
    }

    #[test]
    fn test_set_is_consistent() {
        let pool = CardPool::test_set();
        assert!(pool.len() >= 30);
        let forest = pool.get(pool.by_name("Test Forest").unwrap_or(DefId(0)));
        assert_eq!(forest.mana_abilities.len(), 1);
        assert_eq!(
            forest.mana_abilities[0].single_type(),
            Some(ManaType::Green)
        );
        assert!(forest.mana_abilities[0].tap_only);
        let bolt = pool.get(pool.by_name("Test Bolt").unwrap_or(DefId(0)));
        assert_eq!(
            bolt.spell().map(|s| s.targets[0].what.clone()),
            Some(TargetKind::Any)
        );
        assert!(
            pool.iter()
                .all(|(_, d)| d.is(CardType::Land) || d.mana_cost.is_some())
        );
    }

    #[test]
    fn mana_outputs() {
        let units = |spec: ManaSpec| mana_output(&spec);
        let sol = units(ManaSpec::Fixed("{C}{C}".into())).unwrap_or_default();
        assert_eq!(sol.len(), 1);
        assert_eq!(sol[0].as_slice(), &[ManaType::Colorless.bit(); 2]);
        let birds = units(ManaSpec::AnyColor(Quantity::Fixed(1))).unwrap_or_default();
        assert_eq!(birds[0].as_slice(), &[ANY_COLOR]);
        let lotus = units(ManaSpec::AnyOneColor(Quantity::Fixed(3))).unwrap_or_default();
        assert_eq!(lotus.len(), 5);
        assert!(units(ManaSpec::CommanderIdentity).is_err());
    }
}
