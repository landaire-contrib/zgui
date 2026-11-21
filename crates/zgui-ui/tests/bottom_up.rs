//! A bottom-up list in a window with a real layout.
//!
//! Each fixture reads the boxes the window laid out: where the rows are on screen, how far the port
//! is scrolled, and how tall the content is.

mod desktop;

use core::ops::Range;
use std::cell::Cell;
use std::rc::Rc;

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view;
use zgui::view::{AnyView, ScrollBehavior, ScrollTarget};
use zgui_ui::prelude::*;

use crate::desktop::stage::Stage;

/// A port 400px tall, a loading mark 40px tall and a foot 25px tall.
const SHEET: &str = ".list { width: 300px; height: 400px }
                     .loading { height: 40px; flex: none }
                     .foot { height: 25px; flex: none }";

/// How near an end of the rows, in CSS pixels, the host is told of.
const DISTANCE: f32 = 300.0;

/// One row: what names it, and how tall it is in CSS pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Line {
    key: u64,
    height: f32,
}

/// Rows with keys `keys`, each `height(key)` tall.
fn lines(keys: Range<u64>, height: impl Fn(u64) -> f32) -> Vec<Line> {
    keys.map(|key| Line {
        key,
        height: height(key),
    })
    .collect()
}

/// Every row 30px tall.
fn even(_: u64) -> f32 {
    30.0
}

/// Rows from 30px to 60px tall. One in four is at the 30px estimate.
fn mixed(key: u64) -> f32 {
    30.0 + (key % 4) as f32 * 10.0
}

/// What the view hands out once it is built.
type Handed = Rc<Cell<Option<Parts>>>;

/// The handles a fixture drives the list with.
#[derive(Clone, Copy)]
struct Parts {
    list: BottomUp<u64>,
    lines: RwSignal<Vec<Line>, LocalStorage>,
    loading: RwSignal<bool, LocalStorage>,
    newer: RwSignal<bool, LocalStorage>,
}

/// An open list, and how many times the host was told of each end.
struct Chat {
    stage: Stage,
    parts: Parts,
    near_start: Rc<Cell<usize>>,
    near_end: Rc<Cell<usize>>,
}

impl Chat {
    /// A list of `rows`, started at the row with key `start` when it is given, opened and settled.
    fn open(rows: Vec<Line>, start: Option<u64>) -> Self {
        let handed: Handed = Rc::default();
        let out = Rc::clone(&handed);
        let near_start = Rc::new(Cell::new(0));
        let near_end = Rc::new(Cell::new(0));
        let (told_start, told_end) = (Rc::clone(&near_start), Rc::clone(&near_end));
        let mut stage = Stage::open(SHEET, move || {
            let lines = RwSignal::new_local(rows.clone());
            let loading = RwSignal::new_local(false);
            let newer = RwSignal::new_local(false);
            let count = Signal::derive_local(move || lines.with(Vec::len));
            let (told_start, told_end) = (Rc::clone(&told_start), Rc::clone(&told_end));
            let list = BottomUp::new(
                count,
                MeasuredRows::new(30.0),
                move |index| lines.with(|lines| lines[index].key),
                2,
            )
            .newer(newer)
            .edge_distance(DISTANCE)
            .on_near_start(move || told_start.set(told_start.get() + 1))
            .on_near_end(move || told_end.set(told_end.get() + 1));
            if let Some(key) = start {
                list.scroll_to_key(key);
            }
            out.set(Some(Parts {
                list,
                lines,
                loading,
                newer,
            }));

            view! {
                BottomUpList(
                    class = "list",
                    list = list,
                    label = "Messages",
                    top = move || AnyView::new(view! {
                        if move || loading.get() {
                            box(class = "loading") {"loading"}
                        } else {}
                    }),
                    bottom = || AnyView::new(view! { box(class = "foot") {"foot"} }),
                    row = move |index: usize| {
                        let line = move || lines.with(|lines| lines.get(index).copied());
                        view! {
                            box(style:height = move || line().map(|line| format!("{}px", line.height))) {
                                {move || line().map(|line| format!("row {}", line.key)).unwrap_or_default()}
                            }
                        }
                    }
                )
            }
        });
        stage.settle();
        let parts = handed.get().expect("the view was built");
        Self {
            stage,
            parts,
            near_start,
            near_end,
        }
    }

    /// The list.
    fn list(&self) -> BottomUp<u64> {
        self.parts.list
    }

    /// Changes the rows, and runs the frames that asks for.
    fn change(&mut self, change: impl FnOnce(&mut Vec<Line>)) {
        self.parts.lines.update(change);
        self.stage.settle();
    }

