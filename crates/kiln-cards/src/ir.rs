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
    /// "If this card is in your opening hand, you may [effect]" (CR 103.6:
    /// Leyline of the Void, Gemstone Caverns).
    OpeningHand(OpeningHand),
    /// "As this enters, choose [a color / a number]" (CR 614.12): the choice
    /// is kept with the permanent under `store`.
    AsEnters { choose: ChoiceSpec, store: String },
}

/// An opening-hand action (CR 103.6).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningHand {
    /// "and you're not the starting player".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_starting_player: bool,
    pub effect: Effect,
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
    /// CR 702.40: "When you cast this spell, copy it for each other spell
    /// that was cast before it this turn".
    Storm,
    /// CR 702.61: while it's on the stack, players can't cast spells or
    /// activate abilities that aren't mana abilities.
    SplitSecond,
    /// CR 702.33: an optional additional cost.
    Kicker(Vec<CostPart>),
    /// CR 702.11d: "hexproof from [colors]".
    HexproofFrom(kiln_types::Colors),
    /// CR 702.109: an alternative cost; the permanent gains haste and
    /// returns to its owner's hand at the beginning of the next end step.
    Dash(Vec<CostPart>),
    /// CR 702.124h
    Partner,
    /// CR 702.124j
    PartnerWith(String),
    /// CR 702.16
    Protection(ObjectFilter),
    /// "Enchant [object or player]" (CR 702.5): what an Aura can be attached to.
    Enchant(TargetKind),
    /// "Equip [cost]" (CR 702.6): attach to target creature you control, as a sorcery.
    Equip(Vec<CostPart>),
    /// "Flashback [cost]" (CR 702.34a): cast from your graveyard for this
    /// cost rather than its mana cost; then it's exiled when it leaves the stack.
    Flashback(Vec<CostPart>),
    /// "[This] can't be blocked": not a keyword ability, but an evasion
    /// restriction tracked like one (CR 509.1b).
    CantBeBlocked,
    /// "Convoke" (CR 702.51a): creatures you tap help pay for this spell.
    Convoke,
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
    /// Strive: "costs [this] more to cast for each target beyond the first"
    /// (CR 601.2f).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_extra_target: Vec<CostPart>,
    /// "This spell costs {1} less to cast for each [quantity]" (Blasphemous
    /// Act; CR 601.2f): generic mana only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_reduction: Option<Quantity>,
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
    /// Overload (CR 702.96): cast this way, the spell has no targets and
    /// this effect replaces its text ("target" becomes "each").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<Effect>,
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
    /// "Activate only if [condition]" (CR 602.5b; Mox Opal's metalcraft).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
    /// The zone the ability works from, if not the battlefield (CR 113.6b,
    /// 113.6j, 113.6m): `hand` for channel and the Spirit Guides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<ZoneName>,
    /// "This ability costs {1} less to activate for each [quantity]": generic
    /// mana only (CR 601.2f via 602.2b).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_reduction: Option<Quantity>,
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
    /// "This ability still resolves if its target becomes illegal" (an
    /// exception to CR 608.2b; Gilded Drake).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resolves_anyway: bool,
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
    /// "If you would draw a card while your library has no cards in it".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
}

