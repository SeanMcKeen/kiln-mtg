//! Determinization (`docs/14 Players & Policies/Policy Interface.md`, M9 plan
//! S1): a full game consistent with what one player knows, for search.
//!
//! Sampling happens here, on the engine side, so a policy never reads hidden
//! information: it gets a plausible fabrication. What a viewer can't tell
//! apart is redealt:
//! - each other player's hand and library are pooled and dealt again, keeping
//!   their sizes (CR 402.3: hands are hidden; CR 401.2: libraries too);
//! - the viewer's own library is shuffled, except a top card it may look at
//!   (Bolas's Citadel), since it knows the contents but not the order;
//! - the game's random stream is reseeded, so future shuffles are unknown.
//!
//! Cards are redealt by permuting the card definitions of the hidden objects:
//! object ids, zones and everything that refers to them stay intact.
//! Opponents' hidden cards come from their real decks (open decklists).

use kiln_types::PlayerId;

use crate::engine::Game;
use crate::rng::{GameRng, splitmix64};
use crate::view::PlayerView;

impl PlayerView<'_> {
    /// A copy of the game consistent with this view, sampled with `seed`.
    /// The copy logs no events.
    pub fn determinize(&self, seed: u64) -> Game {
        self.game.determinized(self.me(), seed)
    }
}

impl Game {
    pub(crate) fn determinized(&self, me: PlayerId, seed: u64) -> Game {
        let mut g = self.clone();
        let mut rng = GameRng::new(splitmix64(seed ^ 0xDE7E_A11A));
        g.events.clear();
        g.rules.log_events = false;
        for p in 0..g.players.len() {
            let pid = PlayerId(u8::try_from(p).unwrap_or(u8::MAX));
            let visible_top = g.top_card_visible(pid);
            let player = &g.players[p];
            let hidden: Vec<_> = if g.sees_as(me, pid) {
                player
                    .library
                    .iter()
                    .copied()
                    .filter(|id| Some(*id) != visible_top)
                    .collect()
            } else {
                player.hand.iter().chain(&player.library).copied().collect()
            };
            let mut defs: Vec<_> = hidden
                .iter()
                .filter_map(|id| g.objects.get(*id).map(|o| o.def))
                .collect();
            rng.shuffle(&mut defs);
            for (id, def) in hidden.iter().zip(defs) {
                if let Some(o) = g.objects.get_mut(*id) {
                    o.def = def;
                }
            }
        }
        g.rng = GameRng::new(rng.next_u64());
        // Characteristics are cached per epoch; the hidden cards changed.
        g.bump_layers();
        g
    }
}