    /// Adds `count` rows of 30px at the end.
    fn append(&mut self, count: u64) {
        self.change(|lines| {
            let next = lines.last().map_or(0, |line| line.key + 1);
            lines.extend(self::lines(next..next + count, even));
        });
    }

    /// Makes the row with `key` `height` CSS pixels tall.
    fn resize(&mut self, key: u64, height: f32) {
        self.change(|lines| {
            if let Some(line) = lines.iter_mut().find(|line| line.key == key) {
                line.height = height;
            }
        });
    }

    /// The port, in window device pixels.
    fn port(&self) -> Rect<DevicePx, Device> {
        let node = self
            .list()
            .port()
            .get_untracked()
            .expect("the port is bound");
        self.stage
            .handles()
            .host
            .border_box(node)
            .expect("the port is laid out")
    }

    /// How far the port is scrolled, how far it can still go, and how tall its content is.
    fn scroll(&self) -> (f32, f32, f32) {
        let node = self
            .list()
            .port()
            .get_untracked()
            .expect("the port is bound");
        let position = self.stage.handles().host.scroll_position(node);
        (
            position.offset.y.0,
            position.remaining().height.0,
            position.content_size.height.0,
        )
    }

    /// Scrolls the port to `offset` at once, as a scrollbar does.
    fn scroll_to(&mut self, offset: f32) {
        self.list().port().scroll_to(
            ScrollTarget::Offset(Point::new(DevicePx(0.0), DevicePx(offset))),
            ScrollBehavior::Instant,
        );
        self.stage.settle();
    }

    /// Turns the wheel `lines` down over the middle of the port.
    fn wheel(&mut self, lines: f32) {
        let port = self.port();
        self.stage.move_to(Point::new(
            DevicePx(port.origin.x.0 + port.size.width.0 / 2.0),
            DevicePx(port.origin.y.0 + port.size.height.0 / 2.0),
        ));
        self.stage.wheel(lines);
    }

    /// The box of the outermost node that says `text`.
    fn rect_of(&self, text: &str) -> Option<Rect<DevicePx, Device>> {
        self.stage.census().outermost(text)?.rect
    }

    /// The box of the row with `key`, when it is built.
    fn row(&self, key: u64) -> Option<Rect<DevicePx, Device>> {
        self.rect_of(&format!("row {key}"))
    }

    /// Each row that reaches into the port, with the top of its box, in order.
    fn on_screen(&self) -> Vec<(u64, f32)> {
        let port = self.port();
        let (top, bottom) = (port.origin.y.0, port.origin.y.0 + port.size.height.0);
        let census = self.stage.census();
        self.parts.lines.with_untracked(|lines| {
            lines
                .iter()
                .filter_map(|line| {
                    let rect = census.outermost(&format!("row {}", line.key))?.rect?;
                    let (start, end) = (rect.origin.y.0, rect.origin.y.0 + rect.size.height.0);
                    (end > top && start < bottom).then_some((line.key, start))
                })
                .collect()
        })
    }
}

#[test]
fn a_list_starts_at_the_end_and_follows_rows_as_they_are_added() {
    let mut chat = Chat::open(lines(0..100, even), None);
    assert_eq!(chat.scroll().1, 0.0, "the list starts at the end");
    assert!(chat.list().at_end_untracked());

    chat.append(5);
    let port = chat.port();
    let last = chat.row(104).expect("the newest row is built");
    assert_eq!(chat.scroll().1, 0.0, "the list followed the new rows");
    assert_eq!(
        last.origin.y.0 + last.size.height.0 + 25.0,
        port.origin.y.0 + port.size.height.0,
        "the newest row is above the foot at the bottom of the port"
    );
}

#[test]
fn a_list_at_the_end_follows_a_row_that_grows() {
    let mut chat = Chat::open(lines(0..100, even), None);
    chat.resize(99, 200.0);
    assert_eq!(chat.scroll().1, 0.0, "the list followed the row as it grew");

    chat.resize(97, 150.0);
    assert_eq!(chat.scroll().1, 0.0, "and a row above it");
}

#[test]
fn the_reader_leaves_the_end_and_a_return_to_it_follows_again() {
    let mut chat = Chat::open(lines(0..100, even), None);
    chat.wheel(-3.0);
    let (offset, remaining, _) = chat.scroll();
    assert!(remaining > 0.0, "the wheel moved the port up");
    assert!(!chat.list().at_end_untracked());

    let before = chat.on_screen();
    chat.append(3);
    assert_eq!(chat.scroll().0, offset, "the list does not follow the end");
    assert_eq!(chat.on_screen(), before, "the rows on screen did not move");

    chat.wheel(20.0);
    assert_eq!(chat.scroll().1, 0.0, "the wheel went back to the end");
    assert!(chat.list().at_end_untracked());
    chat.append(3);
    assert_eq!(chat.scroll().1, 0.0, "the list follows the end again");
}

