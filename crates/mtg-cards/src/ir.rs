//! Card IR v1 draft (M3 spike, step A1; `docs/05 Card Definitions/Card Definition Spike Plan.md`).
//!
//! One typed representation of what a card does. Every front-end (Oracle
//! parser, YAML definitions, Rust escapes) produces it, and the engine will
//! interpret it (Phase B). The YAML form of these types *is* the DSL candidate
//! measured by the spike.
//!
//! Design rules:
//! - **Compositional:** effects, filters, quantities and costs combine freely,
//!   because 89% of distinct Oracle lines are unique (Oracle Text Corpus Analysis).
//! - **Rules procedures stay in the engine.** The IR says *what* (target a
//!   creature, counter a spell), never *how* priority or casting work.
//! - **Finite named rule modifications** ([`RuleMod`]) for statics that change
//!   game rules rather than objects (Grand Abolisher, Opposition Agent).
//! - **Explicit escape hatch** ([`Effect::Escape`], [`Ability::Escape`]) with
//!   a function name, so unexpressed behaviour is visible, never silent.

use serde::{Deserialize, Serialize};

/// A card's behaviour: one entry per face.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardIr {
    pub faces: Vec<FaceIr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceIr {
    #[serde(default)]
    pub abilities: Vec<Ability>,
}

/// An ability (CR 113) or the effect of an instant/sorcery (CR 113.3a).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Ability {
    /// A keyword ability (CR 702).
    Keyword(Keyword),
    /// The spell ability of an instant or sorcery (CR 113.3a).
    Spell(SpellAbility),
    /// CR 602. Mana abilities (CR 605.1a) are activated abilities whose effect adds mana.
    Activated(ActivatedAbility),
    /// CR 603.
    Triggered(TriggeredAbility),
    /// CR 604.
    Static(StaticAbility),
    /// CR 614, 615.
    Replacement(ReplacementAbility),
    /// Behaviour implemented by a named Rust function (the escape hatch).
    Escape(Escape),
}

/// A named Rust implementation, with the Oracle text it covers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Escape {
    pub function: String,
    pub text: String,
}

/// Keyword abilities (CR 702) the IR names directly.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Keyword {
    Deathtouch,
    Defender,
    DoubleStrike,
    FirstStrike,
    Flash,
    Flying,
    Haste,
    Hexproof,
    Indestructible,
    Lifelink,
    Menace,
    Reach,
    Shroud,
    Trample,
    Vigilance,
    /// CR 702.21
    Ward(Vec<CostPart>),
    /// CR 702.24
    CumulativeUpkeep(Vec<CostPart>),
    /// CR 702.124h
    Partner,
    /// CR 702.124j
    PartnerWith(String),
    /// CR 702.16
    Protection(ObjectFilter),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellAbility {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetSpec>,
    pub effect: Effect,
    /// "rather than pay this spell's mana cost" (CR 118.9).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternative_costs: Vec<AltCost>,
    /// Additional costs (CR 601.2b).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_costs: Vec<CostPart>,
}

