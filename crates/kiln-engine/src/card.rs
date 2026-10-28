//! Card definitions: characteristics plus card IR v1 abilities (M3 plan, B-D1).
//!
//! The engine never looks at card names; behaviour comes only from the
//! abilities in [`CardDef::abilities`], which are `kiln_cards::ir` values
//! produced by any front-end. A definition is built only if the engine can
//! execute every ability on it ([`crate::support`], B-D2); anything else is
//! a loud [`Unsupported`] error, never a blank card.

use std::collections::BTreeMap;
use std::fmt;

use kiln_cards::ir::{
    self, Ability, ActivatedAbility, CostPart, FaceIr, ManaSpec, Quantity, SpellAbility,
    TargetKind, TargetSpec,
};
use kiln_data::CardRecord;
use kiln_types::{
    CardType, Color, Colors, ManaCost, ManaSymbol, OracleId, PrintedStat, Supertype, TypeLine,
};
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
pub struct Keywords(u32);

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
    /// CR 702.11d: hexproof from each color (WUBRG order).
    pub const HEXPROOF_FROM: [Keywords; 5] = [
        Keywords(1 << 15),
        Keywords(1 << 16),
        Keywords(1 << 17),
        Keywords(1 << 18),
        Keywords(1 << 19),
    ];
    /// CR 509.1b: "can't be blocked" (an evasion restriction, not a keyword).
    pub const CANT_BE_BLOCKED: Keywords = Keywords(1 << 20);

    const NAMES: [(&'static str, Keywords); 16] = [
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
        ("can't be blocked", Self::CANT_BE_BLOCKED),
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
            K::CantBeBlocked => Self::CANT_BE_BLOCKED,
            K::HexproofFrom(colors) => Color::ALL
                .iter()
                .enumerate()
                .filter(|(_, c)| colors.contains(**c))
                .fold(Keywords::default(), |k, (i, _)| {
                    k.with(Self::HEXPROOF_FROM[i])
                }),
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
            (Self::CANT_BE_BLOCKED, K::CantBeBlocked),
        ];
        all.into_iter().find(|(b, _)| *b == self).map(|(_, k)| k)
    }
}

/// Mana by [`ManaType`] index (W U B R G C).
pub type ManaBag = [u8; 6];

/// A mana ability (CR 605.1a) of a definition, with its output precomputed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaAbility {
    /// Index into [`CardDef::abilities`].
    pub ability: u8,
    /// Every distinct outcome of one activation, in WUBRG order.
    pub bags: SmallVec<[ManaBag; 1]>,
    /// The cost is exactly {T} and the effect needs no choices besides the
    /// mana, so automatic payment plans and payability checks may use it (B-D8).
    pub tap_only: bool,
    /// The output depends on the game (the commander's color identity), so
    /// `bags` is a placeholder and the game computes it when asked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dynamic: bool,
}

impl ManaAbility {
    /// The single mana type this ability makes, if it always makes exactly
    /// one mana of one type.
    pub fn single_type(&self) -> Option<ManaType> {
        match self.bags.as_slice() {
            [bag] if bag.iter().sum::<u8>() == 1 => {
                ManaType::ALL.into_iter().find(|t| bag[t.index()] == 1)
            }
            _ => None,
        }
    }

    /// Whether every outcome is exactly one mana.
    pub fn single_unit(&self) -> bool {
        self.bags.iter().all(|b| b.iter().sum::<u8>() == 1)
    }
}

