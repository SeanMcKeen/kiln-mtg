//! Object filters and target legality (CR 115,
//! `docs/03 Architecture/Subsystems/Targeting & Legality.md`).

use mtg_cards::ir::{ObjectFilter, PlayerRel, TargetKind};
use mtg_core::{CardType, Color, ObjectId, PlayerId, Supertype};

use crate::card::{CardDef, Keywords};
use crate::engine::Game;
use crate::state::{Target, ZoneId};

impl Game {
    /// Whether player `p` stands in relation `rel` to `you`.
    pub(crate) fn player_rel(&self, rel: PlayerRel, you: PlayerId, p: PlayerId) -> bool {
        match rel {
            PlayerRel::Any => true,
            PlayerRel::You => p == you,
            PlayerRel::Opponent => p != you,
        }
    }

    /// Whether object `id` matches `f`, evaluated for an ability controlled by
    /// `you` whose source is `this`. The filter's zone, if any, must match too.
    pub(crate) fn matches_filter(
        &self,
        id: ObjectId,
        f: &ObjectFilter,
        you: PlayerId,
        this: Option<ObjectId>,
    ) -> bool {
        let Some(o) = self.obj(id) else { return false };
        // CR 613: the object's current characteristics.
        let Some(c) = self.chars(id) else {
            return false;
        };
        let t = c.type_line();
        if (f.this && this != Some(id)) || (f.other && this == Some(id)) {
            return false;
        }
        if f.attached && !self.is_attached_to(this, id) {
            return false;
        }
        if let Some(letters) = &f.not_color
            && letters
                .chars()
                .filter_map(Color::from_letter)
                .any(|col| c.colors().contains(col))
        {
            return false;
        }
        // CR 201.2: "named [this card]".
        if f.same_name
            && this.is_none_or(|s| {
                self.object_name(s).is_none() || self.object_name(s) != self.object_name(id)
            })
        {
            return false;
        }
        let has_type = |w: &String| {
            CardType::ALL
                .iter()
                .any(|ct| ct.word().eq_ignore_ascii_case(w) && t.has_type(*ct))
        };
        if !f.types.is_empty() && !f.types.iter().any(has_type) {
            return false;
        }
        if f.not_types.iter().any(has_type) {
            return false;
        }
        let has_sub = |w: &String| t.subtypes.iter().any(|s| s.0.eq_ignore_ascii_case(w));
        if !f.subtypes.is_empty() && !f.subtypes.iter().any(has_sub) {
            return false;
        }
        if f.not_subtypes.iter().any(has_sub) {
            return false;
        }
        let has_super = |w: &String| {
            Supertype::ALL
                .iter()
                .any(|st| st.word().eq_ignore_ascii_case(w) && t.has_supertype(*st))
        };
        if !f.supertypes.is_empty() && !f.supertypes.iter().any(has_super) {
            return false;
        }
        if f.not_supertypes.iter().any(has_super) {
            return false;
        }
        if let Some(letters) = &f.color
            && !letters
                .chars()
                .filter_map(Color::from_letter)
                .any(|col| c.colors().contains(col))
        {
            return false;
        }
        if !self.player_rel(f.controller, you, c.controller()) {
            return false;
        }
        if let Some(z) = f.zone
            && !zone_is(z, o.zone)
        {
            return false;
        }
        if f.min_mana_value.is_some() || f.max_mana_value.is_some() {
            let mv = self.mana_value_of(id);
            if f.min_mana_value.is_some_and(|m| mv < m) || f.max_mana_value.is_some_and(|m| mv > m)
            {
                return false;
            }
        }
        true
    }

    /// CR 202.3: an object's mana value; on the stack, X is its announced value (CR 202.3e).
    pub(crate) fn mana_value_of(&self, id: ObjectId) -> u32 {
        let Some(o) = self.obj(id) else { return 0 };
        // A copy has the copied mana cost (CR 707.2).
        let def = self.def(self.chars(id).map_or(o.def, |c| c.def_id()));
        let mut mv = def.mana_value();
        if o.zone == ZoneId::Stack
            && let Some(item) = self.stack.iter().find(|s| s.is_spell() && s.object == id)
        {
            let xs = def.mana_cost.as_ref().map_or(0, |c| {
                c.symbols()
                    .iter()
                    .filter(|s| matches!(s, mtg_core::ManaSymbol::Variable(_)))
                    .count()
            });
            mv += item.x * u32::try_from(xs).unwrap_or(0);
        }
        mv
    }

    /// Legal choices for one target slot (CR 115.1, 601.2c), for a spell or
    /// ability controlled by `you` whose stack object is `this` (a spell is an
    /// illegal target for itself, CR 115.5). Players first in seat order, then
    /// permanents in battlefield order, then spells from the top of the stack.
    pub(crate) fn legal_targets(
        &self,
        kind: &TargetKind,
        you: PlayerId,
        this: Option<ObjectId>,
        src: Option<&CardDef>,
    ) -> Vec<Target> {
        let mut out = Vec::new();
        let players = |rel: PlayerRel, out: &mut Vec<Target>| {
            out.extend(
                self.players
                    .iter()
                    .filter(|p| {
                        p.in_game()
                            && self.player_rel(rel, you, p.id)
                            && !self.player_protected(p.id)
                    })
                    .map(|p| Target::Player(p.id)),
            );
        };
        match kind {
            TargetKind::Any => {
                // CR 115.4: a creature, player, planeswalker or battle.
                players(PlayerRel::Any, &mut out);
                out.extend(
                    self.battlefield
                        .iter()
                        .copied()
                        .filter(|id| {
                            self.is_any_target_permanent(*id) && self.targetable_by(*id, you, src)
                        })
                        .map(Target::Object),
                );
            }
            TargetKind::Player(rel) => players(*rel, &mut out),
            // Cards in graveyards (an Aura's "enchant creature card in a graveyard").
            TargetKind::Object(f) if f.zone == Some(mtg_cards::ir::ZoneName::Graveyard) => out
                .extend(
                    self.players
                        .iter()
                        .flat_map(|p| p.graveyard.iter().copied())
                        .filter(|id| self.matches_filter(*id, f, you, None))
                        .map(Target::Object),
                ),
            TargetKind::Object(f) => out.extend(
                self.battlefield
                    .iter()
                    .copied()
                    .filter(|id| {
                        self.matches_filter(*id, f, you, None) && self.targetable_by(*id, you, src)
                    })
                    .map(Target::Object),
            ),
            TargetKind::Spell(f) => out.extend(
                self.stack
                    .iter()
                    .rev()
                    .map(|s| s.object)
                    .filter(|id| Some(*id) != this && self.is_spell_matching(*id, f, you))
                    .map(Target::Object),
            ),
        }
        out
    }