/// An alternative cost (CR 118.9), optionally conditional.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AltCost {
    /// Empty = "without paying its mana cost".
    #[serde(default)]
    pub cost: Vec<CostPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivatedAbility {
    pub cost: Vec<CostPart>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetSpec>,
    pub effect: Effect,
    /// CR 602.5d: "Activate only as a sorcery".
    #[serde(default, skip_serializing_if = "is_default")]
    pub timing: Timing,
    /// "Activate only once each turn" (CR 602.5b).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub once_per_turn: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Timing {
    #[default]
    Any,
    Sorcery,
    /// "Activate only as an instant" (redundant for most abilities, but printed).
    Instant,
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    *t == T::default()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggeredAbility {
    pub trigger: Trigger,
    /// Intervening "if" clause (CR 603.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetSpec>,
    pub effect: Effect,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticAbility {
    pub effect: StaticEffect,
    /// "During your turn, …", "As long as …".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplacementAbility {
    pub event: ReplaceEvent,
    /// What happens instead (CR 614.1a).
    pub instead: Effect,
}

/// Events a replacement effect watches (CR 614.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReplaceEvent {
    /// "If [object] would enter" / "As [object] enters" (CR 614.1c, 614.12).
    WouldEnter(ObjectFilter),
    /// "If [player] would draw a card" (CR 614.11).
    WouldDraw(PlayerRel),
    /// "If [object] would be put into a graveyard from anywhere".
    WouldGoToGraveyard(ObjectFilter),
    /// A player would find cards while searching (Opposition Agent).
    WouldFindWhileSearching(PlayerRel),
}

// ---- triggers ----------------------------------------------------------------

/// Trigger events (CR 603.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Trigger {
    /// "When/Whenever [object] enters" (CR 603.6a).
    Enters(ObjectFilter),
    /// "dies" = put into a graveyard from the battlefield (CR 700.4).
    Dies(ObjectFilter),
    LeavesBattlefield(ObjectFilter),
    /// "Whenever [player] casts [spell]".
    Cast {
        by: PlayerRel,
        spell: ObjectFilter,
    },
    /// "Whenever [player] draws a card".
    Draws {
        by: PlayerRel,
        /// "except the first one they draw in each of their draw steps".
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        except_first_in_draw_step: bool,
    },
    /// "Whenever [player] discards a card".
    Discards {
        by: PlayerRel,
    },
    /// "Whenever [player] taps [object] for mana" (CR 605.1b).
    TapsForMana {
        by: PlayerRel,
        what: ObjectFilter,
    },
    /// "At the beginning of [step]" (e.g. upkeep, CR 503.1a; end step, CR 513.1a).
    BeginningOf {
        step: StepName,
        whose: PlayerRel,
    },
    Attacks(ObjectFilter),
    /// "Whenever [source] deals combat damage to a player".
    DealsCombatDamageToPlayer(ObjectFilter),
    /// Several trigger conditions for one ability ("When ~ enters and whenever …").
    AnyOf(Vec<Trigger>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StepName {
    Upkeep,
    Draw,
    PrecombatMain,
    PostcombatMain,
    BeginningOfCombat,
    End,
}

// ---- static effects ----------------------------------------------------------

/// Continuous effects from static abilities (CR 604, 611.3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StaticEffect {
    /// Layer 7c: "[objects] get +N/+M".
    Pump {
        affected: ObjectFilter,
        power: i32,
        toughness: i32,
    },
    /// Layer 6: "[objects] have [keyword]".
    Grant {
        affected: ObjectFilter,
        keyword: Keyword,
    },
    /// Layer 6 for cards in a zone: Underworld Breach's escape.
    GrantEscape {
        affected: ObjectFilter,
        extra_cost: Vec<CostPart>,
    },
    /// Layer 4: set (or add) a land type. Blood Moon: replace = true.
    LandType {
        affected: ObjectFilter,
        land_type: String,
        replace: bool,
    },
    /// "[who] can't cast [spells]" (Grand Abolisher, Silence).
    CantCast {
        who: PlayerRel,
        spells: ObjectFilter,
    },
    /// "[who] can't activate abilities of [sources]".
    CantActivate {
        who: PlayerRel,
        sources: ObjectFilter,
    },
    /// "This doesn't untap during your untap step" (CR 502.3).
    DoesntUntap(ObjectFilter),
    /// "Skip your [step]" (CR 614.1b).
    SkipStep { who: PlayerRel, step: StepName },
    /// Named rule modifications.
    Rule(RuleMod),
}

/// Statics that modify game rules rather than objects. A closed set the engine
/// implements one by one; adding a variant is a reviewed engine change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleMod {
    /// "You control your opponents while they're searching their libraries" (CR 723).
    ControlOpponentsWhileSearching,
    /// "You may play those cards … spend mana as though any color" (linked to exiled cards).
    MayPlayExiledWithThis { any_color_mana: bool },
    /// "This spell can't be countered" (a static ability that functions on the stack, CR 101.2).
    CantBeCountered,
}

// ---- effects -----------------------------------------------------------------

