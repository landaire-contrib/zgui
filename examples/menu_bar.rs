//! An application that lives in the menu bar.
//!
//! Run it with `cargo run -p zgui-examples --example menu_bar`.
//!
//! A click on the status item opens a small panel right under it, and a second click, Escape or a
//! click anywhere else closes it. A right click or a Control-click on the item opens its menu.
//! The window's close button hides the window, and the application keeps running in the menu bar.
//! "Show Window" in the status item's menu shows the window again, and so does a reopen request
//! from the desktop. "Quit" in either menu stops the application.
//!
//! What it is worth reading for:
//!
//! * `ExitPolicy::Explicit` with a close request that hides the window: the window keeps its whole
//!   document, so showing it again costs one frame;
//! * a status item with a template icon drawn in code, and a menu rebuilt from a signal;
//! * a status item that reports its presses, and a panel placed under it from where it is: a
//!   blurred window with the frame's corners and shadow and no title bar or buttons;
//! * the Dock icon hidden while the window is hidden, and shown again with the window;
//! * a timer, a task and a foreign thread that go on counting while the window is hidden.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use zgui::desktop::{
    AppMenu, MenuAction, MenuRole, Shortcut, StatusAnchor, StatusIcon, StatusItem, StatusPress,
    Submenu, on_menu, on_reopen, on_status_item_press,
};
use zgui::geom::{Css, CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::RenderEffect;

/// The side of the status item's picture, in pixels.
const ICON_SIZE: u32 = 36;

/// The width of the panel under the status item.
const PANEL_WIDTH: f32 = 320.0;

/// The height of the panel under the status item.
const PANEL_HEIGHT: f32 = 360.0;

/// The least distance between the panel and the side edges of the screen.
const PANEL_MARGIN: f32 = 8.0;

/// What the window, the menus and the status item share.
#[derive(Clone, Copy)]
struct Counters {
    /// Whether the counters stand still.
    paused: RwSignal<bool>,
    /// Seconds counted by a timer.
    timer: RwSignal<u64>,
    /// Seconds counted by a task that sleeps on a worker thread.
    task: RwSignal<u64>,
    /// Seconds counted by a thread outside the reactive system.
    thread: RwSignal<u64>,
}

impl Counters {
    /// Counters at zero, running.
    fn new() -> Self {
        Self {
            paused: RwSignal::new(false),
            timer: RwSignal::new(0),
            task: RwSignal::new(0),
            thread: RwSignal::new(0),
        }
    }

    /// Adds one second to `counter`, unless the counters stand still.
    fn tick(self, counter: RwSignal<u64>) {
        if !self.paused.get_untracked() {
            counter.update(|seconds| *seconds += 1);
        }
    }
}

/// The panel a click on the status item opens under it.
#[derive(Clone, Default)]
struct QuickPanel {
    /// The panel's window, from the first click on.
    window: Rc<RefCell<Option<WindowHandle>>>,
    /// Whether the panel shows.
    shown: Rc<Cell<bool>>,
}

impl QuickPanel {
    /// Shows the panel under the item `anchor` describes, or hides it when it shows.
    fn toggle(&self, windows: &Windows, anchor: &StatusAnchor) {
        if self.shown.get() {
            self.hide();
            return;
        }
        self.shown.set(true);

        let position = panel_position(anchor);
        let (x, y) = (position.x.0, position.y.0);
        let opened = self.window.borrow().clone();
        if let Some(window) = opened {
            window.set_position(x, y);
            window.set_visible(true);
            window.focus();
            return;
        }

        let options = WindowOptions::new("Quick panel")
            .with_size(PANEL_WIDTH, PANEL_HEIGHT)
            .with_position(x, y)
            .with_resizable(false)
            .with_decorations(Decorations::NoTitleBar)
            .with_title_buttons_hidden(true)
            .with_transparent(true)
            .with_blur(true)
            .with_stylesheet(PANEL_SHEET)
            .on_close_request({
                let panel = self.clone();
                move || {
                    panel.hide();
                    CloseResponse::Veto
                }
            });
        let window = windows.open(options, || view! { QuickPanelContent() });
        *self.window.borrow_mut() = Some(window);
    }

    /// Hides the panel, keeping its window for the next click.
    fn hide(&self) {
        self.shown.set(false);
        let opened = self.window.borrow().clone();
        if let Some(window) = opened {
            window.set_visible(false);
        }
    }

    /// Whether `window` is the panel's.
    fn is(&self, window: &WindowHandle) -> bool {
        self.window
            .borrow()
            .as_ref()
            .is_some_and(|panel| panel.id() == window.id())
    }
}

/// Shows every window but the panel, with the Dock icon, and brings the application to the front.
fn show(windows: &Windows, panel: &QuickPanel) {
    windows.set_dock_icon_visible(true);
    for window in windows.all() {
        if panel.is(&window) {
            continue;
        }
        window.set_visible(true);
        window.focus();
    }
}

/// Where the panel's top left corner goes: centered under the item, and inside the part of the
/// screen windows may cover.
fn panel_position(anchor: &StatusAnchor) -> Point<CssPx, Css> {
    let item = anchor.item;
    let area = anchor.work_area;
    let centered = item.origin.x.0 + (item.size.width.0 - PANEL_WIDTH) / 2.0;
    let leftmost = area.origin.x.0 + PANEL_MARGIN;
    let rightmost = area.origin.x.0 + area.size.width.0 - PANEL_WIDTH - PANEL_MARGIN;
    let x = centered.min(rightmost).max(leftmost);
    let y = (item.origin.y.0 + item.size.height.0).max(area.origin.y.0);
    Point::new(CssPx(x), CssPx(y))
}

/// The window: three counters and a button that pauses them.
#[component]
fn Panel() -> impl IntoView {
    let counters = expect_context::<Counters>();
    let window = use_window();
    let windows = use_windows();

    // The close button hides the window, and the Dock icon goes with it.
    let close = on_close_request({
        let window = window.clone();
        move || {
            window.set_visible(false);
            windows.set_dock_icon_visible(false);
            CloseResponse::Veto
        }
    });
    // Held for the window's whole life, which is the application's.
    core::mem::forget(close);

    let interval = set_interval(Duration::from_secs(1), move || {
        counters.tick(counters.timer);
    });
    core::mem::forget(interval);

    spawn(async move {
        loop {
            blocking(|| std::thread::sleep(Duration::from_secs(1))).await;
            counters.tick(counters.task);
        }
    });

    // `ui()` answers for the thread it is called on, so it is taken here.
    let ui = ui();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            ui.post(move || counters.tick(counters.thread));
        }
    });

    let seconds = |counter: RwSignal<u64>| move || format!("{} s", counter.get());

    view! {
        column(class = "panel") {
            label(class = "panel__title") {"Menu bar"}
            label(class = "panel__hint") {
                "Close this window to hide it. \"Show Window\" in the status item's menu and the \
                 Dock icon show it again."
            }
            row(class = "panel__row") {
                label {"Timer"}
                spacer()
                label(class = "panel__value") {{seconds(counters.timer)}}
            }
            row(class = "panel__row") {
                label {"Task"}
                spacer()
                label(class = "panel__value") {{seconds(counters.task)}}
            }
            row(class = "panel__row") {
                label {"Thread"}
                spacer()
                label(class = "panel__value") {{seconds(counters.thread)}}
            }
            control(
                class = "panel__button",
                tabindex = Focus::Sequential,
                on:click = move |_| counters.paused.update(|paused| *paused = !*paused)
            ) {
                {move || if counters.paused.get() { "Resume" } else { "Pause" }}
            }
        }
    }
}

