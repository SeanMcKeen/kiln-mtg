// A generic cEDH playbook (M8 example). It answers mulligans, land drops and
// win attempts; every other decision is deferred to the fallback policy.
//
// Each function gets (view, decision) and returns an option index, an option
// label, or () to defer. `view.hand`, `view.battlefield`, `view.stack` hold
// objects with name, types, mana_value, land, creature, controller, tapped;
// `view.players[i]` has life, hand_count, library_count; `view.me` is my seat.

fn mulligan(view, d) {
    let lands = view.hand.filter(|c| c.land).len();
    let cheap = view.hand.filter(|c| !c.land && c.mana_value <= 2).len();
    if (lands >= 2 && lands <= 4) || (lands >= 1 && cheap >= 3) {
        tag("keep");
        return "Keep";
    }
    tag("mulligan");
    "Mulligan"
}

fn priority(view, d) {
    // A land drop first.
    let land = find_option(d.options, "Play ");
    if land >= 0 {
        tag("land drop");
        return land;
    }
    // Win with Thassa's Oracle or Laboratory Maniac once the library is small.
    let library = view.players[view.me].library_count;
    for win in ["Thassa's Oracle", "Laboratory Maniac"] {
        let i = find_option(d.options, "Cast " + win);
        if i >= 0 && library <= 3 {
            tag("win attempt");
            return i;
        }
    }
    ()
}