/// One-shot effects, executed in written order (CR 608.2c).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    /// Several effects in order ("…, then …").
    Seq(Vec<Effect>),
    /// "Choose one —" (CR 700.2).
    Modal {
        min: u32,
        max: u32,
        modes: Vec<Effect>,
    },
    /// "You may [effect]" (the controller chooses at resolution, CR 608.2d).
    May(Box<Effect>),
    /// "[who] may [action]. If [they] do, [then]. If [they] don't, [otherwise]."
    IfDoes {
        who: PlayerRef,
        action: Box<Effect>,
        then: Box<Effect>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise: Option<Box<Effect>>,
    },
    /// "If [condition], [then]."
    If {
        condition: Condition,
        then: Box<Effect>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise: Option<Box<Effect>>,
    },
    /// "[effect] unless [payer] pays [cost]" (CR 118.12).
    Unless {
        payer: PlayerRef,
        cost: Vec<CostPart>,
        effect: Box<Effect>,
    },
    /// "Each player …": the effect runs for each player in APNAP order (CR 101.4), binding `each`.
    ForEachPlayer {
        players: PlayerRel,
        effect: Box<Effect>,
    },
    /// Pay a cost as part of an effect.
    Pay {
        who: PlayerRef,
        cost: Vec<CostPart>,
    },
    DealDamage {
        to: Recipient,
        amount: Quantity,
    },
    Destroy(ObjectSet),
    Exile {
        what: ObjectSet,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        face_down: bool,
        /// Name the exiled objects for later reference (CR 607 linked abilities).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remember: Option<String>,
    },
    /// Put objects into a zone ("Put that card into your hand", "Return … to its owner's hand").
    Move {
        what: ObjectSet,
        to: Destination,
    },
    /// "[objects] get +N/+M until end of turn" (a layer 7c effect with a duration, CR 611.2).
    Pump {
        what: ObjectSet,
        power: i32,
        toughness: i32,
    },
    /// "[object] enters tapped", as a replacement's modification (CR 614.1c).
    EntersTapped,
    /// "Reveal cards from the top of [who]'s library until you reveal a card
    /// matching [filter]. Put that card into [found_to] and the rest into [rest_to]."
    RevealUntil {
        who: PlayerRef,
        filter: ObjectFilter,
        found_to: Destination,
        rest_to: Destination,
    },
    Sacrifice {
        who: PlayerRef,
        what: ObjectSet,
    },
    /// CR 701.6
    Counter(ObjectSet),
    Draw {
        who: PlayerRef,
        amount: Quantity,
    },
    Discard {
        who: PlayerRef,
        what: DiscardSpec,
    },
    GainLife {
        who: PlayerRef,
        amount: Quantity,
    },
    LoseLife {
        who: PlayerRef,
        amount: Quantity,
    },
    /// CR 106.3
    AddMana {
        mana: ManaSpec,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spend_only_on: Option<ObjectFilter>,
    },
    /// CR 701.23. A search for "a card" (an empty filter) must find one if
    /// possible (701.23d); a filter states a quality, so finding nothing is
    /// allowed (701.23b). With `to: top_of_library` the library is shuffled
    /// before the found cards go on top (CR 701.24b), so no separate
    /// `shuffle` follows.
    Search {
        who: PlayerRef,
        filter: ObjectFilter,
        count: Quantity,
        to: Destination,
        reveal: bool,
    },
    Shuffle(PlayerRef),
    /// CR 701.22
    Scry {
        who: PlayerRef,
        amount: Quantity,
    },
    /// Look at the top N cards and redistribute them (Ponder, Thassa's Oracle, Kinnan).
    LookAtTop {
        who: PlayerRef,
        count: Quantity,
        /// Up to this many matching cards may go to `take_to`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        take: Option<TakeSpec>,
        /// Where the rest go.
        rest: RestOrder,
    },
    /// Move cards a player chooses from one of their zones (Brainstorm's put-back).
    MoveCards {
        who: PlayerRef,
        from: ZoneName,
        count: Quantity,
        to: Destination,
    },
    Tap(ObjectSet),
    Untap(ObjectSet),
    CreateToken {
        token: TokenSpec,
        count: Quantity,
        controller: PlayerRef,
    },
    /// "Put N [kind] counters on [objects]" (CR 122.1, 122.6).
    PutCounters {
        what: ObjectSet,
        counter: CounterKind,
        amount: Quantity,
    },
    /// "Remove N [kind] counters from [objects]" (as many as there are, CR 609.3).
    RemoveCounters {
        what: ObjectSet,
        counter: CounterKind,
        amount: Quantity,
    },
    /// CR 701.47
    Amass {
        subtype: String,
        amount: Quantity,
    },
    /// Choose a value at resolution and remember it as `store`.
    Choose {
        what: ChoiceSpec,
        store: String,
    },
    /// Create a delayed triggered ability (CR 603.7).
    Delayed {
        at: Trigger,
        effect: Box<Effect>,
    },
    /// "[who] wins the game" (CR 104.2b).
    WinGame(PlayerRef),
    /// Implemented by a named Rust function.
    Escape(Escape),
}