/// Distinct outcome bags of mana alternatives, in WUBRG order.
pub(crate) fn expand_bags(alternatives: &ManaAlternatives) -> SmallVec<[ManaBag; 1]> {
    let mut out: Vec<ManaBag> = Vec::new();
    for alt in alternatives {
        let mut bags: Vec<ManaBag> = vec![[0; 6]];
        for unit in alt {
            let mut next = Vec::new();
            for bag in &bags {
                for t in ManaType::ALL {
                    if unit & t.bit() != 0 {
                        let mut b = *bag;
                        b[t.index()] += 1;
                        next.push(b);
                    }
                }
            }
            next.sort_unstable();
            next.dedup();
            bags = next;
        }
        out.extend(bags);
    }
    // Descending, so single-color outcomes come out in WUBRG order.
    out.sort_unstable_by(|a, b| b.cmp(a));
    out.dedup();
    out.into_iter().collect()
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
    /// CR 903.4: from the card data. Made-up cards leave it empty and get
    /// their colors and their cost's colors.
    #[serde(default)]
    pub color_identity: Colors,
    /// A planeswalker's printed loyalty (CR 209.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loyalty: Option<i32>,
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
    /// CR 903.4: the card's color identity (Commander).
    #[serde(default)]
    pub color_identity: Colors,
    /// A planeswalker's printed loyalty (CR 209.1, 306.5b).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loyalty: Option<i32>,
    /// Abilities in printed order, then intrinsic ones (CR 305.6).
    pub abilities: Vec<Ability>,
    /// Derived: the keyword bitset, for fast combat checks.
    pub keywords: Keywords,
    /// Derived: the mana abilities (CR 605.1a), in ability order.
    pub mana_abilities: SmallVec<[ManaAbility; 2]>,
    /// Derived: for a modal spell, a bitmask per mode of the target slots it
    /// refers to (CR 601.2c: only the chosen modes' targets are required).
    pub mode_slots: SmallVec<[u32; 4]>,
    /// Derived: the same per modal triggered ability, by ability index (CR 700.2b).
    pub trigger_mode_slots: SmallVec<[(u8, SmallVec<[u32; 4]>); 1]>,
    /// A token's definition (CR 111): never in a deck, never a nameable card (CR 201.4).
    pub token: bool,
    /// Abilities from this index on aren't printed: they're the abilities
    /// its copy effects grant (CR 707.9a), stored here so they can be
    /// referenced. `None`: all abilities are printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden_from: Option<u16>,
    /// An Aura's enchant ability as a target slot: Aura spells target what
    /// they'll enchant (CR 303.4a, 702.5a).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enchant: Option<ir::TargetSpec>,
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
        // CR 702.40a: storm is a triggered ability that functions on the stack.
        if abilities.contains(&Ability::Keyword(ir::Keyword::Storm)) {
            abilities.push(storm());
        }
        // CR 702.24a: cumulative upkeep is a triggered ability.
        let upkeeps: Vec<Vec<CostPart>> = abilities
            .iter()
            .filter_map(|a| match a {
                Ability::Keyword(ir::Keyword::CumulativeUpkeep(c)) => Some(c.clone()),
                _ => None,
            })
            .collect();
        for cost in upkeeps {
            abilities.push(cumulative_upkeep(&cost).map_err(unsupported)?);
        }
        // CR 702.6a: equip is an activated ability, as a sorcery.
        let equips: Vec<Vec<CostPart>> = abilities
            .iter()
            .filter_map(|a| match a {
                Ability::Keyword(ir::Keyword::Equip(c)) => Some(c.clone()),
                _ => None,
            })
            .collect();
        abilities.extend(equips.into_iter().map(equip));
        let enchant = abilities.iter().find_map(|a| match a {
            Ability::Keyword(ir::Keyword::Enchant(k)) => Some(ir::TargetSpec {
                what: k.clone(),
                up_to: None,
            }),
            _ => None,
        });
        // CR 702.21a: ward is a triggered ability.
        let wards: Vec<Vec<CostPart>> = abilities
            .iter()
            .filter_map(|a| match a {
                Ability::Keyword(ir::Keyword::Ward(c)) => Some(c.clone()),
                _ => None,
            })
            .collect();
        abilities.extend(wards.into_iter().map(ward));
        // CR 707.9a: abilities a copy effect grants, kept after the printed ones.
        let printed = abilities.len();
        let granted = copy_exception_abilities(&abilities);
        let hidden_from = (!granted.is_empty())
            .then(|| u16::try_from(printed).map_err(|_| unsupported("too many abilities".into())))
            .transpose()?;
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
                    let bags = expand_bags(&ability_mana_output(&x.effect).map_err(unsupported)?);
                    mana_abilities.push(ManaAbility {
                        ability: u8::try_from(i)
                            .map_err(|_| unsupported("more than 255 abilities".into()))?,
                        bags,
                        // Restricted mana (CR 106.6) isn't offered by payment plans:
                        // plans don't know what it may pay for.
                        tap_only: x.cost == [CostPart::Tap]
                            && choice_free(&x.effect)
                            && !restricted_mana(&x.effect),
                        dynamic: dynamic_mana(&x.effect),
                    });
                }
                _ => {}
            }
        }
        abilities.extend(granted);
        let mut mode_slots = SmallVec::new();
        let mut trigger_mode_slots = SmallVec::new();
        for (i, a) in abilities.iter().enumerate().take(printed) {
            match a {
                Ability::Spell(s) => {
                    if let ir::Effect::Modal { modes, .. } = &s.effect {
                        mode_slots.extend(modes.iter().map(target_slots));
                    }
                }
                // Modal triggered and activated abilities (CR 700.2a-b), by ability index.
                Ability::Triggered(ir::TriggeredAbility {
                    effect: ir::Effect::Modal { modes, .. },
                    ..
                })
                | Ability::Activated(ir::ActivatedAbility {
                    effect: ir::Effect::Modal { modes, .. },
                    ..
                }) => {
                    if let Ok(i) = u8::try_from(i) {
                        trigger_mode_slots.push((i, modes.iter().map(target_slots).collect()));
                    }
                }
                _ => {}
            }
        }
        let color_identity = ch.color_identity.union(ch.colors).union(
            ch.mana_cost
                .as_ref()
                .map_or(Colors::COLORLESS, ManaCost::colors),
        );
        let def = CardDef {
            name: ch.name,
            oracle_id: ch.oracle_id,
            mana_cost: ch.mana_cost,
            type_line: ch.type_line,
            colors: ch.colors,
            power: ch.power,
            toughness: ch.toughness,
            color_identity,
            loyalty: ch.loyalty,
            abilities,
            keywords,
            mana_abilities,
            mode_slots,
            trigger_mode_slots,
            token: false,
            hidden_from,
            enchant,
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

    /// "You may cast this card from exile" (CR 113.6f).
    pub fn castable_from_exile(&self) -> bool {
        self.abilities.iter().any(|a| {
            matches!(a, Ability::Static(s)
                if s.effect == ir::StaticEffect::Rule(ir::RuleMod::MayCastFromExile))
        })
    }

    /// Its dash cost (CR 702.109a), if it has one.
    /// Its flashback cost (CR 702.34a).
    pub fn flashback_cost(&self) -> Option<&[CostPart]> {
        self.abilities.iter().find_map(|a| match a {
            Ability::Keyword(ir::Keyword::Flashback(c)) => Some(c.as_slice()),
            _ => None,
        })
    }

    pub fn dash_cost(&self) -> Option<&[CostPart]> {
        self.abilities.iter().find_map(|a| match a {
            Ability::Keyword(ir::Keyword::Dash(c)) => Some(c.as_slice()),
            _ => None,
        })
    }

    /// Its "as this enters, choose" ability (CR 614.12): what and under which name.
    pub fn as_enters(&self) -> Option<(&ir::ChoiceSpec, &str)> {
        self.abilities.iter().find_map(|a| match a {
            Ability::AsEnters { choose, store } => Some((choose, store.as_str())),
            _ => None,
        })
    }

    /// Its opening-hand action (CR 103.6), with its ability index.
    pub fn opening_hand(&self) -> Option<(u8, &ir::OpeningHand)> {
        self.abilities
            .iter()
            .enumerate()
            .find_map(|(i, a)| match a {
                Ability::OpeningHand(o) => Some((u8::try_from(i).ok()?, o)),
                _ => None,
            })
    }

    /// Whether some activated ability works from the hand (CR 113.6b).
    pub fn has_hand_abilities(&self) -> bool {
        self.abilities
            .iter()
            .any(|a| matches!(a, Ability::Activated(x) if x.zone == Some(ir::ZoneName::Hand)))
    }

    pub fn is_legendary(&self) -> bool {
        self.type_line.has_supertype(Supertype::Legendary)
    }

    pub fn mana_value(&self) -> u32 {
        self.mana_cost.as_ref().map_or(0, ManaCost::mana_value)
    }

    /// The per-mode target masks of the spell (`None`) or of triggered
    /// ability `ability`; empty if it isn't modal.
    pub fn mode_masks(&self, ability: Option<u8>) -> &[u32] {
        match ability {
            None => &self.mode_slots,
            Some(i) => self
                .trigger_mode_slots
                .iter()
                .find(|(a, _)| *a == i)
                .map_or(&[], |(_, m)| m.as_slice()),
        }
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
        E::If {
            then, otherwise, ..
        } => could_add_mana(then) || otherwise.as_deref().is_some_and(could_add_mana),
        _ => false,
    }
}