/// Events a replacement effect watches (CR 614.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReplaceEvent {
    /// "If [object] would enter" / "As [object] enters" (CR 614.1c, 614.12).
    WouldEnter(ObjectFilter),
    /// "If [player] would draw a card" (CR 614.11).
    WouldDraw {
        by: PlayerRel,
        /// "except the first one they draw in each of their draw steps".
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        except_first_in_draw_step: bool,
    },
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
        /// "their first [spell] each turn".
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        first_each_turn: bool,
        /// "their second spell each turn" (Kraum).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        second_each_turn: bool,
    },
    /// "Whenever [player] draws a card".
    Draws {
        by: PlayerRel,
        /// "except the first one they draw in each of their draw steps".
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        except_first_in_draw_step: bool,
        /// "their second card each turn" (Faerie Mastermind).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        second_each_turn: bool,
    },
    /// "Whenever [player] discards a card".
    Discards {
        by: PlayerRel,
    },
    /// "Whenever [object] becomes the target of a spell or ability [player]
    /// controls" (ward, CR 702.21a).
    BecomesTarget {
        by: PlayerRel,
        what: ObjectFilter,
    },
    /// "Whenever [player] gains life" (CR 119.9).
    GainsLife {
        by: PlayerRel,
    },
    /// "Whenever [player] loses life" (CR 119.3).
    LosesLife {
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
    /// "Whenever [object] becomes tapped" (CR 110.5; City of Brass).
    BecomesTapped(ObjectFilter),
    /// "Whenever [player] searches their library" (CR 701.23; Archivist of Oghma).
    Searches {
        by: PlayerRel,
    },
    /// "Whenever [source] deals combat damage to a player".
    DealsCombatDamageToPlayer(ObjectFilter),
    /// "Whenever [player] activates an ability that isn't a mana ability"
    /// (CR 602.2b; mana abilities don't use the stack, CR 605.3a).
    Activates {
        by: PlayerRel,
    },
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
    /// Either main phase ("your next main phase").
    Main,
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
    /// Layer 4: "[objects] are [types] in addition to their other types"
    /// (Opalescence; amass's type change).
    AddTypes {
        affected: ObjectFilter,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        types: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        subtypes: Vec<String>,
    },
    /// Layer 4: "[objects] aren't [types]" (Heliod: "isn't a creature").
    RemoveTypes {
        affected: ObjectFilter,
        types: Vec<String>,
    },
    /// Layer 6: "[objects] lose all abilities" (Humility, Dress Down).
    LoseAllAbilities { affected: ObjectFilter },
    /// Layer 7b: "[objects] have base power and toughness N/M" (CR 613.4b).
    SetBasePt {
        affected: ObjectFilter,
        power: BasePt,
        toughness: BasePt,
    },
    /// One ability whose effect applies in several layers; once it starts
    /// applying it keeps applying to the same objects (CR 613.6).
    Multi(Vec<StaticEffect>),
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
    /// A rule about players: "You can't lose the game" (Platinum Angel).
    PlayerRule {
        who: PlayerRel,
        rule: PlayerRuleKind,
    },
    /// "[spells] cost {N} more to cast" (Thalia; CR 601.2f).
    CostIncrease { spells: ObjectFilter, amount: u32 },
    /// "[spells] cost {N} less to cast" (Grand Arbiter Augustin IV;
    /// CR 601.2f): generic mana only.
    CostReduction { spells: ObjectFilter, amount: u32 },
    /// "[spells] can't be countered" (Allosaurus Shepherd; CR 101.2).
    SpellsCantBeCountered { spells: ObjectFilter },
    /// "Activated abilities of [objects] cost {N} less to activate. This
    /// effect can't reduce the mana in that cost to less than one mana"
    /// (Power Artifact; CR 601.2f, 602.2b).
    ActivationCostReduction { affected: ObjectFilter, amount: u32 },
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
    /// "You may cast this card from exile" (Eternal Scourge; it works in
    /// exile, CR 113.6f).
    MayCastFromExile,
    /// "This spell can't be countered" (a static ability that functions on the stack, CR 101.2).
    CantBeCountered,
    /// "Creatures entering don't cause abilities to trigger" (Torpor Orb).
    CreaturesEnteringDontTrigger,
    /// "Each player can't cast more than one spell each turn" (Rule of Law).
    OneSpellEachTurn,
    /// "Your opponents can't cast spells from anywhere other than their
    /// hands" (Drannith Magistrate).
    OpponentsCastOnlyFromHand,
    /// "Creature cards in graveyards and libraries can't enter the
    /// battlefield. Players can't cast spells from graveyards or libraries."
    /// (Grafdigger's Cage).
    GraveyardLibraryLock,
    /// "If an opponent would search a library, that player searches the top
    /// four cards of that library instead" (Aven Mindcensor).
    OpponentsSearchTopFour,
    /// "You may look at the top card of your library any time. You may play
    /// lands and cast spells from the top of your library. If you cast a
    /// spell this way, pay life equal to its mana value rather than pay its
    /// mana cost." (Bolas's Citadel).
    PlayFromTopPayingLife,
    /// "The 'legend rule' doesn't apply" (Mirror Gallery; CR 704.5j).
    NoLegendRule,
    /// "You may activate abilities of creatures you control as though those
    /// creatures had haste" (Thousand-Year Elixir; CR 302.6).
    AbilitiesAsThoughHaste,
}

