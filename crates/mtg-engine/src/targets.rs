//! Object filters and target legality (CR 115,
//! `docs/03 Architecture/Subsystems/Targeting & Legality.md`).

use mtg_cards::ir::{ObjectFilter, PlayerRel, TargetKind};
use mtg_core::{CardType, Color, ObjectId, PlayerId, Supertype};

use crate::card::Keywords;
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
        let def = self.def(o.def);
        let t = &def.type_line;
        if (f.this && this != Some(id)) || (f.other && this == Some(id)) {
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
                .any(|c| def.colors.contains(c))
        {
            return false;
        }
        if !self.player_rel(f.controller, you, o.controller) {
            return false;
        }
        if let Some(z) = f.zone
            && !zone_is(z, o.zone)
        {
            return false;
        }
        true
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
    ) -> Vec<Target> {
        let mut out = Vec::new();
        let players = |rel: PlayerRel, out: &mut Vec<Target>| {
            out.extend(
                self.players
                    .iter()
                    .filter(|p| p.in_game() && self.player_rel(rel, you, p.id))
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
                            self.is_any_target_permanent(*id) && self.targetable_by(*id, you)
                        })
                        .map(Target::Object),
                );
            }
            TargetKind::Player(rel) => players(*rel, &mut out),
            TargetKind::Object(f) => out.extend(
                self.battlefield
                    .iter()
                    .copied()
                    .filter(|id| {
                        self.matches_filter(*id, f, you, None) && self.targetable_by(*id, you)
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

    /// CR 702.18a (shroud), 702.11b (hexproof): whether a spell or ability
    /// controlled by `you` may target permanent `id`.
    pub(crate) fn targetable_by(&self, id: ObjectId, you: PlayerId) -> bool {
        let kw = self.keywords(id);
        if kw.has(Keywords::SHROUD) {
            return false;
        }
        !(kw.has(Keywords::HEXPROOF) && self.obj(id).is_some_and(|o| o.controller != you))
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
    pub(crate) fn target_still_legal(&self, kind: &TargetKind, t: Target, you: PlayerId) -> bool {
        match (kind, t) {
            (TargetKind::Any, Target::Player(p)) => self.in_game(p),
            (TargetKind::Player(rel), Target::Player(p)) => {
                self.in_game(p) && self.player_rel(*rel, you, p)
            }
            (TargetKind::Any, Target::Object(id)) => {
                self.on_battlefield(id)
                    && self.is_any_target_permanent(id)
                    && self.targetable_by(id, you)
            }
            (TargetKind::Object(f), Target::Object(id)) => {
                self.on_battlefield(id)
                    && self.matches_filter(id, f, you, None)
                    && self.targetable_by(id, you)
            }
            (TargetKind::Spell(f), Target::Object(id)) => self.is_spell_matching(id, f, you),
            _ => false,
        }
    }

    fn on_battlefield(&self, id: ObjectId) -> bool {
        self.obj(id).is_some_and(|o| o.zone == ZoneId::Battlefield)
    }

    /// CR 115.4: permanents "any target" can choose.
    fn is_any_target_permanent(&self, id: ObjectId) -> bool {
        self.obj_def(id).is_some_and(|d| {
            d.is(CardType::Creature) || d.is(CardType::Planeswalker) || d.is(CardType::Battle)
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