/// Who or what receives damage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Recipient {
    Target(u8),
    Player(PlayerRef),
    Object(ObjectRef),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DiscardSpec {
    /// "discards their hand".
    Hand,
    /// "discards N cards" (the player chooses), optionally "a land card".
    Chosen {
        count: Quantity,
        #[serde(default, skip_serializing_if = "is_default")]
        filter: ObjectFilter,
    },
    /// "discards N cards at random".
    Random(Quantity),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TakeSpec {
    pub filter: ObjectFilter,
    pub up_to: u32,
    pub to: Destination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RestOrder {
    /// Back on top in any order.
    TopAnyOrder,
    /// On the bottom in any order.
    BottomAnyOrder,
    /// On the bottom in a random order.
    BottomRandom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ZoneName {
    Hand,
    Library,
    Graveyard,
    Battlefield,
    Exile,
    Stack,
    Command,
}

/// Where moved cards go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Destination {
    Hand,
    Battlefield,
    TopOfLibrary,
    BottomOfLibrary,
    Graveyard,
    Exile,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TokenSpec {
    /// A predefined token (CR 111.10): Treasure, Clue, Food…
    Predefined(String),
    /// A creature token with stated characteristics (CR 111.4: its name is
    /// its subtypes plus "Token" unless `name` says otherwise).
    Creature {
        power: i32,
        toughness: i32,
        /// WUBRG letters; empty = colorless.
        #[serde(default)]
        colors: String,
        subtypes: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keywords: Vec<Keyword>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

/// Kinds of counters (CR 122) the IR names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CounterKind {
    #[serde(rename = "+1/+1")]
    PlusOne,
    #[serde(rename = "-1/-1")]
    MinusOne,
    #[serde(rename = "charge")]
    Charge,
    #[serde(rename = "loyalty")]
    Loyalty,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ChoiceSpec {
    CardName,
    Color,
    Number { min: i64, max: i64 },
}

/// Mana produced by an effect (CR 106).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ManaSpec {
    /// Specific symbols, e.g. "{C}{C}" or "{B}{B}{B}".
    Fixed(String),
    /// One of several fixed options ("Add {G} or {U}").
    OneOf(Vec<String>),
    /// N mana in any combination of colors (each chosen separately).
    AnyColor(Quantity),
    /// N mana of any one color.
    AnyOneColor(Quantity),
    /// One mana of any color in your commander's color identity (CR 903.4f).
    CommanderIdentity,
    /// One mana of any of the colors of an object (Chrome Mox's exiled card).
    ColorOf(ObjectRef),
    /// One mana of any type the triggering permanent produced (Kinnan).
    TypeProducedByTrigger,
}

// ---- references, filters, quantities ----------------------------------------

/// A single object an effect refers to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ObjectRef {
    /// The object the ability is on ("this", "~").
    This,
    /// The object chosen for target slot N.
    Target(u8),
    /// The object from the trigger event ("that card", "that spell").
    Triggering,
    /// An object remembered by name ("the exiled card").
    Remembered(String),
}

/// A set of objects an effect acts on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ObjectSet {
    One(ObjectRef),
    /// "each creature you control", "all other cards revealed this way".
    All(ObjectFilter),
    /// "the top N cards of [player]'s library".
    TopOfLibrary {
        owner: PlayerRef,
        count: Quantity,
    },
    /// Cards a player chooses from one of their zones ("a nonland card from your hand").
    ChosenFrom {
        who: PlayerRef,
        zone: ZoneName,
        filter: ObjectFilter,
        count: Quantity,
    },
}

/// A player an effect refers to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlayerRef {
    /// The ability's controller.
    You,
    /// The player chosen for target slot N.
    Target(u8),
    ControllerOf(ObjectRef),
    OwnerOf(ObjectRef),
    /// The player from the trigger event ("that player").
    Triggering,
    /// The player bound by [`Effect::ForEachPlayer`].
    Each,
}

/// A relation to the ability's controller, for filters and "each" effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlayerRel {
    #[default]
    Any,
    You,
    Opponent,
}