/// The exceptions to a copy effect (CR 707.9). They become part of the
/// copy's copiable values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyExcept {
    /// Card types added ("except it's an artifact in addition").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    /// Subtypes added ("except it's an Illusion in addition", CR 707.9b).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtypes: Vec<String>,
    /// Abilities the copy gains (CR 707.9a), e.g. haste.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abilities: Vec<Ability>,
    /// "except it isn't legendary".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_legendary: bool,
    /// "it enters with an additional [counter] on it if it's a [type]"
    /// (Spark Double). Not a copiable value (CR 707.9).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counters: Vec<TypedCounter>,
}

/// A player choosing among the cards a search found (Intuition).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchSplit {
    pub chooser: PlayerRef,
    pub count: Quantity,
    pub chosen_to: Destination,
}

/// A choice players can vote for, and what each vote for it does.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoteChoice {
    pub name: String,
    pub per_vote: Effect,
}

/// A counter a copy enters with if it has a card type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypedCounter {
    pub if_type: String,
    pub counter: CounterKind,
}

/// Player-affecting rules an effect can create (CR 613.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlayerRuleKind {
    /// "your life total can't change".
    LifeCantChange,
    /// "you gain protection from everything" (CR 702.16j).
    ProtectionFromEverything,
    /// "You can't lose the game" (CR 104.3a: conceding still works).
    CantLose,
    /// "Your opponents can't win the game" (CR 104.2a still applies).
    OpponentsCantWin,
    /// "Spells you control can't be countered" (Veil of Summer).
    SpellsCantBeCountered,
    /// "You have hexproof from [colors]" (CR 702.11d).
    HexproofFrom(kiln_types::Colors),
    /// "[player] can't cast spells" (Silence, Orim's Chant).
    CantCastSpells,
    /// "Damage that would reduce your life total to less than 1 reduces it
    /// to 1 instead" (Angel's Grace).
    DamageCantReduceBelowOne,
    /// "Creatures [player] controls can't attack" (Orim's Chant kicked:
    /// "creatures can't attack this turn" applies to every player).
    CreaturesCantAttack,
    /// "You have no maximum hand size" (Reliquary Tower; CR 402.2, 514.1).
    NoMaximumHandSize,
    /// "Untap all permanents you control during each other player's untap
    /// step" (Seedborn Muse; CR 502.3).
    UntapDuringOthersUntapSteps,
}