#[test]
fn a_return_to_the_end_on_request_follows_again() {
    let mut chat = Chat::open(lines(0..100, even), None);
    chat.scroll_to(500.0);
    assert!(!chat.list().at_end_untracked());

    chat.list().follow_end();
    chat.stage.settle();
    assert_eq!(chat.scroll().1, 0.0);
    assert!(chat.list().at_end_untracked());

    chat.append(2);
    assert_eq!(chat.scroll().1, 0.0, "the list follows the end");
}

#[test]
fn newer_rows_beyond_the_end_keep_the_end_from_being_followed() {
    let mut chat = Chat::open(lines(0..100, even), None);
    chat.parts.newer.set(true);
    chat.scroll_to(500.0);
    chat.scroll_to(f32::MAX);
    assert_eq!(
        chat.scroll().1,
        0.0,
        "the port is at the end of the rows held"
    );
    assert!(
        !chat.list().at_end_untracked(),
        "and they are not the newest"
    );

    let offset = chat.scroll().0;
    chat.append(3);
    assert_eq!(chat.scroll().0, offset, "the list does not follow the end");
}

#[test]
fn a_list_can_start_with_a_row_at_the_top() {
    let chat = Chat::open(lines(0..200, mixed), Some(60));
    let row = chat.row(60).expect("the row is built");
    assert_eq!(row.origin.y.0, chat.port().origin.y.0);
    assert!(!chat.list().at_end_untracked());
}

#[test]
fn a_held_row_stays_at_the_top_while_heights_settle_and_the_reader_lets_it_go() {
    let mut chat = Chat::open(lines(0..200, mixed), None);
    assert!(chat.list().scroll_to_row(100));
    chat.stage.settle();
    let top = chat.port().origin.y.0;
    assert_eq!(chat.row(100).expect("the row is built").origin.y.0, top);

    chat.resize(99, 120.0);
    chat.resize(101, 150.0);
    chat.append(4);
    assert_eq!(
        chat.row(100).expect("the row is built").origin.y.0,
        top,
        "the held row stays at the top as rows around it change"
    );

    chat.wheel(1.0);
    let moved = chat.on_screen();
    assert!(
        chat.row(100).expect("the row is built").origin.y.0 < top,
        "the wheel moved the held row up"
    );

    chat.resize(99, 30.0);
    chat.resize(120, 200.0);
    chat.append(4);
    assert_eq!(
        chat.on_screen(),
        moved,
        "the list does not go back to the row it held"
    );
}

#[test]
fn a_held_row_near_the_end_reaches_the_top_when_the_rows_below_it_grow() {
    let mut chat = Chat::open(lines(0..200, mixed), None);
    assert!(chat.list().scroll_to_row(197));
    chat.stage.settle();
    let top = chat.port().origin.y.0;
    assert!(
        chat.row(197).expect("the row is built").origin.y.0 > top,
        "the rows below the held row are too short to let it reach the top"
    );

    chat.resize(198, 300.0);
    chat.resize(199, 300.0);
    assert_eq!(chat.row(197).expect("the row is built").origin.y.0, top);
}

#[test]
fn a_held_key_the_list_does_not_have_is_waited_for() {
    let mut chat = Chat::open(lines(0..100, even), None);
    chat.list().scroll_to_key(500);
    chat.stage.settle();
    let offset = chat.scroll().0;
    chat.append(3);
    assert_eq!(
        chat.scroll().0,
        offset,
        "the list waits, and does not follow the end"
    );

    chat.change(|lines| *lines = self::lines(400..600, mixed));
    assert_eq!(
        chat.row(500).expect("the row is built").origin.y.0,
        chat.port().origin.y.0,
        "the page with the row came in"
    );
}

#[test]
fn a_page_inserted_above_leaves_every_row_on_screen_in_place() {
    let mut chat = Chat::open(lines(1_000..1_100, mixed), None);
    let middle: f32 = (1_000..1_050).map(mixed).sum();
    chat.scroll_to(middle + 7.0);
    let before = chat.on_screen();
    let top = chat.port().origin.y.0;
    assert!(
        before.first().is_some_and(|(_, start)| *start < top),
        "the first row on screen is cut by the top of the port"
    );

    chat.change(|lines| {
        lines.splice(
            0..0,
            self::lines(960..1_000, |key| 20.0 + (key % 7) as f32 * 9.0),
        );
    });
    assert_eq!(chat.on_screen(), before);

    // A mark appears above the rows while a page loads, and goes when the page comes in.
    chat.parts.loading.set(true);
    chat.stage.settle();
    assert_eq!(
        chat.on_screen(),
        before,
        "the mark above the port moved nothing"
    );
    chat.parts.lines.update(|lines| {
        lines
            .splice(0..0, self::lines(900..960, mixed))
            .for_each(drop);
    });
    chat.parts.loading.set(false);
    chat.stage.settle();
    assert_eq!(chat.on_screen(), before);
}

