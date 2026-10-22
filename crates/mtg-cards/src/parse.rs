//! Oracle-text parser **prototype** (M3 spike, step A3).
//!
//! A hand-written compositional recursive-descent parser: small sub-parsers
//! for filters, targets, quantities, costs and mana are composed inside
//! templates for effects, triggers, keywords and statics. It covers the most
//! common phrase families only; any line it can't parse completely is
//! reported as unsupported, never approximated (Design Principle 9).
//!
//! Input lines are reminder-stripped Oracle text. Self-references (the card's
//! name, its short name, "this creature", …) are normalized to `~` and the
//! text is lowercased before parsing.

use crate::ir::{
    Ability, ActivatedAbility, CostPart, Destination, DiscardSpec, Effect, Keyword, ManaSpec,
    ObjectFilter, ObjectRef, ObjectSet, PlayerRef, PlayerRel, Quantity, Recipient, ReplaceEvent,
    ReplacementAbility, SpellAbility, StaticAbility, StaticEffect, StepName, TargetKind,
    TargetSpec, Timing, TokenSpec, Trigger, TriggeredAbility, ZoneName,
};

/// The parse of one face.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedFace {
    pub abilities: Vec<Ability>,
    /// One entry per input line: the normalized line and whether it parsed
    /// (and, for [`parse_face_checked`], passed the renderer lint).
    pub lines: Vec<(String, bool)>,
    /// Lines that parsed but whose rendering didn't match (checked parses only).
    pub lint_failures: Vec<String>,
}

impl ParsedFace {
    pub fn complete(&self) -> bool {
        self.lines.iter().all(|(_, ok)| *ok)
    }
}

/// Parses one face's reminder-stripped text. `is_spell` = instant or sorcery
/// (its non-ability lines form the spell ability).
pub fn parse_face(text: &str, name: &str, is_spell: bool) -> ParsedFace {
    parse_face_with(text, name, is_spell, false)
}

/// Like [`parse_face`], but a line counts as parsed only if rendering its IR
/// matches the line (the mandatory renderer lint, ADR-0007).
pub fn parse_face_checked(text: &str, name: &str, is_spell: bool) -> ParsedFace {
    parse_face_with(text, name, is_spell, true)
}

fn parse_face_with(text: &str, name: &str, is_spell: bool, lint: bool) -> ParsedFace {
    let mut out = ParsedFace::default();
    let mut spell: Option<SpellAbility> = None;
    for raw in text.lines() {
        let line = normalize_line(raw, name);
        if line.is_empty() {
            continue;
        }
        let parsed = parse_line(&line, is_spell);
        if lint
            && let Some(ir) = &parsed
            && !crate::render::matches(&crate::render::render_line(ir), &line)
        {
            out.lint_failures.push(line.clone());
            out.lines.push((line, false));
            continue;
        }
        match parsed {
            Some(LineIr::Abilities(a)) => {
                out.abilities.extend(a);
                out.lines.push((line, true));
            }
            Some(LineIr::Spell(targets, effect)) => {
                let s = spell.get_or_insert_with(|| SpellAbility {
                    targets: Vec::new(),
                    effect: Effect::Seq(Vec::new()),
                    alternative_costs: Vec::new(),
                    additional_costs: Vec::new(),
                });
                let offset = u8::try_from(s.targets.len()).unwrap_or(0);
                s.targets.extend(targets);
                let effect = shift_targets(effect, offset);
                s.effect = match std::mem::replace(&mut s.effect, Effect::Seq(Vec::new())) {
                    Effect::Seq(mut v) if !v.is_empty() || offset == 0 => {
                        v.push(effect);
                        if v.len() == 1 {
                            v.pop().unwrap_or(Effect::Seq(Vec::new()))
                        } else {
                            Effect::Seq(v)
                        }
                    }
                    prev => Effect::Seq(vec![prev, effect]),
                };
                out.lines.push((line, true));
            }
            None => out.lines.push((line, false)),
        }
    }
    if let Some(s) = spell {
        out.abilities.push(Ability::Spell(s));
    }
    out
}

/// Lowercases and replaces self-references with `~`.
pub fn normalize_line(line: &str, name: &str) -> String {
    let mut l = line.trim().to_lowercase();
    let name_l = name.to_lowercase();
    for face in name_l.split(" // ") {
        if !face.is_empty() {
            l = l.replace(face, "~");
        }
        if let Some((short, _)) = face.split_once(", ")
            && short.len() > 2
        {
            l = replace_word(&l, short, "~");
        }
    }
    for this in [
        "this creature",
        "this artifact",
        "this enchantment",
        "this land",
        "this permanent",
        "this spell",
        "this card",
        "this planeswalker",
        "this battle",
        "this vehicle",
        "this equipment",
        "this aura",
    ] {
        l = l.replace(this, "~");
    }
    l
}