/// "Add A. If [condition], instead add B" (Gemstone Caverns): both branches add mana.
fn conditional_mana(e: &ir::Effect) -> bool {
    matches!(e, ir::Effect::If { then, otherwise: Some(o), .. }
        if matches!(**then, ir::Effect::AddMana { .. }) && matches!(**o, ir::Effect::AddMana { .. }))
}

/// Whether a mana ability's effect needs no choices once its mana is fixed:
/// the mana plus, at most, life or damage to its controller (Ancient Tomb,
/// Talismans).
fn choice_free(e: &ir::Effect) -> bool {
    use ir::{Effect as E, PlayerRef as P, Recipient as R};
    match e {
        E::AddMana { .. } => true,
        E::Seq(v) => v.iter().all(choice_free),
        E::If {
            then, otherwise, ..
        } => choice_free(then) && otherwise.as_deref().is_none_or(choice_free),
        E::DealDamage {
            to: R::Player(P::You),
            ..
        }
        | E::LoseLife { who: P::You, .. }
        | E::GainLife { who: P::You, .. } => true,
        _ => false,
    }
}

/// Whether an effect adds mana with a spending restriction (CR 106.6).
fn restricted_mana(e: &ir::Effect) -> bool {
    let mut found = false;
    e.walk(&mut |x| {
        found |= matches!(
            x,
            ir::Effect::AddMana {
                spend_only_on: Some(_),
                ..
            }
        );
    });
    found
}

