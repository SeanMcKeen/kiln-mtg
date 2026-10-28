// Playbook for Thrasios + Tymna (thrasios-tymna.txt). M8 example: a shared core (mulligans, land drops,
// commanders, cheap mana, wins) plus this deck's own lines. Everything else
// defers to the fallback policy.

// ---- shared core (each playbook is self-contained) -----------------------

// Keep 2-4 lands, or 1 land with three cheap spells.
fn keep_hand(view) {
    let lands = view.hand.filter(|c| c.land).len();
    let cheap = view.hand.filter(|c| !c.land && c.mana_value <= 2).len();
    (lands >= 2 && lands <= 4) || (lands >= 1 && cheap >= 3)
}

fn mulligan(view, d) {
    if keep_hand(view) {
        tag("keep");
        return "Keep";
    }
    tag("mulligan");
    "Mulligan"
}

// The opponent with the least life (-1 if none is left).
fn weakest_opponent(view) {
    let best = -1;
    let life = 1000000;
    for p in view.players {
        if p.seat != view.me && p.in_game && p.life < life {
            life = p.life;
            best = p.seat;
        }
    }
    best
}

// The option that attacks or targets that player, or -1.
fn option_for_player(d, prefix, seat) {
    let want = prefix + "P" + (seat + 1);
    for i in 0..d.count {
        if d.options[i].ends_with(want) {
            return i;
        }
    }
    -1
}

// Land drops, commanders, cheap mana, and wins. -1 when nothing applies.
fn core_priority(view, d) {
    let land = find_option(d.options, "Play ");
    if land >= 0 {
        tag("land drop");
        return land;
    }
    if view.my_turn && view.stack.len() == 0 {
        for c in view.command {
            if c.owner == view.me {
                let i = find_option(d.options, "Cast " + c.name);
                if i >= 0 {
                    tag("cast commander");
                    return i;
                }
            }
        }
        for c in view.hand {
            if !c.land && c.types.contains("artifact") && c.mana_value <= 2 {
                let i = find_option(d.options, "Cast " + c.name);
                if i >= 0 {
                    tag("fast mana");
                    return i;
                }
            }
        }
    }
    let flux = find_option(d.options, "Activate Aetherflux Reservoir ability 2");
    if flux >= 0 {
        tag("aetherflux kill");
        return flux;
    }
    let library = view.players[view.me].library_count;
    let oracle = find_option(d.options, "Cast Thassa's Oracle");
    if oracle >= 0 && library <= 3 {
        tag("win attempt");
        return oracle;
    }
    -1
}

// Our own Aetherflux Reservoir aims at the weakest opponent.
fn choose_target(view, d) {
    let top = view.stack;
    if top.len() > 0 {
        let s = top[top.len() - 1];
        if s.controller == view.me && s.name == "Aetherflux Reservoir" {
            let i = option_for_player(d, "Target ", weakest_opponent(view));
            if i >= 0 {
                tag("aim at weakest");
                return i;
            }
        }
    }
    ()
}

fn priority(view, d) {
    let i = core_priority(view, d);
    if i >= 0 {
        return i;
    }
    // Spare mana on opponents' turns goes into Thrasios.
    if !view.my_turn && view.stack.len() == 0 {
        let t = find_option(d.options, "Activate Thrasios, Triton Hero");
        if t >= 0 {
            tag("thrasios draw");
            return t;
        }
    }
    ()
}

// Attack the weakest opponent with everything.
fn declare_attacker(view, d) {
    let target = weakest_opponent(view);
    if target < 0 {
        return ();
    }
    let i = option_for_player(d, "attacks ", target);
    if i >= 0 {
        tag("attack weakest");
        return i;
    }
    ()
}