#[test]
fn the_host_is_told_once_for_each_approach_to_an_end() {
    let mut chat = Chat::open(lines(100..200, even), None);
    assert_eq!(chat.near_start.get(), 0);
    assert_eq!(chat.near_end.get(), 0, "the host has no newer rows");

    chat.scroll_to(200.0);
    assert_eq!(chat.near_start.get(), 1);
    chat.scroll_to(100.0);
    assert_eq!(chat.near_start.get(), 1, "the reader is still near");

    chat.scroll_to(1_000.0);
    chat.scroll_to(50.0);
    assert_eq!(
        chat.near_start.get(),
        2,
        "the reader went away and came back"
    );

    // A small page leaves the reader near the start, and a large one does not.
    chat.change(|lines| {
        lines
            .splice(0..0, self::lines(98..100, even))
            .for_each(drop)
    });
    assert_eq!(
        chat.near_start.get(),
        3,
        "a new first row is a new approach"
    );
    chat.change(|lines| lines.splice(0..0, self::lines(60..98, even)).for_each(drop));
    assert_eq!(
        chat.near_start.get(),
        3,
        "the page took the reader away from the start"
    );

    chat.parts.newer.set(true);
    chat.scroll_to(f32::MAX);
    assert_eq!(chat.near_end.get(), 1);
    chat.scroll_to(f32::MAX - 1.0);
    assert_eq!(chat.near_end.get(), 1, "the reader is still near");
    chat.scroll_to(0.0);
    chat.scroll_to(f32::MAX);
    assert_eq!(chat.near_end.get(), 2, "the reader went away and came back");
}

#[test]
fn a_short_list_that_grows_past_the_port_ends_at_the_end() {
    let mut chat = Chat::open(lines(0..3, even), None);
    chat.append(30);
    assert_eq!(chat.scroll().1, 0.0);
}

#[test]
fn a_short_list_that_older_rows_take_past_the_port_ends_at_the_end() {
    let mut chat = Chat::open(lines(100..103, even), None);
    chat.change(|lines| {
        lines
            .splice(0..0, self::lines(60..100, mixed))
            .for_each(drop)
    });
    assert_eq!(chat.scroll().1, 0.0);
}

#[test]
fn the_edge_slots_are_inside_the_extent() {
    let mut chat = Chat::open(lines(0..100, even), None);
    chat.parts.loading.set(true);
    chat.stage.settle();
    let port = chat.port();
    let (_, remaining, content) = chat.scroll();
    assert_eq!(content, 40.0 + 100.0 * 30.0 + 25.0);
    assert_eq!(remaining, 0.0);
    let foot = chat.rect_of("foot").expect("the foot is built");
    assert_eq!(
        foot.origin.y.0 + foot.size.height.0,
        port.origin.y.0 + port.size.height.0
    );

    chat.scroll_to(0.0);
    let mark = chat.rect_of("loading").expect("the mark is built");
    assert_eq!(mark.origin.y.0, port.origin.y.0);
    assert_eq!(
        chat.row(0).expect("the first row is built").origin.y.0,
        port.origin.y.0 + 40.0
    );
}

#[test]
fn rows_shorter_than_the_port_stand_at_its_bottom() {
    let chat = Chat::open(lines(0..3, even), None);
    let port = chat.port();
    let (offset, _, content) = chat.scroll();
    assert_eq!((offset, content), (0.0, 400.0));
    let last = chat.row(2).expect("the last row is built");
    assert_eq!(
        last.origin.y.0 + last.size.height.0 + 25.0,
        port.origin.y.0 + port.size.height.0
    );
}

#[test]
fn the_visible_rows_are_those_half_on_screen() {
    let mut chat = Chat::open(lines(0..100, mixed), None);
    for offset in [None, Some(1_234.0), Some(17.0)] {
        if let Some(offset) = offset {
            chat.scroll_to(offset);
        }

        let port = chat.port();
        let (top, bottom) = (port.origin.y.0, port.origin.y.0 + port.size.height.0);
        let shown: Vec<u64> = chat
            .on_screen()
            .into_iter()
            .filter(|(key, start)| {
                let height = mixed(*key);
                let inside = (start + height).min(bottom) - start.max(top);
                inside >= height / 2.0
            })
            .map(|(key, _)| key)
            .collect();
        let expected = shown[0] as usize..*shown.last().expect("a row is shown") as usize + 1;
        assert_eq!(chat.list().visible_untracked(), expected, "at {offset:?}");
    }
}