fn replace_word(text: &str, word: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(word) {
        let before_ok = rest[..pos]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after = &rest[pos + word.len()..];
        let after_ok = after.chars().next().is_none_or(|c| !c.is_alphanumeric());
        out.push_str(&rest[..pos]);
        out.push_str(if before_ok && after_ok { with } else { word });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The IR of one Oracle line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LineIr {
    Abilities(Vec<Ability>),
    Spell(Vec<TargetSpec>, Effect),
}

/// Parses one normalized line (see [`normalize_line`]).
pub fn parse_line(line: &str, is_spell: bool) -> Option<LineIr> {
    if let Some(k) = keyword_line(line) {
        return Some(LineIr::Abilities(k));
    }
    if let Some(a) = triggered(line) {
        return Some(LineIr::Abilities(vec![a]));
    }
    if let Some(a) = activated(line) {
        return Some(LineIr::Abilities(vec![a]));
    }
    if let Some(a) = static_line(line) {
        return Some(LineIr::Abilities(vec![a]));
    }
    if is_spell {
        let mut ctx = Ctx::default();
        let e = effect_text(line, &mut ctx)?;
        return Some(LineIr::Spell(ctx.targets, e));
    }
    None
}

// ---- cursor -------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Cur<'a> {
    s: &'a str,
}

impl<'a> Cur<'a> {
    fn new(s: &'a str) -> Self {
        Self { s }
    }
    fn lit(&mut self, p: &str) -> bool {
        match self.s.strip_prefix(p) {
            Some(r) => {
                self.s = r;
                true
            }
            None => false,
        }
    }
    fn done(&self) -> bool {
        self.s.is_empty()
    }
    fn number(&mut self) -> Option<i64> {
        const WORDS: [&str; 11] = [
            "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        ];
        for (i, w) in WORDS.iter().enumerate() {
            if self.word(w) {
                return Some(i64::try_from(i).unwrap_or(0));
            }
        }
        if self.word("a") || self.word("an") {
            return Some(1);
        }
        let digits = self.s.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let n = self.s[..digits].parse().ok()?;
        self.s = &self.s[digits..];
        Some(n)
    }
    /// Matches `w` only at a word boundary.
    fn word(&mut self, w: &str) -> bool {
        if let Some(r) = self.s.strip_prefix(w)
            && r.chars().next().is_none_or(|c| !c.is_alphanumeric())
        {
            self.s = r;
            return true;
        }
        false
    }
    /// A run of mana symbols, returned uppercased: `{2}{U}`.
    fn mana(&mut self) -> Option<String> {
        let mut out = String::new();
        let mut c = *self;
        while c.s.starts_with('{') {
            let close = c.s.find('}')?;
            let inner = &c.s[1..close];
            if !inner
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '/')
                || inner == "t"
                || inner == "q"
            {
                break;
            }
            out.push('{');
            out.push_str(&inner.to_uppercase());
            out.push('}');
            c.s = &c.s[close + 1..];
        }
        if out.is_empty() {
            return None;
        }
        *self = c;
        Some(out)
    }
}

/// Target slots allocated while parsing one ability.
#[derive(Default)]
struct Ctx {
    targets: Vec<TargetSpec>,
}

impl Ctx {
    fn add(&mut self, t: TargetSpec) -> u8 {
        self.targets.push(t);
        u8::try_from(self.targets.len() - 1).unwrap_or(0)
    }
    fn last(&self) -> Option<u8> {
        self.targets
            .len()
            .checked_sub(1)
            .and_then(|n| u8::try_from(n).ok())
    }
}

fn shift_targets(e: Effect, by: u8) -> Effect {
    if by == 0 {
        return e;
    }
    // Spell lines are parsed with fresh contexts; renumber later lines' targets.
    let json = serde_json::to_string(&e).unwrap_or_default();
    let mut out = String::with_capacity(json.len());
    let mut rest = json.as_str();
    while let Some(pos) = rest.find("\"target\":") {
        out.push_str(&rest[..pos + 9]);
        rest = &rest[pos + 9..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 {
            let n: u8 = rest[..digits].parse().unwrap_or(0);
            out.push_str(&(n + by).to_string());
            rest = &rest[digits..];
        }
    }
    out.push_str(rest);
    serde_json::from_str(&out).unwrap_or(e)
}

// ---- filters ------------------------------------------------------------------

const TYPES: [&str; 8] = [
    "artifact",
    "battle",
    "creature",
    "enchantment",
    "instant",
    "land",
    "planeswalker",
    "sorcery",
];
const COLORS: [(&str, &str); 5] = [
    ("white", "W"),
    ("blue", "U"),
    ("black", "B"),
    ("red", "R"),
    ("green", "G"),
];

/// A noun phrase describing objects: "another nonland permanent you control",
/// "artifact or enchantment", "creatures". Returns the filter and whether it was plural.
fn filter(c: &mut Cur) -> Option<(ObjectFilter, bool)> {
    let mut f = ObjectFilter::default();
    let save = *c;
    // Position just after the last adjective, before its trailing space, so a
    // following "spell" stays matchable as " spell".
    let mut after_adjective: Option<Cur> = None;
    loop {
        if c.word("another") || c.word("other") {
            f.other = true;
        } else if c.word("basic") {
            f.supertypes.push("basic".into());
        } else if c.word("nonbasic") {
            f.not_supertypes.push("basic".into());
        } else if c.word("legendary") {
            f.supertypes.push("legendary".into());
        } else if let Some(i) = COLORS.iter().position(|(w, _)| c.word(w)) {
            f.color = Some(COLORS[i].1.into());
        } else if let Some(t) = TYPES.iter().find(|t| c.word(&format!("non{t}"))) {
            f.not_types.push((*t).into());
        } else {
            break;
        }
        after_adjective = Some(*c);
        if !c.lit(" ") {
            *c = save;
            return None;
        }
    }
    let mut plural = false;
    let mut types = Vec::new();
    loop {
        let found = TYPES.iter().find_map(|t| {
            if c.word(&format!("{t}s")) {
                Some((*t, true))
            } else if c.word(t) {
                Some((*t, false))
            } else {
                None
            }
        });
        if let Some((t, p)) = found {
            types.push(t.to_owned());
            plural |= p;
        } else if c.word("permanents") || c.word("cards") {
            plural = true;
        } else if c.word("permanent") || c.word("card") {
        } else {
            break;
        }
        // Continue a type list ("artifact, creature, or enchantment") only if a type noun follows.
        let mut probe = *c;
        if (probe.lit(", or ") || probe.lit(" or ") || probe.lit(", "))
            && (TYPES.iter().any(|t| probe.s.starts_with(t))
                || probe.s.starts_with("permanent")
                || probe.s.starts_with("card"))
        {
            *c = probe;
        } else {
            break;
        }
    }
    if types.is_empty()
        && !plural
        && !save.s.starts_with("permanent")
        && !save.s.starts_with("card")
    {
        match after_adjective {
            // Adjectives only ("noncreature" before "spell").
            Some(a) => *c = a,
            None => {
                *c = save;
                return None;
            }
        }
    }
    f.types = types;
    // A trailing "card" after types ("creature card").
    let _ = c.word(" card") || c.word(" cards");
    let mut probe = *c;
    if probe.lit(" you control") {
        f.controller = PlayerRel::You;
        *c = probe;
    } else if probe.lit(" an opponent controls") || probe.lit(" your opponents control") {
        f.controller = PlayerRel::Opponent;
        *c = probe;
    }
    Some((f, plural))
}

// ---- targets and recipients ----------------------------------------------------

/// "any target", "target creature you control", "target player", "up to one target …".
fn target(c: &mut Cur, ctx: &mut Ctx) -> Option<u8> {
    let save = *c;
    let up_to = if c.lit("up to one ") { Some(1) } else { None };
    if c.lit("any target") {
        return Some(ctx.add(TargetSpec {
            what: TargetKind::Any,
            up_to,
        }));
    }
    if !(c.lit("target ") || c.lit("another target ")) {
        *c = save;
        return None;
    }
    let other = save.s.starts_with("another");
    if c.word("player") {
        return Some(ctx.add(TargetSpec {
            what: TargetKind::Player(PlayerRel::Any),
            up_to,
        }));
    }
    if c.word("opponent") {
        return Some(ctx.add(TargetSpec {
            what: TargetKind::Player(PlayerRel::Opponent),
            up_to,
        }));
    }
    let mut probe = *c;
    if let Some((mut f, false)) = filter(&mut probe) {
        f.other |= other;
        if probe.lit(" spell") {
            *c = probe;
            return Some(ctx.add(TargetSpec {
                what: TargetKind::Spell(f),
                up_to,
            }));
        }
        *c = probe;
        return Some(ctx.add(TargetSpec {
            what: TargetKind::Object(f),
            up_to,
        }));
    }
    if c.lit("spell") {
        return Some(ctx.add(TargetSpec {
            what: TargetKind::Spell(ObjectFilter::default()),
            up_to,
        }));
    }
    *c = save;
    None
}

/// Objects acted on: a target, `~`, "it", or "all/each <filter>".
fn objects(c: &mut Cur, ctx: &mut Ctx) -> Option<ObjectSet> {
    if c.lit("~") {
        return Some(ObjectSet::One(ObjectRef::This));
    }
    if c.word("it") {
        return ctx
            .last()
            .map(|t| ObjectSet::One(ObjectRef::Target(t)))
            .or(Some(ObjectSet::One(ObjectRef::This)));
    }
    if c.lit("all ") || c.lit("each ") {
        let (f, _) = filter(c)?;
        return Some(ObjectSet::All(f));
    }
    target(c, ctx).map(|t| ObjectSet::One(ObjectRef::Target(t)))
}

fn player(c: &mut Cur, ctx: &mut Ctx) -> Option<PlayerRef> {
    if c.word("you") {
        return Some(PlayerRef::You);
    }
    if c.lit("its controller") {
        return Some(
            ctx.last()
                .map_or(PlayerRef::ControllerOf(ObjectRef::This), |t| {
                    PlayerRef::ControllerOf(ObjectRef::Target(t))
                }),
        );
    }
    if c.lit("that player") {
        return Some(PlayerRef::Triggering);
    }
    target(c, ctx).map(PlayerRef::Target)
}

// ---- costs and mana -------------------------------------------------------------

fn cost_list(s: &str) -> Option<Vec<CostPart>> {
    let mut out = Vec::new();
    for part in s.split(", ") {
        let mut c = Cur::new(part);
        let p = if c.lit("{t}") {
            CostPart::Tap
        } else if c.lit("{q}") {
            CostPart::Untap
        } else if let Some(m) = c.mana() {
            CostPart::Mana(m)
        } else if c.lit("sacrifice ~") {
            CostPart::Sacrifice(ObjectSet::One(ObjectRef::This))
        } else if c.lit("sacrifice ") {
            c.number()?;
            c.lit(" ");
            let (f, _) = filter(&mut c)?;
            CostPart::Sacrifice(ObjectSet::ChosenFrom {
                who: PlayerRef::You,
                zone: ZoneName::Battlefield,
                filter: f,
                count: Quantity::Fixed(1),
            })
        } else if c.lit("pay ") {
            let n = c.number()?;
            if !c.lit(" life") {
                return None;
            }
            CostPart::PayLife(Quantity::Fixed(n))
        } else if c.lit("discard your hand") {
            CostPart::Discard(DiscardSpec::Hand)
        } else if c.lit("discard ") {
            let n = c.number()?;
            if !(c.lit(" card") || c.lit(" cards")) {
                return None;
            }
            CostPart::Discard(DiscardSpec::Chosen {
                count: Quantity::Fixed(n),
                filter: ObjectFilter::default(),
            })
        } else {
            return None;
        };
        if !c.done() {
            return None;
        }
        out.push(p);
    }
    Some(out)
}

fn mana_spec(c: &mut Cur) -> Option<ManaSpec> {
    if c.lit("one mana of any color in your commander's color identity") {
        return Some(ManaSpec::CommanderIdentity);
    }
    if c.lit("one mana of any color") {
        return Some(ManaSpec::AnyColor(Quantity::Fixed(1)));
    }
    let save = *c;
    if let Some(n) = c.number() {
        if c.lit(" mana of any one color") {
            return Some(ManaSpec::AnyOneColor(Quantity::Fixed(n)));
        }
        if c.lit(" mana in any combination of colors") {
            return Some(ManaSpec::AnyColor(Quantity::Fixed(n)));
        }
        *c = save;
    }
    let first = c.mana()?;
    let mut options = vec![first];
    loop {
        if c.lit(", or ") || c.lit(" or ") || c.lit(", ") {
            options.push(c.mana()?);
        } else {
            break;
        }
    }
    Some(if options.len() == 1 {
        ManaSpec::Fixed(options.remove(0))
    } else {
        ManaSpec::OneOf(options)
    })
}

// ---- effects -------------------------------------------------------------------

/// One or more sentences of effect text.
fn effect_text(text: &str, ctx: &mut Ctx) -> Option<Effect> {
    let text = text.trim().trim_end_matches('.');
    let mut effects = Vec::new();
    for sentence in text.split(". ") {
        for clause in sentence.split(", then ") {
            effects.push(clause_with_and(clause.trim_start_matches("then "), ctx)?);
        }
    }
    Some(if effects.len() == 1 {
        effects.remove(0)
    } else {
        Effect::Seq(effects)
    })
}

fn clause_with_and(clause: &str, ctx: &mut Ctx) -> Option<Effect> {
    let mut c = Cur::new(clause);
    if let Some(e) = clause_full(&mut c, ctx) {
        return Some(e);
    }
    // "you draw two cards and lose 2 life": split once on " and " with the subject carried over.
    let (a, b) = clause.split_once(" and ")?;
    let first = clause_full(&mut Cur::new(a), ctx)?;
    let subject = if a.starts_with("you ") { "you " } else { "" };
    let second = clause_full(&mut Cur::new(&format!("{subject}{b}")), ctx)?;
    Some(Effect::Seq(vec![first, second]))
}

fn clause_full(c: &mut Cur, ctx: &mut Ctx) -> Option<Effect> {
    let mut probe = *c;
    let e = clause(&mut probe, ctx)?;
    if probe.done() {
        *c = probe;
        Some(e)
    } else {
        None
    }
}

fn clause(c: &mut Cur, ctx: &mut Ctx) -> Option<Effect> {
    if c.lit("you may ") {
        return clause(c, ctx).map(|e| Effect::May(Box::new(e)));
    }
    if c.lit("~ deals ") {
        let n = c.number()?;
        if !c.lit(" damage to ") {
            return None;
        }
        if c.lit("each opponent") {
            return Some(Effect::ForEachPlayer {
                players: PlayerRel::Opponent,
                effect: Box::new(Effect::DealDamage {
                    to: Recipient::Player(PlayerRef::Each),
                    amount: Quantity::Fixed(n),
                }),
            });
        }
        if c.lit("you") {
            return Some(Effect::DealDamage {
                to: Recipient::Player(PlayerRef::You),
                amount: Quantity::Fixed(n),
            });
        }
        let t = target(c, ctx)?;
        return Some(Effect::DealDamage {
            to: Recipient::Target(t),
            amount: Quantity::Fixed(n),
        });
    }
    if c.lit("destroy ") {
        return objects(c, ctx).map(Effect::Destroy);
    }
    if c.lit("exile ") {
        return objects(c, ctx).map(|what| Effect::Exile {
            what,
            face_down: false,
            remember: None,
        });
    }
    if c.lit("tap ") {
        return objects(c, ctx).map(Effect::Tap);
    }
    if c.lit("untap ") {
        return objects(c, ctx).map(Effect::Untap);
    }
    if c.lit("return ") {
        let what = objects(c, ctx)?;
        if !(c.lit(" to its owner's hand") || c.lit(" to their owners' hands")) {
            return None;
        }
        return Some(Effect::Move {
            what,
            to: Destination::Hand,
        });
    }
    if c.lit("counter ") {
        let what = objects(c, ctx)?;
        let mut probe = *c;
        if probe.lit(" unless its controller pays ") {
            let cost = probe.mana()?;
            *c = probe;
            let t = ctx.last()?;
            return Some(Effect::Unless {
                payer: PlayerRef::ControllerOf(ObjectRef::Target(t)),
                cost: vec![CostPart::Mana(cost)],
                effect: Box::new(Effect::Counter(what)),
            });
        }
        return Some(Effect::Counter(what));
    }
    if c.lit("add ") {
        return mana_spec(c).map(|mana| Effect::AddMana {
            mana,
            spend_only_on: None,
        });
    }
    if c.lit("shuffle your library") || c.word("shuffle") {
        return Some(Effect::Shuffle(PlayerRef::You));
    }
    if c.lit("scry ") {
        return c.number().map(|n| Effect::Scry {
            who: PlayerRef::You,
            amount: Quantity::Fixed(n),
        });
    }
    if c.lit("draw ") {
        return draw_amount(c).map(|n| Effect::Draw {
            who: PlayerRef::You,
            amount: Quantity::Fixed(n),
        });
    }
    if c.lit("each opponent loses ") || c.lit("each opponent loses ") {
        let n = c.number()?;
        return c.lit(" life").then(|| Effect::ForEachPlayer {
            players: PlayerRel::Opponent,
            effect: Box::new(Effect::LoseLife {
                who: PlayerRef::Each,
                amount: Quantity::Fixed(n),
            }),
        });
    }
    if c.lit("each player draws ") {
        let n = draw_amount(c)?;
        return Some(Effect::ForEachPlayer {
            players: PlayerRel::Any,
            effect: Box::new(Effect::Draw {
                who: PlayerRef::Each,
                amount: Quantity::Fixed(n),
            }),
        });
    }
    if c.lit("search your library for ") {
        return search(c);
    }
    if c.lit("create ") {
        let n = c.number()?;
        if !c.lit(" ") {
            return None;
        }
        let name = ["treasure", "clue", "food", "blood", "gold"]
            .iter()
            .find(|t| c.word(t))?;
        if !(c.lit(" token") || c.lit(" tokens")) {
            return None;
        }
        let mut cap = name.to_string();
        cap[..1].make_ascii_uppercase();
        return Some(Effect::CreateToken {
            token: TokenSpec::Predefined(cap),
            count: Quantity::Fixed(n),
            controller: PlayerRef::You,
        });
    }
    // "<objects> get(s) +N/+M until end of turn"
    let mut probe = *c;
    if let Some(what) = objects(&mut probe, ctx)
        && (probe.lit(" gets ") || probe.lit(" get "))
        && let Some((p, t)) = pt_mod(&mut probe)
        && probe.lit(" until end of turn")
    {
        *c = probe;
        return Some(Effect::Pump {
            what,
            power: p,
            toughness: t,
        });
    }
    // "<player> draws/gains/loses …"
    let mut probe = *c;
    if let Some(who) = player(&mut probe, ctx) {
        let e = if probe.lit(" draws ") || probe.lit(" draw ") {
            draw_amount(&mut probe).map(|n| Effect::Draw {
                who: who.clone(),
                amount: Quantity::Fixed(n),
            })
        } else if probe.lit(" gains life equal to its power") {
            let t = ctx.last()?;
            Some(Effect::GainLife {
                who: who.clone(),
                amount: Quantity::Expr(crate::ir::QuantityExpr::PowerOf(ObjectRef::Target(t))),
            })
        } else if probe.lit(" gains ") || probe.lit(" gain ") {
            let n = probe.number()?;
            probe.lit(" life").then(|| Effect::GainLife {
                who: who.clone(),
                amount: Quantity::Fixed(n),
            })
        } else if probe.lit(" loses ") || probe.lit(" lose ") {
            let n = probe.number()?;
            probe.lit(" life").then(|| Effect::LoseLife {
                who: who.clone(),
                amount: Quantity::Fixed(n),
            })
        } else {
            None
        };
        if let Some(e) = e {
            *c = probe;
            return Some(e);
        }
    }
    None
}

fn draw_amount(c: &mut Cur) -> Option<i64> {
    let n = c.number()?;
    (c.lit(" cards") || c.lit(" card")).then_some(n)
}

fn pt_mod(c: &mut Cur) -> Option<(i32, i32)> {
    let sign = |c: &mut Cur| {
        if c.lit("+") {
            Some(1)
        } else if c.lit("-") {
            Some(-1)
        } else {
            None
        }
    };
    let sp = sign(c)?;
    let p = i32::try_from(c.number()?).ok()? * sp;
    if !c.lit("/") {
        return None;
    }
    let st = sign(c)?;
    let t = i32::try_from(c.number()?).ok()? * st;
    Some((p, t))
}

/// "a [basic land] card, [reveal it,] put it into your hand, then shuffle" (the
/// ", then shuffle" was split off by the caller as a separate clause).
fn search(c: &mut Cur) -> Option<Effect> {
    c.number()?;
    c.lit(" ");
    let (f, _) = filter(c)?;
    let reveal = c.lit(", reveal it");
    let to = if c.lit(", put it into your hand") {
        Destination::Hand
    } else if c.lit(", put it onto the battlefield") {
        Destination::Battlefield
    } else {
        return None;
    };
    Some(Effect::Search {
        who: PlayerRef::You,
        filter: f,
        count: Quantity::Fixed(1),
        to,
        reveal,
    })
}

// ---- line kinds ------------------------------------------------------------------

fn keyword(c: &mut Cur) -> Option<Keyword> {
    const SIMPLE: [(&str, Keyword); 15] = [
        ("deathtouch", Keyword::Deathtouch),
        ("defender", Keyword::Defender),
        ("double strike", Keyword::DoubleStrike),
        ("first strike", Keyword::FirstStrike),
        ("flash", Keyword::Flash),
        ("flying", Keyword::Flying),
        ("haste", Keyword::Haste),
        ("hexproof", Keyword::Hexproof),
        ("indestructible", Keyword::Indestructible),
        ("lifelink", Keyword::Lifelink),
        ("menace", Keyword::Menace),
        ("reach", Keyword::Reach),
        ("shroud", Keyword::Shroud),
        ("trample", Keyword::Trample),
        ("vigilance", Keyword::Vigilance),
    ];
    for (w, k) in SIMPLE {
        if c.word(w) {
            return Some(k);
        }
    }
    if c.lit("ward ") {
        return c.mana().map(|m| Keyword::Ward(vec![CostPart::Mana(m)]));
    }
    if c.lit("cumulative upkeep ") {
        return c
            .mana()
            .map(|m| Keyword::CumulativeUpkeep(vec![CostPart::Mana(m)]));
    }
    if c.lit("protection from ") {
        let i = COLORS.iter().position(|(w, _)| c.word(w.as_ref()))?;
        return Some(Keyword::Protection(ObjectFilter {
            color: Some(COLORS[i].1.into()),
            ..ObjectFilter::default()
        }));
    }
    if c.word("partner") {
        if c.lit(" with ") {
            let name = c.s.to_owned();
            c.s = "";
            return Some(Keyword::PartnerWith(name));
        }
        return Some(Keyword::Partner);
    }
    None
}

fn keyword_line(line: &str) -> Option<Vec<Ability>> {
    let mut c = Cur::new(line.trim_end_matches('.'));
    let mut out = vec![Ability::Keyword(keyword(&mut c)?)];
    while c.lit(", ") {
        out.push(Ability::Keyword(keyword(&mut c)?));
    }
    c.done().then_some(out)
}

fn activated(line: &str) -> Option<Ability> {
    let (cost, rest) = line.split_once(": ")?;
    let cost = cost_list(cost)?;
    let mut text = rest.trim_end_matches('.').to_owned();
    let mut timing = Timing::Any;
    let mut once = false;
    for (suffix, t, o) in [
        (". activate only as a sorcery", Timing::Sorcery, false),
        (". activate only once each turn", Timing::Any, true),
        (". activate only as an instant", Timing::Instant, false),
    ] {
        if let Some(stripped) = text.strip_suffix(suffix) {
            text = stripped.to_owned();
            timing = t;
            once |= o;
        }
    }
    let mut ctx = Ctx::default();
    let effect = effect_text(&text, &mut ctx)?;
    Some(Ability::Activated(ActivatedAbility {
        cost,
        targets: ctx.targets,
        effect,
        timing,
        once_per_turn: once,
    }))
}

fn trigger_phrase(s: &str) -> Option<Trigger> {
    let mut c = Cur::new(s);
    let rel = |c: &mut Cur| {
        if c.lit("you ") {
            Some(PlayerRel::You)
        } else if c.lit("an opponent ") {
            Some(PlayerRel::Opponent)
        } else if c.lit("a player ") {
            Some(PlayerRel::Any)
        } else {
            None
        }
    };
    let this = ObjectFilter {
        this: true,
        ..ObjectFilter::default()
    };
    let t = if c.lit("~ enters") {
        Trigger::Enters(this)
    } else if c.lit("~ dies") {
        Trigger::Dies(this)
    } else if c.lit("~ attacks") {
        Trigger::Attacks(this)
    } else if c.lit("~ deals combat damage to a player") {
        Trigger::DealsCombatDamageToPlayer(this)
    } else if c.lit("~ leaves the battlefield") {
        Trigger::LeavesBattlefield(this)
    } else if c.lit("the beginning of ") {
        let whose = if c.lit("your ") {
            PlayerRel::You
        } else if c.lit("each ") {
            PlayerRel::Any
        } else {
            return None;
        };
        let step = if c.lit("upkeep") {
            StepName::Upkeep
        } else if c.lit("end step") {
            StepName::End
        } else if c.lit("draw step") {
            StepName::Draw
        } else if c.lit("precombat main phase") {
            StepName::PrecombatMain
        } else if c.lit("postcombat main phase") {
            StepName::PostcombatMain
        } else {
            return None;
        };
        Trigger::BeginningOf { step, whose }
    } else if let Some(by) = rel(&mut c) {
        if c.lit("casts ") || c.lit("cast ") {
            c.number()?;
            c.lit(" ");
            let spell = if c.lit("spell") {
                ObjectFilter::default()
            } else {
                let (f, _) = filter(&mut c)?;
                if !c.lit(" spell") {
                    return None;
                }
                f
            };
            Trigger::Cast {
                by,
                spell,
                first_each_turn: false,
            }
        } else if c.lit("draws a card") || c.lit("draw a card") {
            Trigger::Draws {
                by,
                except_first_in_draw_step: false,
            }
        } else if c.lit("discards a card") || c.lit("discard a card") {
            Trigger::Discards { by }
        } else {
            return None;
        }
    } else {
        // "a/an/another <filter> enters [under your control]" / "… dies"
        c.number().or_else(|| c.lit("another ").then_some(1))?;
        c.lit(" ");
        let (mut f, _) = filter(&mut c)?;
        if c.lit(" enters") {
            if c.lit(" under your control") {
                f.controller = PlayerRel::You;
            }
            Trigger::Enters(f)
        } else if c.lit(" dies") {
            Trigger::Dies(f)
        } else {
            return None;
        }
    };
    c.done().then_some(t)
}

fn triggered(line: &str) -> Option<Ability> {
    let mut c = Cur::new(line);
    if !(c.lit("when ") || c.lit("whenever ") || c.lit("at ")) {
        return None;
    }
    // The trigger ends at the first comma that leaves a parseable trigger.
    let body = c.s;
    let mut start = 0;
    while let Some(off) = body[start..].find(", ") {
        let split = start + off;
        if let Some(trigger) = trigger_phrase(&body[..split]) {
            let mut ctx = Ctx::default();
            let effect = effect_text(&body[split + 2..], &mut ctx)?;
            return Some(Ability::Triggered(TriggeredAbility {
                trigger,
                condition: None,
                targets: ctx.targets,
                effect,
            }));
        }
        start = split + 2;
    }
    None
}

fn static_line(line: &str) -> Option<Ability> {
    let l = line.trim_end_matches('.');
    let this = ObjectFilter {
        this: true,
        ..ObjectFilter::default()
    };
    if l == "~ enters tapped" {
        return Some(Ability::Replacement(ReplacementAbility {
            event: ReplaceEvent::WouldEnter(this),
            instead: Effect::EntersTapped,
            condition: None,
        }));
    }
    if l == "~ doesn't untap during your untap step" {
        return Some(Ability::Static(StaticAbility {
            effect: StaticEffect::DoesntUntap(this),
            condition: None,
        }));
    }
    if l == "skip your draw step" {
        return Some(Ability::Static(StaticAbility {
            effect: StaticEffect::SkipStep {
                who: PlayerRel::You,
                step: StepName::Draw,
            },
            condition: None,
        }));
    }
    let mut c = Cur::new(l);
    let (affected, true) = filter(&mut c)? else {
        return None;
    };
    if c.lit(" get ") {
        let (power, toughness) = pt_mod(&mut c)?;
        return c.done().then_some(Ability::Static(StaticAbility {
            effect: StaticEffect::Pump {
                affected,
                power,
                toughness,
            },
            condition: None,
        }));
    }
    if c.lit(" have ") {
        let k = keyword(&mut c)?;
        return c.done().then_some(Ability::Static(StaticAbility {
            effect: StaticEffect::Grant {
                affected,
                keyword: k,
            },
            condition: None,
        }));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str, name: &str, spell: bool) -> ParsedFace {
        let p = parse_face(text, name, spell);
        assert!(p.complete(), "{text:?} → {:?}", p.lines);
        p
    }

    #[test]
    fn spells() {
        let p = one(
            "Lightning Bolt deals 3 damage to any target.",
            "Lightning Bolt",
            true,
        );
        assert!(matches!(&p.abilities[0], Ability::Spell(s) if s.targets.len() == 1));
        one(
            "Exile target creature. Its controller gains life equal to its power.",
            "Swords to Plowshares",
            true,
        );
        one(
            "Counter target noncreature spell unless its controller pays {2}.",
            "Spell Pierce",
            true,
        );
        one(
            "Destroy target artifact or enchantment. Its controller gains 4 life.",
            "Nature's Claim",
            true,
        );
        one(
            "Search your library for a card, put it into your hand, then shuffle.",
            "Generic Tutor",
            true,
        );
        one(
            "You draw two cards and lose 2 life.",
            "Night's Whisper",
            true,
        );
        one("Scry 2, then draw a card.", "Preordain", true);
        one("Add {B}{B}{B}.", "Dark Ritual", true);
    }

    #[test]
    fn permanents() {
        one("{T}: Add {C}{C}.", "Sol Ring", false);
        one(
            "{T}: Add one mana of any color in your commander's color identity.",
            "Arcane Signet",
            false,
        );
        one(
            "{T}, Sacrifice this artifact: Add one mana of any color.",
            "Lotus Petal",
            false,
        );
        one(
            "Flying\n{T}: Add one mana of any color.",
            "Birds of Paradise",
            false,
        );
        one(
            "When this creature enters, draw a card.",
            "Elvish Visionary",
            false,
        );
        one(
            "This land enters tapped.\n{T}: Add {U} or {B}.",
            "Dimir Guildgate",
            false,
        );
        one("Creatures you control get +1/+1.", "Glorious Anthem", false);
        one(
            "Whenever an opponent casts a noncreature spell, you may draw a card.",
            "X",
            false,
        );
    }

    #[test]
    fn unsupported_lines_are_reported() {
        let p = parse_face(
            "Whenever you cast your second spell each turn, draw a card.",
            "X",
            false,
        );
        assert!(!p.complete());
    }
}