/// What the panel under the status item shows: the counters at a glance.
#[component]
fn QuickPanelContent() -> impl IntoView {
    let counters = expect_context::<Counters>();
    let panel = use_local_context::<QuickPanel>().expect("the application provides the panel");
    let window = use_window();

    // A click anywhere else takes the keyboard from the panel, and the panel goes, as a menu does.
    let focused = window.focused();
    let blurred = RenderEffect::new({
        let panel = panel.clone();
        move |was_focused: Option<bool>| {
            let focused = focused.get();
            if was_focused == Some(true) && !focused {
                panel.hide();
            }
            focused
        }
    });

    // The window shows once its first frame is on screen, and only a window that shows can take
    // the keyboard. So the panel asks for it on the turn after that frame. A second click before
    // then hid the panel while its window could not be hidden yet, and the window goes instead.
    let timers = Timers::current().expect("a component runs inside a window");
    let later = Rc::new(RefCell::new(None));
    let first_frame = request_frame({
        let later = Rc::clone(&later);
        let panel = panel.clone();
        let window = window.clone();
        move |_| {
            let settle = move || {
                if panel.shown.get() {
                    window.focus();
                } else {
                    window.set_visible(false);
                }
            };
            *later.borrow_mut() = Some(timers.set_timeout(Duration::ZERO, settle));
        }
    });

    // What hears Escape while nothing in the panel has focus.
    let anchor = NodeRef::new();
    let shortcut = RenderEffect::new(move |previous: Option<Option<WindowShortcut>>| {
        drop(previous);
        anchor.get();
        anchor.window_shortcut()
    });
    on_cleanup_local(move || drop((blurred, first_frame, later, shortcut)));

    let seconds = |counter: RwSignal<u64>| move || format!("{} s", counter.get());

    view! {
        column(
            class = "quick",
            node_ref = anchor,
            on:key_down = move |ev| {
                if matches!(ev.key, Key::Named(NamedKey::Escape)) {
                    panel.hide();
                }
            }
        ) {
            label(class = "panel__title") {"Quick panel"}
            label(class = "panel__hint") {"Escape or a click anywhere else closes it."}
            row(class = "panel__row") {
                label {"Timer"}
                spacer()
                label(class = "panel__value") {{seconds(counters.timer)}}
            }
            row(class = "panel__row") {
                label {"Task"}
                spacer()
                label(class = "panel__value") {{seconds(counters.task)}}
            }
            row(class = "panel__row") {
                label {"Thread"}
                spacer()
                label(class = "panel__value") {{seconds(counters.thread)}}
            }
        }
    }
}