/// What a target slot accepts (CR 115).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetSpec {
    pub what: TargetKind,
    /// "up to N" targets; default exactly 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up_to: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetKind {
    /// "any target" (CR 115.4).
    Any,
    /// A permanent (or card in a zone) matching the filter.
    Object(ObjectFilter),
    /// A spell on the stack matching the filter.
    Spell(ObjectFilter),
    Player(PlayerRel),
}

/// Object characteristics a filter can test. All present fields must match.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectFilter {
    /// Only this object.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub this: bool,
    /// "other"/"another": not this object.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub other: bool,
    /// Any of these card types ("artifact or enchantment").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    /// None of these card types ("noncreature").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtypes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_subtypes: Vec<String>,
    /// e.g. "basic", "nonbasic" via `not_supertypes`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supertypes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_supertypes: Vec<String>,
    /// Any of these colors, as WUBRG letters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub controller: PlayerRel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<ZoneName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<bool>,
    /// "a card with the chosen name".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named: Option<String>,
    /// "with mana value N or greater" (CR 202.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_mana_value: Option<u32>,
    /// "with mana value N or less" (CR 202.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_mana_value: Option<u32>,
}

/// Numbers (CR 107.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Quantity {
    Fixed(i64),
    Expr(QuantityExpr),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum QuantityExpr {
    /// The announced X (CR 107.3).
    X,
    PowerOf(ObjectRef),
    ManaValueOf(ObjectRef),
    /// The number of objects matching a filter.
    Count(ObjectFilter),
    /// Devotion to a color (CR 700.5).
    Devotion(String),
    CardsInLibrary(PlayerRef),
    /// Opponents dealt combat damage this turn (Tymna).
    OpponentsDealtCombatDamageThisTurn,
    /// The most cards a player discarded this way (Windfall).
    MostDiscardedThisWay,
    /// A remembered value.
    Var(String),
    Plus(Box<Quantity>, i64),
    /// The number of counters of a kind on an object.
    CountersOn {
        what: ObjectRef,
        counter: CounterKind,
    },
}

/// Conditions (intervening ifs, "if you control …").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    YourTurn,
    /// "If you control a commander" (CR 903.3d).
    ControlCommander,
    /// "if this is tapped".
    ThisTapped,
    /// left ≥ right.
    AtLeast(Quantity, Quantity),
    /// "If you do" bookkeeping is in [`Effect::IfDoes`], not here.
    Not(Box<Condition>),
}

/// Parts of a cost (CR 118).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CostPart {
    /// Mana symbols, e.g. "{2}{U}".
    Mana(String),
    /// {T} (CR 107.5).
    Tap,
    /// {Q} (CR 107.6).
    Untap,
    PayLife(Quantity),
    Sacrifice(ObjectSet),
    Discard(DiscardSpec),
    /// Exile a card from your hand matching the filter ("exile a blue card from your hand").
    ExileFromHand(ObjectFilter),
    /// Exile N other cards from your graveyard (escape).
    ExileFromGraveyard {
        count: u32,
        other: bool,
    },
    /// Exile an object (e.g. "Exile a creature you control"); `remember` names it for the effect.
    Exile {
        what: ObjectFilter,
        remember: String,
    },
    /// Remove N counters of a kind from this object (CR 122).
    RemoveCounters {
        counter: CounterKind,
        amount: u32,
    },
}