    /// CR 702.18a (shroud), 702.11b (hexproof), 702.16b (protection): whether
    /// a spell or ability controlled by `you`, from source `src`, may target
    /// permanent `id`.
    pub(crate) fn targetable_by(&self, id: ObjectId, you: PlayerId, src: Option<&CardDef>) -> bool {
        let kw = self.keywords(id);
        if kw.has(Keywords::SHROUD) {
            return false;
        }
        if src.is_some_and(|s| self.protected_from(id, s, you)) {
            return false;
        }
        !(kw.has(Keywords::HEXPROOF) && self.obj(id).is_some_and(|o| o.controller != you))
    }

    /// CR 702.16a: whether permanent `id` has protection from a source with
    /// definition `src` controlled by `src_controller`.
    pub(crate) fn protected_from(
        &self,
        id: ObjectId,
        src: &CardDef,
        src_controller: PlayerId,
    ) -> bool {
        let Some(o) = self.obj(id) else {
            return false;
        };
        self.abilities_of(id).iter().any(|(_, a)| {
            matches!(a, mtg_cards::ir::Ability::Keyword(mtg_cards::ir::Keyword::Protection(f))
                if crate::triggers::card_matches(self, src, src_controller, f, o.controller))
        })
    }

    fn is_spell_matching(&self, id: ObjectId, f: &ObjectFilter, you: PlayerId) -> bool {
        let mut f = f.clone();
        f.zone = None;
        self.obj(id).is_some_and(|o| o.zone == ZoneId::Stack)
            && self.matches_filter(id, &f, you, None)
    }

    /// CR 608.2b: whether a chosen target is still legal as the spell or
    /// ability resolves. An object target must still be the same object in the
    /// same zone (a zone change makes a new object, CR 400.7).
    pub(crate) fn target_still_legal(
        &self,
        kind: &TargetKind,
        t: Target,
        you: PlayerId,
        src: Option<&CardDef>,
    ) -> bool {
        match (kind, t) {
            (TargetKind::Any, Target::Player(p)) => self.in_game(p) && !self.player_protected(p),
            (TargetKind::Player(rel), Target::Player(p)) => {
                self.in_game(p) && self.player_rel(*rel, you, p) && !self.player_protected(p)
            }
            (TargetKind::Any, Target::Object(id)) => {
                self.on_battlefield(id)
                    && self.is_any_target_permanent(id)
                    && self.targetable_by(id, you, src)
            }
            (TargetKind::Object(f), Target::Object(id))
                if f.zone == Some(mtg_cards::ir::ZoneName::Graveyard) =>
            {
                self.obj(id)
                    .is_some_and(|o| matches!(o.zone, ZoneId::Graveyard(_)))
                    && self.matches_filter(id, f, you, None)
            }
            (TargetKind::Object(f), Target::Object(id)) => {
                self.on_battlefield(id)
                    && self.matches_filter(id, f, you, None)
                    && self.targetable_by(id, you, src)
            }
            (TargetKind::Spell(f), Target::Object(id)) => self.is_spell_matching(id, f, you),
            _ => false,
        }
    }

    pub(crate) fn on_battlefield(&self, id: ObjectId) -> bool {
        // A phased-out permanent is treated as though it doesn't exist (CR 702.26b).
        self.obj(id).is_some_and(|o| o.zone == ZoneId::Battlefield)
            && (self.phased_out.is_empty() || self.battlefield.contains(&id))
    }

    /// CR 702.16b, 702.16j: a player with protection from everything can't
    /// be targeted.
    fn player_protected(&self, p: PlayerId) -> bool {
        self.has_player_rule(p, mtg_cards::ir::PlayerRuleKind::ProtectionFromEverything)
    }

    /// CR 115.4: permanents "any target" can choose.
    fn is_any_target_permanent(&self, id: ObjectId) -> bool {
        self.chars(id).is_some_and(|c| {
            c.is(CardType::Creature) || c.is(CardType::Planeswalker) || c.is(CardType::Battle)
        })
    }
}

fn zone_is(z: mtg_cards::ir::ZoneName, actual: ZoneId) -> bool {
    use mtg_cards::ir::ZoneName as Z;
    matches!(
        (z, actual),
        (Z::Hand, ZoneId::Hand(_))
            | (Z::Library, ZoneId::Library(_))
            | (Z::Graveyard, ZoneId::Graveyard(_))
            | (Z::Battlefield, ZoneId::Battlefield)
            | (Z::Exile, ZoneId::Exile)
            | (Z::Stack, ZoneId::Stack)
            | (Z::Command, ZoneId::Command)
    )
}