/// A base power or toughness set by a static ability (CR 613.4b).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BasePt {
    Fixed(i32),
    /// The affected object's mana value (Opalescence).
    ManaValue,
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
        /// "under their owners' control" (otherwise the effect's controller
        /// puts permanents onto the battlefield under their own, CR 110.2).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        under_owners: bool,
    },
    /// "[objects] get +N/+M until end of turn" (a layer 7c effect with a duration, CR 611.2).
    Pump {
        what: ObjectSet,
        power: Quantity,
        toughness: Quantity,
    },
    /// "[object] enters tapped", as a replacement's modification (CR 614.1c).
    EntersTapped,
    /// "This enters with N [kind] counters on it" (CR 614.1c, 122.6); X is
    /// the spell's X (0 if it wasn't cast).
    EntersWithCounters {
        counter: CounterKind,
        amount: Quantity,
    },
    /// "Reveal [objects]" (CR 701.20a), e.g. the top card of a library.
    Reveal(ObjectSet),
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
    /// "Counter ... If that spell is countered this way, exile it instead of
    /// putting it into its owner's graveyard" (Force of Negation).
    CounterAndExile(ObjectSet),
    /// "Look at [player]'s hand" (Gitaxian Probe).
    LookAtHand(PlayerRef),
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
        /// "Spend this mana only to cast [spells]" (CR 106.6).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spend_only_on: Option<ObjectFilter>,
        /// "... and that spell can't be countered" (Delighted Halfling).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        uncounterable: bool,
        /// Who gets the mana, if not the controller ("its controller adds",
        /// Wild Growth: the player who tapped the land).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<PlayerRef>,
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
        /// "with mana value X or less": an amount worked out as the search
        /// happens (Green Sun's Zenith).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_mana_value: Option<Quantity>,
        /// "Search your library and/or graveyard" (Finale of Devastation).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        graveyard: bool,
        /// "put one onto [to] and the other into [rest_to]" (Cultivate): the
        /// first card found goes to `to`, the rest here.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rest_to: Option<Destination>,
        /// Name the found cards for later parts of the effect ("untap that
        /// land"; CR 400.7j finds them in the public zone they went to).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remember: Option<String>,
        /// "up to N cards": finding fewer is allowed (CR 701.23d).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        up_to: bool,
        /// "cards with different names" (Gifts Ungiven).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        different_names: bool,
        /// "[player] chooses N of them. Put the chosen cards [there] and the
        /// rest [`to`]" (Intuition, Gifts Ungiven).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        split: Option<SearchSplit>,
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
    /// "Tap or untap [objects]": the controller chooses which as it
    /// resolves (Pestermite).
    TapOrUntap(ObjectSet),
    /// "End the turn" (CR 724.1; Sundial of the Infinite).
    EndTheTurn,
    /// "Exile [objects], then return those cards to the battlefield under
    /// their owners' control" (Displacer Kitten; new objects, CR 400.7).
    Flicker(ObjectSet),
    /// "After this phase, there is an additional combat phase" (CR 500.8).
    AdditionalCombatPhase,
    /// "You control [player] during that player's next turn" (Mindslaver;
    /// CR 723.1).
    ControlNextTurn(PlayerRef),
    /// Voting (CR 701.38): starting with the controller, each player in turn
    /// order votes for one choice; then each choice's effect happens once
    /// per vote for it, in the listed order (Council's dilemma).
    Vote {
        choices: Vec<VoteChoice>,
        /// "While voting, you may vote an additional time" (CR 701.38d).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        extra_vote: bool,
    },
    /// "Until end of turn, [objects] have base power and toughness P/T and
    /// become [subtypes] in addition to their other types" (Allosaurus
    /// Shepherd; CR 613.4b, 613.1d, 611.2c).
    Become {
        what: ObjectSet,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_pt: Option<[i32; 2]>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        add_subtypes: Vec<String>,
    },
    /// "Copy that ability", the activated ability that triggered this one
    /// (Rings of Brighthearth; CR 707.10). Its controller may choose new
    /// targets for the copy (CR 707.10c).
    CopyTriggeringAbility {
        new_targets: bool,
    },
    CreateToken {
        token: TokenSpec,
        count: Quantity,
        controller: PlayerRef,
        /// "that's tapped and attacking" (CR 508.4).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        attacking: bool,
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
    /// "[objects] phase out" (CR 702.26).
    PhaseOut {
        what: ObjectSet,
    },
    /// "Until your next turn, [player] ..." (CR 611.2a): a rule for a player.
    PlayerRule {
        who: PlayerRef,
        rule: PlayerRuleKind,
        /// "this turn" / "until end of turn" instead of "until your next turn".
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        until_end_of_turn: bool,
    },
    /// "Exchange control of [a] and [b]" (CR 701.12a-b); `otherwise` runs if
    /// no exchange happens ("If you don't or can't make an exchange, ...").
    ExchangeControl {
        a: ObjectRef,
        b: ObjectRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        otherwise: Option<Box<Effect>>,
    },
    /// "Attach this to [object]" (CR 701.3).
    Attach {
        to: ObjectRef,
    },
    /// "Return enchanted creature card to the battlefield under your control
    /// and attach this to it" (Animate Dead).
    ReturnAttached,
    /// "Gain control of [objects]" (layer 2, CR 613.1b), until end of turn or for good.
    GainControl {
        what: ObjectSet,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        until_end_of_turn: bool,
        /// Who gains control, if not the effect's controller ("an opponent
        /// gains control of this artifact": a chosen opponent).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<PlayerRef>,
    },
    /// "[effect]. You may repeat this process any number of times" (Ad Nauseam).
    Repeat(Box<Effect>),
    /// "[objects] gain [keyword] until end of turn" (layer 6).
    GainKeyword {
        what: ObjectSet,
        keyword: Keyword,
    },
    /// "You may have this enter as a copy of [object] on the battlefield,
    /// except ..." (CR 707.9, 614.1c): the instead-effect of a self-replacement.
    EnterAsCopy {
        of: ObjectFilter,
        except: CopyExcept,
    },
    /// "Create a token that's a copy of [object], except ..." (CR 707.9, 111.10).
    CreateTokenCopy {
        of: ObjectSet,
        #[serde(default)]
        except: CopyExcept,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remember: Option<String>,
    },
    /// "Copy [spell]. You may choose new targets for the copy" (CR 707.10, 707.10c).
    CopySpell {
        what: ObjectSet,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        new_targets: bool,
        /// How many copies (storm); one if omitted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<Quantity>,
        /// Who controls the copies, if not the effect's controller (Chain of Vapor).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        controller: Option<PlayerRef>,
    },
    /// "Until end of turn, you may cast that card" (Ragavan, Silas Renn).
    MayCastThisTurn(ObjectSet),
    /// "You may choose new targets for [spell or ability]" (CR 115.7d;
    /// Deflecting Swat): each target may be kept or changed to a legal one.
    ChangeTargets(ObjectRef),
    /// "Copy [object]": a copy of a card, created in the zone the card is in
    /// (CR 707.12), named `remember` for the rest of the effect.
    CopyCard {
        what: ObjectSet,
        remember: String,
    },
    /// "Cast [object] without paying its mana cost" during resolution
    /// (CR 608.2g, 118.9). Usually inside a `May`.
    Cast {
        what: ObjectSet,
        without_paying: bool,
    },
    /// "[who] wins the game" (CR 104.2b).
    WinGame(PlayerRef),
    /// "[who] loses the game" (CR 104.3e).
    LoseGame(PlayerRef),
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
    /// "each [permanent]" (Blasphemous Act: "deals 13 damage to each creature").
    Each(ObjectFilter),
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
    /// "Put it onto the battlefield tapped" (CR 110.5b).
    BattlefieldTapped,
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
    #[serde(rename = "age")]
    Age,
    #[serde(rename = "luck")]
    Luck,
    #[serde(rename = "wish")]
    Wish,
    #[serde(rename = "chorus")]
    Chorus,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ChoiceSpec {
    CardName,
    Color,
    Number {
        min: i64,
        max: i64,
    },
    /// An opponent still in the game.
    Opponent,
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
    /// An amount of {C} ("add an amount of {C} equal to ...").
    Colorless(Quantity),
    /// One mana of any color among [objects] (Mox Amber).
    ColorsAmong(ObjectFilter),
    /// One mana of any color a land an opponent controls could produce
    /// (Fellwar Stone, Exotic Orchard; CR 106.7).
    OpponentLandColors,
    /// Fixed symbols added [times] times ("Add {G} for each creature you control").
    Times { mana: String, times: Quantity },
    /// One mana of the color chosen as the source entered (Utopia Sprawl).
    ChosenColor(String),
    /// One mana of each color among [objects] (Bloom Tender's vivid).
    EachColorAmong(ObjectFilter),
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
    /// The object or card this is attached to ("enchanted creature", CR 303.4b).
    AttachedTo,
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
    /// Every card in a player's zone ("target player's graveyard").
    Zone {
        owner: PlayerRef,
        zone: ZoneName,
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
    /// A player chosen earlier in the effect (`choose: opponent`, stored under this name).
    Chosen(String),
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
    /// An activated or triggered ability on the stack (Stifle; mana
    /// abilities never are, CR 605.3b).
    Ability,
    /// A spell matching the filter, or an ability on the stack (Deflecting Swat).
    SpellOrAbility(ObjectFilter),
}

/// Object characteristics a filter can test. All present fields must match.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectFilter {
    /// Only this object.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub this: bool,
    /// The object the source is attached to ("enchanted creature",
    /// "equipped creature"; CR 303.4b, 301.5a).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub attached: bool,
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
    /// Whose card it is ("an opponent's graveyard": cards opponents own).
    #[serde(default, skip_serializing_if = "is_default")]
    pub owner: PlayerRel,
    /// "non[color]": none of these color letters (Snuff Out: "nonblack").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_color: Option<String>,
    /// "with mana value, power, or toughness equal to the chosen number"
    /// (Talion): the source's choice under this name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_is: Option<String>,
    /// "with the same name as this" ("each card named Rite of Flame", CR 201.2).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub same_name: bool,
    /// "with mana value N or greater" (CR 202.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_mana_value: Option<u32>,
    /// "with mana value N or less" (CR 202.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_mana_value: Option<u32>,
    /// "attacking creatures" (CR 506.4).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub attacking: bool,
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
    /// "That much": the amount from the trigger event (life gained or lost).
    TriggeringAmount,
    /// Spells [players] cast this turn, of a color if given (Veil of Summer).
    SpellsCastThisTurn {
        by: PlayerRel,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
    /// The most spells any one opponent cast this turn (Mindbreak Trap).
    MostSpellsCastByAnOpponent,
    /// Half of an amount, rounded up (Peer into the Abyss).
    HalfRoundedUp(Box<Quantity>),
    /// A player's life total.
    LifeOf(PlayerRef),
    /// The mana value of the top card of a player's library (Ad Nauseam).
    TopOfLibraryManaValue(PlayerRef),
    /// "the number of [objects] [player] controls" (Carpet of Flowers: Islands
    /// target opponent controls).
    CountOf {
        filter: ObjectFilter,
        controller: PlayerRef,
    },
    /// The negation of a quantity ("-X/-X").
    Neg(Box<Quantity>),
    /// The number of opponents still in the game (bond lands: "two or more
    /// opponents"; CR 102.2).
    Opponents,
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
    /// "Activate only during combat" (the combat phase, CR 506.1).
    InCombat,
    /// left ≥ right.
    AtLeast(Quantity, Quantity),
    /// "if you haven't added mana with this ability this turn" (Carpet of Flowers).
    UnusedThisTurn,
    /// "If this spell was kicked" (CR 702.33d).
    Kicked,
    /// "if it's [filter]": the object in a target slot (Pyroblast: "if it's blue").
    TargetMatches {
        target: u8,
        filter: ObjectFilter,
    },
    /// "If [the top card of who's library] is a [filter] card".
    TopOfLibraryMatches {
        who: PlayerRef,
        filter: ObjectFilter,
    },
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
    /// A loyalty cost: put (+N) or remove (−N) loyalty counters (CR 606.4).
    Loyalty(i32),
    PayLife(Quantity),
    Sacrifice(ObjectSet),
    Discard(DiscardSpec),
    /// Exile a card from your hand matching the filter ("exile a blue card from your hand").
    ExileFromHand(ObjectFilter),
    /// Tap N untapped [objects] you control ("Tap an untapped creature you
    /// control", Springleaf Drum). Summoning sickness doesn't apply: they
    /// aren't paying their own {T} (CR 302.6).
    TapUntapped {
        what: ObjectFilter,
        #[serde(default = "one")]
        count: u32,
    },
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
    /// A mana cost paid `times` times (cumulative upkeep, CR 702.24a).
    ManaRepeated {
        mana: String,
        times: Quantity,
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
                    Ability::OpeningHand(o) => collect_escapes(&o.effect, &mut out),
                    Ability::Keyword(_) | Ability::Static(_) | Ability::AsEnters { .. } => {}
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
        Effect::ExchangeControl { otherwise, .. } => otherwise.as_deref().into_iter().collect(),
        Effect::Repeat(x) => vec![x],
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
        | Effect::Untap(ObjectSet::One(ObjectRef::Target(n)))
        | Effect::TapOrUntap(ObjectSet::One(ObjectRef::Target(n))) => Some(*n),
        Effect::Exile {
            what: ObjectSet::One(ObjectRef::Target(n)),
            ..
        } => Some(*n),
        Effect::GainLife {
            who: PlayerRef::ControllerOf(ObjectRef::Target(n)) | PlayerRef::Target(n),
            ..
        } => Some(*n),
        Effect::ExchangeControl { a, b, .. } => [a, b]
            .into_iter()
            .filter_map(|r| match r {
                ObjectRef::Target(n) => Some(*n),
                _ => None,
            })
            .max(),
        _ => None,
    };
    children(e)
        .into_iter()
        .filter_map(max_target_ref)
        .chain(own)
        .max()
}

fn one() -> u32 {
    1
}