impl Effect {
    /// Calls `f` on this effect and every nested effect, depth first.
    pub fn walk(&self, f: &mut impl FnMut(&Effect)) {
        f(self);
        for c in children(self) {
            c.walk(f);
        }
    }
}

impl CardIr {
    /// Structural checks: target indices refer to declared slots.
    pub fn validate(&self) -> Result<(), String> {
        for (fi, face) in self.faces.iter().enumerate() {
            for (ai, a) in face.abilities.iter().enumerate() {
                let (targets, effect) = match a {
                    Ability::Spell(s) => (s.targets.len(), Some(&s.effect)),
                    Ability::Activated(x) => (x.targets.len(), Some(&x.effect)),
                    Ability::Triggered(t) => (t.targets.len(), Some(&t.effect)),
                    _ => (0, None),
                };
                if let Some(e) = effect {
                    let max = max_target_ref(e);
                    if let Some(m) = max
                        && usize::from(m) >= targets
                    {
                        return Err(format!(
                            "face {fi} ability {ai}: refers to target {m} but declares {targets} target slot(s)"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Escape hatches used by this card (function names).
    pub fn escapes(&self) -> Vec<String> {
        let mut out = Vec::new();
        for face in &self.faces {
            for a in &face.abilities {
                match a {
                    Ability::Escape(e) => out.push(e.function.clone()),
                    Ability::Spell(s) => collect_escapes(&s.effect, &mut out),
                    Ability::Activated(x) => collect_escapes(&x.effect, &mut out),
                    Ability::Triggered(t) => collect_escapes(&t.effect, &mut out),
                    Ability::Replacement(r) => collect_escapes(&r.instead, &mut out),
                    Ability::Keyword(_) | Ability::Static(_) => {}
                }
            }
        }
        out
    }
}

fn children(e: &Effect) -> Vec<&Effect> {
    match e {
        Effect::Seq(v) => v.iter().collect(),
        Effect::Modal { modes, .. } => modes.iter().collect(),
        Effect::May(x)
        | Effect::Unless { effect: x, .. }
        | Effect::ForEachPlayer { effect: x, .. }
        | Effect::Delayed { effect: x, .. } => vec![x],
        Effect::IfDoes {
            action,
            then,
            otherwise,
            ..
        } => {
            let mut v: Vec<&Effect> = vec![action, then];
            v.extend(otherwise.as_deref());
            v
        }
        Effect::If {
            then, otherwise, ..
        } => {
            let mut v: Vec<&Effect> = vec![then];
            v.extend(otherwise.as_deref());
            v
        }
        _ => Vec::new(),
    }
}

fn collect_escapes(e: &Effect, out: &mut Vec<String>) {
    if let Effect::Escape(x) = e {
        out.push(x.function.clone());
    }
    for c in children(e) {
        collect_escapes(c, out);
    }
}

fn max_target_ref(e: &Effect) -> Option<u8> {
    let own = match e {
        Effect::DealDamage {
            to: Recipient::Target(n),
            ..
        } => Some(*n),
        Effect::Destroy(ObjectSet::One(ObjectRef::Target(n)))
        | Effect::Counter(ObjectSet::One(ObjectRef::Target(n)))
        | Effect::Tap(ObjectSet::One(ObjectRef::Target(n)))
        | Effect::Untap(ObjectSet::One(ObjectRef::Target(n))) => Some(*n),
        Effect::Exile {
            what: ObjectSet::One(ObjectRef::Target(n)),
            ..
        } => Some(*n),
        Effect::GainLife {
            who: PlayerRef::ControllerOf(ObjectRef::Target(n)) | PlayerRef::Target(n),
            ..
        } => Some(*n),
        _ => None,
    };
    children(e)
        .into_iter()
        .filter_map(max_target_ref)
        .chain(own)
        .max()
}
