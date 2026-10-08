//! Timing benchmarks for the settings window: what one live controller update costs, and
//! what building the view costs. Run with `scripts/bench.sh gui`.

use std::hint::black_box;

use super::*;
use crate::bench::{bench, busy_config, snapshot_at};

/// The app on a game's Profiles tab, with a live controller reading shown.
fn app_on_game() -> App {
    let mut app = App::boot().0;
    app.config = busy_config(40);
    let _ = app.update(Message::SelectPage(Page::Game(Some("Game 39".into()))));
    let _ = app.update(Message::LiveInput(Some(snapshot_at(0))));
    app
}

#[test]
#[ignore = "benchmark; run with scripts/bench.sh gui"]
fn bench_gui() {
    let mut app = app_on_game();
    bench("gui: update(LiveInput)", 5_000, |i| {
        let _ = app.update(Message::LiveInput(Some(snapshot_at(i))));
    });

    let app = app_on_game();
    bench("gui: view(), game page", 2_000, |_| {
        black_box(app.view());
    });

    let mut app = app_on_game();
    bench("gui: update(LiveInput) + view()", 2_000, |i| {
        let _ = app.update(Message::LiveInput(Some(snapshot_at(i))));
        black_box(app.view());
    });

    let mut app = App::boot().0;
    app.config = busy_config(40);
    bench("gui: view(), overview page", 2_000, |_| {
        black_box(app.view());
    });
}