/// The status item's picture: a ring with a dot in the middle, as a mask.
fn icon() -> Option<StatusIcon> {
    let center = (ICON_SIZE as f32 - 1.0) / 2.0;
    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let distance = (x as f32 - center).hypot(y as f32 - center);
            let ring = (11.0..=16.0).contains(&distance);
            let dot = distance <= 5.0;
            let alpha = if ring || dot { 255 } else { 0 };
            rgba.extend_from_slice(&[0, 0, 0, alpha]);
        }
    }
    let image = WindowIcon::from_rgba(rgba, ICON_SIZE, ICON_SIZE).ok()?;
    Some(StatusIcon::new(image).with_template(true))
}

/// The status item, with its menu. A click opens the panel, and a secondary click the menu.
fn status_item() -> Option<StatusItem> {
    let counters = expect_context::<Counters>();
    let mut item = StatusItem::new()
        .with_tooltip("Menu bar example")
        .with_press(StatusPress::Reports)
        .with(MenuAction::new("show", "Show Window"))
        .with(MenuAction::new("pause", "Pause Counters").with_checked(counters.paused.get()))
        .separator()
        .with(MenuAction::new("quit", "Quit"));
    if let Some(icon) = icon() {
        item = item.with_icon(icon);
    }
    Some(item)
}

/// The application menu.
fn app_menu() -> AppMenu {
    AppMenu::new().with(
        Submenu::new("Menu bar")
            .with(MenuRole::About)
            .separator()
            .with(MenuRole::Hide)
            .with(MenuRole::HideOthers)
            .with(MenuRole::ShowAll)
            .separator()
            .with(
                MenuAction::new("quit", "Quit Menu bar")
                    .with_shortcut(Shortcut::new(Key::character("q"), Modifiers::PRIMARY)),
            ),
    )
}

/// How it looks.
const SHEET: &str = css!(
    ":root {
        background-color: #16181d;
        color: #e8ecf4;
        font-family: sans-serif;
        display: block;
    }

    .panel {
        gap: 10px;
        padding: 24px;
    }

    .panel__title { font-size: 18px; }

    .panel__hint {
        font-size: 12px;
        color: #8a93a6;
    }

    .panel__row {
        align-items: center;
        font-size: 14px;
    }

    .panel__value { color: #9ecbff; }

    .panel__button {
        margin-top: 8px;
        padding: 6px 12px;
        border-radius: 7px;
        background-color: #262b36;
        text-align: center;
    }

    .panel__button:hover { background-color: #323948; }"
);

/// How the panel under the status item looks, over the sheet above.
const PANEL_SHEET: &str = css!(
    ":root { background-color: transparent; }

    .quick {
        gap: 10px;
        padding: 20px;
        height: 100%;
        background-color: rgba(22, 24, 29, 0.6);
    }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.MenuBar")
        .with_title("Menu bar")
        .with_size(360.0, 260.0)
        .with_stylesheet(SHEET)
        // The application stops when it says so, which is when the user picks Quit.
        .with_exit_policy(ExitPolicy::Explicit)
        .with_context(|| {
            let counters = Counters::new();
            provide_context(counters);
            let panel = QuickPanel::default();
            provide_local_context(panel.clone());
            let windows = use_windows();
            let picked = on_menu({
                let windows = windows.clone();
                let panel = panel.clone();
                move |id| match id.as_str() {
                    "show" => show(&windows, &panel),
                    "pause" => counters.paused.update(|paused| *paused = !*paused),
                    "quit" => windows.quit(),
                    _ => {}
                }
            });
            let reopened = on_reopen({
                let windows = windows.clone();
                let panel = panel.clone();
                move || show(&windows, &panel)
            });
            let pressed = on_status_item_press(move |anchor| panel.toggle(&windows, anchor));
            // Held for the application's whole life.
            core::mem::forget((picked, reopened, pressed));
        })
        .with_menu(app_menu)
        .with_status_item(status_item)
        .run(|| view! { Panel() })
}
