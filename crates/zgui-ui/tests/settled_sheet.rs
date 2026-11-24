//! A bottom sheet that a drag moves and releases, and the window it is in hidden and shown again.
//!
//! The sheet's surface holds its entrance as a forwards fill, and the drag offset goes back to the
//! detent on a transition. Hiding and showing the window runs a frame after that transition has
//! ended.

mod desktop;

use core::time::Duration;

use zgui::geom::Css;
use zgui::geom::CssPx;
use zgui::geom::Point;
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::view;
use zgui_ui::prelude::*;

use crate::desktop::stage::Stage;

#[test]
fn a_settled_sheet_stays_at_its_detent_after_the_window_is_hidden() {
    let mut stage = Stage::open(".frame { width: 390px; height: 844px }", || {
        let open = RwSignal::new_local(true);
        view! {
            column(class = "frame") {
                BottomSheet(open = open, label = "New Password") {
                    control(on:click = move |_| open.set(false)) {"Cancel"}
                    text {"Body"}
                }
            }
        }
    });
    let cancel = |stage: &Stage| {
        stage
            .census()
            .control("Cancel")
            .and_then(|node| node.centre())
            .expect("the sheet shows Cancel")
    };
    stage.deliver_resize(390.0, 844.0);
    stage.hold(Duration::from_secs(1));
    let before = cancel(&stage);

    stage.pan(
        Point::<CssPx, Css>::new(CssPx(200.0), CssPx(95.0)),
        Point::new(CssPx(200.0), CssPx(250.0)),
        1.0,
    );
    stage.hold(Duration::from_secs(2));
    assert_eq!(
        before,
        cancel(&stage),
        "the drag settles back to the detent"
    );

    stage.hide_and_show();
    stage.hold(Duration::from_secs(1));
    assert_eq!(before, cancel(&stage), "the sheet stays at its detent");
}