/// Whether an effect adds mana whose output depends on the game state.
fn dynamic_mana(e: &ir::Effect) -> bool {
    if conditional_mana(e) {
        return true;
    }
    let mut found = false;
    e.walk(&mut |x| {
        found |= matches!(x, ir::Effect::AddMana { mana, .. } if is_dynamic_spec(mana));
    });
    found
}

/// Mana specs the game computes when the mana is made: the output depends
/// on other objects, the commander, or an amount (M6 plan C10).
pub(crate) fn is_dynamic_spec(spec: &ManaSpec) -> bool {
    let variable = |q: &Quantity| !matches!(q, Quantity::Fixed(_));
    match spec {
        ManaSpec::CommanderIdentity
        | ManaSpec::ColorOf(_)
        | ManaSpec::ColorsAmong(_)
        | ManaSpec::EachColorAmong(_)
        | ManaSpec::OpponentLandColors
        | ManaSpec::ChosenColor(_)
        | ManaSpec::Times { .. } => true,
        ManaSpec::AnyColor(q) | ManaSpec::AnyOneColor(q) | ManaSpec::Colorless(q) => variable(q),
        ManaSpec::Fixed(_) | ManaSpec::OneOf(_) | ManaSpec::TypeProducedByTrigger => false,
    }
}

/// Ways the mana of a mana ability or spell can come out.
pub type ManaAlternatives = SmallVec<[SmallVec<[ManaUnit; 2]>; 1]>;

/// The output of a mana ability's effect. Only a single unconditional
/// `AddMana` is supported so far.
fn ability_mana_output(e: &ir::Effect) -> Result<ManaAlternatives, String> {
    use ir::Effect as E;
    if conditional_mana(e) {
        // A placeholder: the game picks the branch (`ManaAbility::dynamic`).
        return Ok(smallvec::smallvec![SmallVec::from_elem(ANY_COLOR, 1)]);
    }
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
    if is_dynamic_spec(spec) {
        // A placeholder: the game computes the real output (`ManaAbility::dynamic`).
        if let ManaSpec::Times { mana, .. } = spec {
            parse_mana_units(mana)?;
        }
        out.push(SmallVec::from_elem(ANY_COLOR, 1));
        return Ok(out);
    }
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
        ManaSpec::CommanderIdentity
        | ManaSpec::ColorOf(_)
        | ManaSpec::ColorsAmong(_)
        | ManaSpec::EachColorAmong(_)
        | ManaSpec::OpponentLandColors
        | ManaSpec::ChosenColor(_)
        | ManaSpec::Times { .. } => unreachable!("dynamic specs return above"),
        ManaSpec::TypeProducedByTrigger => {
            return Err("mana of the triggering permanent's type outside a trigger".into());
        }
        ManaSpec::Colorless(q) => {
            let n = fixed(q).ok_or("variable amount of mana")?;
            out.push(SmallVec::from_elem(ManaType::Colorless.bit(), n));
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

/// CR 702.40a: "When you cast this spell, copy it for each other spell that
/// was cast before it this turn. You may choose new targets for the copies."
fn storm() -> Ability {
    Ability::Triggered(ir::TriggeredAbility {
        trigger: ir::Trigger::Cast {
            by: ir::PlayerRel::You,
            spell: ir::ObjectFilter {
                this: true,
                ..ir::ObjectFilter::default()
            },
            first_each_turn: false,
            second_each_turn: false,
        },
        condition: None,
        resolves_anyway: false,
        targets: Vec::new(),
        effect: ir::Effect::CopySpell {
            what: ir::ObjectSet::One(ir::ObjectRef::This),
            new_targets: true,
            count: Some(Quantity::Expr(ir::QuantityExpr::TriggeringAmount)),
            controller: None,
        },
    })
}

/// `{T}: Add <type>.`
pub fn tap_for(t: ManaType) -> Ability {
    Ability::Activated(ActivatedAbility {
        cost: vec![CostPart::Tap],
        targets: Vec::new(),
        effect: ir::Effect::AddMana {
            mana: ManaSpec::Fixed(t.symbol().to_owned()),
            spend_only_on: None,
            uncounterable: false,
            to: None,
        },
        timing: ir::Timing::Any,
        once_per_turn: false,
        condition: None,
        zone: None,
        cost_reduction: None,
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
            color_identity: rec.color_identity,
            loyalty: match &f.loyalty {
                Some(PrintedStat::Fixed(n)) => Some(*n),
                _ => None,
            },
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
    /// Token definitions by [`token_key`] (CR 111): added for every token a
    /// card in the pool can create.
    #[serde(default)]
    tokens: BTreeMap<String, DefId>,
    /// Some definition has a triggered ability (or ward), so the engine must
    /// watch events for triggers; trigger-free games skip that work entirely.
    #[serde(default)]
    has_triggers: bool,
    /// Whether any definition has a replacement effect (CR 614); games
    /// without one never look for them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    has_replacements: bool,
    /// Whether any definition has a static ability the layers apply (CR 613);
    /// games without one read printed characteristics directly.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    has_layer_statics: bool,
    /// Whether any definition is an Aura (SBA 704.5m checks).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    has_auras: bool,
    /// Whether any definition has a rule-modifying static ability that the
    /// engine checks while offering actions (can't cast, can't activate,
    /// Torpor Orb; CR 613.11).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    has_rule_statics: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    tracks_spells: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    has_zone_replacements: bool,
    /// Hidden definitions holding each land type's intrinsic mana ability
    /// (CR 305.6), for lands that gain the type (Blood Moon, Urborg).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    intrinsics: BTreeMap<String, DefId>,
    /// A hidden token definition for token copies: its copy effect supplies
    /// every characteristic (CR 707.2), the definition only makes it a token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    copy_token: Option<DefId>,
}

/// Whether these abilities create token copies.
fn creates_token_copies(abilities: &[Ability]) -> bool {
    let mut found = false;
    for a in abilities {
        let root = match a {
            Ability::Spell(s) => &s.effect,
            Ability::Activated(x) => &x.effect,
            Ability::Triggered(t) => &t.effect,
            Ability::Replacement(r) => &r.instead,
            _ => continue,
        };
        root.walk(&mut |e| found |= matches!(e, ir::Effect::CreateTokenCopy { .. }));
    }
    found
}

/// The abilities copy effects in these abilities grant (CR 707.9a).
fn copy_exception_abilities(abilities: &[Ability]) -> Vec<Ability> {
    let mut out = Vec::new();
    for a in abilities {
        let root = match a {
            Ability::Spell(s) => &s.effect,
            Ability::Activated(x) => &x.effect,
            Ability::Triggered(t) => &t.effect,
            Ability::Replacement(r) => &r.instead,
            _ => continue,
        };
        root.walk(&mut |e| match e {
            ir::Effect::EnterAsCopy { except, .. } | ir::Effect::CreateTokenCopy { except, .. } => {
                out.extend(except.abilities.iter().cloned());
            }
            _ => {}
        });
    }
    out
}

impl CardDef {
    /// The abilities printed on the card (without copy-granted ones).
    pub fn printed_abilities(&self) -> &[Ability] {
        match self.hidden_from {
            Some(n) => &self.abilities[..usize::from(n)],
            None => &self.abilities,
        }
    }

    /// The index of a copy-granted ability equal to `a` (CR 707.9a).
    pub(crate) fn granted_index(&self, a: &Ability) -> Option<u8> {
        let from = usize::from(self.hidden_from?);
        self.abilities[from..]
            .iter()
            .position(|x| x == a)
            .and_then(|i| u8::try_from(from + i).ok())
    }
}

/// Land types a definition's static abilities give lands (CR 305.6, 305.7).
fn granted_land_types(abilities: &[Ability]) -> Vec<String> {
    fn walk(e: &ir::StaticEffect, out: &mut Vec<String>) {
        match e {
            ir::StaticEffect::LandType { land_type, .. } => out.push(land_type.clone()),
            ir::StaticEffect::Multi(v) => v.iter().for_each(|e| walk(e, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for a in abilities {
        if let Ability::Static(s) = a {
            walk(&s.effect, &mut out);
        }
    }
    out
}

/// The pool key of a token spec.
pub fn token_key(spec: &ir::TokenSpec) -> String {
    serde_json::to_string(spec).unwrap_or_default()
}

/// Every token spec an ability list can create.
fn token_specs(abilities: &[Ability]) -> Vec<ir::TokenSpec> {
    let mut out = Vec::new();
    for a in abilities {
        let e = match a {
            Ability::Spell(s) => &s.effect,
            Ability::Activated(x) => &x.effect,
            Ability::Triggered(t) => &t.effect,
            _ => continue,
        };
        e.walk(&mut |e| match e {
            ir::Effect::CreateToken { token, .. } => out.push(token.clone()),
            ir::Effect::Amass { subtype, .. } => out.push(amass_token(subtype)),
            _ => {}
        });
    }
    out
}

/// The Army token amass creates (CR 701.47a).
pub fn amass_token(subtype: &str) -> ir::TokenSpec {
    ir::TokenSpec::Creature {
        power: 0,
        toughness: 0,
        colors: "B".into(),
        subtypes: vec![subtype.to_owned(), "Army".into()],
        keywords: Vec::new(),
        name: None,
    }
}

/// CR 702.6a: "[Cost]: Attach to target creature you control. Equip only as a sorcery."
fn equip(cost: Vec<CostPart>) -> Ability {
    Ability::Activated(ir::ActivatedAbility {
        cost,
        targets: vec![ir::TargetSpec {
            what: ir::TargetKind::Object(ir::ObjectFilter {
                types: vec!["creature".into()],
                controller: ir::PlayerRel::You,
                ..ir::ObjectFilter::default()
            }),
            up_to: None,
        }],
        effect: ir::Effect::Attach {
            to: ir::ObjectRef::Target(0),
        },
        timing: ir::Timing::Sorcery,
        once_per_turn: false,
        condition: None,
        zone: None,
        cost_reduction: None,
    })
}

/// CR 702.21a: "Whenever this permanent becomes the target of a spell or
/// ability an opponent controls, counter that spell or ability unless that
/// player pays [cost]."
fn ward(cost: Vec<CostPart>) -> Ability {
    Ability::Triggered(ir::TriggeredAbility {
        trigger: ir::Trigger::BecomesTarget {
            by: ir::PlayerRel::Opponent,
            what: ir::ObjectFilter {
                this: true,
                ..ir::ObjectFilter::default()
            },
        },
        condition: None,
        resolves_anyway: false,
        targets: Vec::new(),
        effect: ir::Effect::Unless {
            payer: ir::PlayerRef::Triggering,
            cost,
            effect: Box::new(ir::Effect::Counter(ir::ObjectSet::One(
                ir::ObjectRef::Triggering,
            ))),
        },
    })
}

/// CR 702.24a: "At the beginning of your upkeep, if this permanent is on the
/// battlefield, put an age counter on this permanent. Then you may pay
/// [cost] for each age counter on it. If you don't, sacrifice it."
fn cumulative_upkeep(cost: &[CostPart]) -> Result<Ability, String> {
    let per_counter = ir::Quantity::Expr(ir::QuantityExpr::CountersOn {
        what: ir::ObjectRef::This,
        counter: ir::CounterKind::Age,
    });
    let cost = cost
        .iter()
        .map(|p| match p {
            CostPart::Mana(m) => Ok(CostPart::ManaRepeated {
                mana: m.clone(),
                times: per_counter.clone(),
            }),
            _ => Err("cumulative upkeep with a non-mana cost".to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let this = || ir::ObjectSet::One(ir::ObjectRef::This);
    Ok(Ability::Triggered(ir::TriggeredAbility {
        trigger: ir::Trigger::BeginningOf {
            step: ir::StepName::Upkeep,
            whose: ir::PlayerRel::You,
        },
        condition: None,
        resolves_anyway: false,
        targets: Vec::new(),
        effect: ir::Effect::Seq(vec![
            ir::Effect::PutCounters {
                what: this(),
                counter: ir::CounterKind::Age,
                amount: Quantity::Fixed(1),
            },
            ir::Effect::Unless {
                payer: ir::PlayerRef::You,
                cost,
                effect: Box::new(ir::Effect::Sacrifice {
                    who: ir::PlayerRef::You,
                    what: this(),
                }),
            },
        ]),
    }))
}

/// A token's definition (CR 111.4, 111.10).
pub fn token_def(spec: &ir::TokenSpec) -> Result<CardDef, Unsupported> {
    let sacrifice_this = || CostPart::Sacrifice(ir::ObjectSet::One(ir::ObjectRef::This));
    let activated = |cost: Vec<CostPart>, effect: ir::Effect| {
        Ability::Activated(ActivatedAbility {
            cost,
            targets: Vec::new(),
            effect,
            timing: ir::Timing::Any,
            once_per_turn: false,
            condition: None,
            zone: None,
            cost_reduction: None,
        })
    };
    let (ch, abilities) = match spec {
        ir::TokenSpec::Predefined(name) => {
            let ability = match name.as_str() {
                // CR 111.10a
                "Treasure" => activated(
                    vec![CostPart::Tap, sacrifice_this()],
                    ir::Effect::AddMana {
                        mana: ManaSpec::AnyColor(Quantity::Fixed(1)),
                        spend_only_on: None,
                        uncounterable: false,
                        to: None,
                    },
                ),
                // CR 111.10b
                "Food" => activated(
                    vec![
                        CostPart::Mana("{2}".into()),
                        CostPart::Tap,
                        sacrifice_this(),
                    ],
                    ir::Effect::GainLife {
                        who: ir::PlayerRef::You,
                        amount: Quantity::Fixed(3),
                    },
                ),
                // CR 111.10f
                "Clue" => activated(
                    vec![CostPart::Mana("{2}".into()), sacrifice_this()],
                    ir::Effect::Draw {
                        who: ir::PlayerRef::You,
                        amount: Quantity::Fixed(1),
                    },
                ),
                other => {
                    return Err(Unsupported {
                        name: other.to_owned(),
                        reason: "predefined token the engine doesn't define yet".into(),
                    });
                }
            };
            (
                synthetic(name, None, &format!("Artifact — {name}"), None),
                vec![ability],
            )
        }
        ir::TokenSpec::Creature {
            power,
            toughness,
            colors,
            subtypes,
            keywords,
            name,
        } => {
            let subtypes_text = subtypes.join(" ");
            let name = name
                .clone()
                .unwrap_or_else(|| format!("{subtypes_text} Token"));
            let mut ch = synthetic(
                &name,
                None,
                &format!("Creature — {subtypes_text}"),
                Some((*power, *toughness)),
            );
            ch.colors = colors.parse().unwrap_or(Colors::COLORLESS);
            (ch, keywords.iter().cloned().map(Ability::Keyword).collect())
        }
    };
    let mut def = CardDef::new(ch, abilities)?;
    def.token = true;
    Ok(def)
}

impl CardPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a definition (or returns the existing id for that name).
    /// Also adds definitions for the tokens the card can create. A token the
    /// engine can't define makes the card unsupported, so callers building
    /// pools from real cards should use [`CardPool::try_add`].
    pub fn add(&mut self, def: CardDef) -> DefId {
        self.try_add(def).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Adds a definition and the tokens it can create (or returns the
    /// existing id for that name).
    pub fn try_add(&mut self, def: CardDef) -> Result<DefId, Unsupported> {
        if let Some(id) = self.by_name.get(&def.name) {
            return Ok(*id);
        }
        let tokens: Vec<(String, CardDef)> = token_specs(&def.abilities)
            .iter()
            .filter(|s| !self.tokens.contains_key(&token_key(s)))
            .map(|s| Ok((token_key(s), token_def(s)?)))
            .collect::<Result<_, Unsupported>>()?;
        let granted = granted_land_types(&def.abilities);
        let id = self.push(def);
        self.by_name
            .insert(self.defs[id.0 as usize].name.clone(), id);
        for (key, t) in tokens {
            let tid = self.push(t);
            self.tokens.insert(key, tid);
        }
        if self.copy_token.is_none() && creates_token_copies(&self.defs[id.0 as usize].abilities) {
            let ch = synthetic("Copy token", None, "Creature", Some((0, 0)));
            let mut d = CardDef::new(ch, Vec::new())?;
            d.token = true;
            self.copy_token = Some(self.push(d));
        }
        for land_type in granted {
            if !self.intrinsics.contains_key(&land_type) {
                let ch = synthetic(
                    &format!("{land_type} (intrinsic)"),
                    None,
                    &format!("Land — {land_type}"),
                    None,
                );
                let d = CardDef::new(ch, Vec::new())?;
                let iid = self.push(d);
                self.intrinsics.insert(land_type, iid);
            }
        }
        Ok(id)
    }

    fn push(&mut self, def: CardDef) -> DefId {
        self.has_triggers |= def.abilities.iter().any(|a| {
            matches!(
                a,
                Ability::Triggered(_) | Ability::Keyword(ir::Keyword::Ward(_))
            )
        });
        self.has_replacements |= def
            .abilities
            .iter()
            .any(|a| matches!(a, Ability::Replacement(_)));
        self.has_auras |= def.enchant.is_some();
        self.has_layer_statics |= def
            .abilities
            .iter()
            .any(|a| matches!(a, Ability::Static(s) if crate::layers::is_layer_static(&s.effect)));
        self.has_rule_statics |= def
            .abilities
            .iter()
            .any(|a| matches!(a, Ability::Static(s) if crate::layers::is_rule_static(&s.effect)));
        self.has_zone_replacements |= def.abilities.iter().any(|a| {
            matches!(a, Ability::Replacement(r)
                if crate::support::is_exile_instead(&r.instead)
                    && matches!(&r.event, ir::ReplaceEvent::WouldGoToGraveyard(_))
                        | matches!(&r.event, ir::ReplaceEvent::WouldEnter(f) if !f.this))
        });
        self.tracks_spells |= self.has_triggers
            || serde_json::to_string(&def.abilities).is_ok_and(|j| {
                j.contains("\"spells_cast_this_turn\"")
                    || j.contains("\"most_spells_cast_by_an_opponent\"")
                    || j.contains("\"one_spell_each_turn\"")
                    || j.contains("\"second_each_turn\"")
            });
        let id = DefId(u32::try_from(self.defs.len()).unwrap_or(u32::MAX));
        self.defs.push(def);
        id
    }

    /// Whether any definition can trigger (CR 603); see the field.
    pub fn has_triggers(&self) -> bool {
        self.has_triggers
    }

    /// Whether any definition has a replacement effect; see the field.
    pub fn has_replacements(&self) -> bool {
        self.has_replacements
    }

    /// Whether any definition is an Aura (an enchant ability, CR 702.5).
    pub fn has_auras(&self) -> bool {
        self.has_auras
    }

    /// Whether any definition has a static ability the layers apply.
    pub fn has_layer_statics(&self) -> bool {
        self.has_layer_statics
    }

    /// Whether any definition has a rule static the engine checks; see the field.
    /// Whether games must keep the spells cast each turn (CR 601.2i): for
    /// triggers, storm, and conditions and amounts that count spells.
    pub fn tracks_spells(&self) -> bool {
        self.tracks_spells
    }

    /// Whether some definition has an "exile it instead" zone replacement.
    pub fn has_zone_replacements(&self) -> bool {
        self.has_zone_replacements
    }

    pub fn has_rule_statics(&self) -> bool {
        self.has_rule_statics
    }

    /// The token definition token copies use.
    pub(crate) fn copy_token(&self) -> Option<DefId> {
        self.copy_token
    }

    /// The intrinsic mana ability of a land type (CR 305.6), if it has one.
    pub(crate) fn intrinsic(&self, land_type: &str) -> Option<crate::effects::AbilityRef> {
        let def = *self.intrinsics.get(land_type)?;
        (!self.get(def).abilities.is_empty())
            .then_some(crate::effects::AbilityRef { def, ability: 0 })
    }

    /// The definition of a token spec, added when a card that creates it was added.
    pub fn token(&self, spec: &ir::TokenSpec) -> Option<DefId> {
        self.tokens.get(&token_key(spec)).copied()
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
        let cards = kiln_cards::defs::parse_synthetic(text, "synthetic-ir-cards.yaml")
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
        color_identity: Colors::COLORLESS,
        loyalty: None,
    }
}

fn spell(targets: Vec<TargetSpec>, effect: ir::Effect) -> SpellAbility {
    SpellAbility {
        targets,
        effect,
        alternative_costs: Vec::new(),
        additional_costs: Vec::new(),
        per_extra_target: Vec::new(),
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
        // A placeholder the game narrows to the commander's identity (M6 plan C6).
        let tower = units(ManaSpec::CommanderIdentity).unwrap_or_default();
        assert_eq!(tower[0].as_slice(), &[ANY_COLOR]);
    }

    #[test]
    fn permanent_kinds_without_rules_are_unsupported() {
        // M4 4.5: their state-based actions and rules aren't implemented yet.
        for (types, why) in [
            // Planeswalkers need a printed loyalty (made-up ones have none).
            ("Legendary Planeswalker — Test", "306.5b"),
            ("Battle — Siege", "704.5v"),
            ("Enchantment — Saga", "704.5s"),
            ("World Enchantment", "704.5k"),
        ] {
            let ch = synthetic("Test Kind", Some("{1}"), types, None);
            let err = CardDef::new(ch, Vec::new()).err().map(|e| e.to_string());
            assert!(
                err.as_deref().is_some_and(|e| e.contains(why)),
                "{types}: {err:?}"
            );
        }
        // Unattached Equipment does nothing, so it stays supported.
        let blade = synthetic("Test Kind", Some("{1}"), "Artifact — Equipment", None);
        assert!(CardDef::new(blade, Vec::new()).is_ok());
    }
}
