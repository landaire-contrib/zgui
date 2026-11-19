//! An application that lives in the menu bar.
//!
//! Run it with `cargo run -p zgui-examples --example menu_bar`.
//!
//! The window's close button hides the window, and the application keeps running in the menu bar.
//! "Show Window" in the status item's menu shows the window again, and so does a reopen request
//! from the desktop. "Quit" in either menu stops the application.
//!
//! What it is worth reading for:
//!
//! * `ExitPolicy::Explicit` with a close request that hides the window: the window keeps its whole
//!   document, so showing it again costs one frame;
//! * a status item with a template icon drawn in code, and a menu rebuilt from a signal;
//! * the Dock icon hidden while the window is hidden, and shown again with the window;
//! * a timer, a task and a foreign thread that go on counting while the window is hidden.

use std::time::Duration;

use zgui::desktop::{
    AppMenu, MenuAction, MenuRole, Shortcut, StatusIcon, StatusItem, Submenu, on_menu, on_reopen,
};
use zgui::prelude::*;

/// The side of the status item's picture, in pixels.
const ICON_SIZE: u32 = 36;

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

/// Shows every window, with the Dock icon, and brings the application to the front.
fn show(windows: &Windows) {
    windows.set_dock_icon_visible(true);
    for window in windows.all() {
        window.set_visible(true);
        window.focus();
    }
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
                "Close this window to hide it. The status item and the Dock icon show it again."
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

/// The status item, with its menu.
fn status_item() -> Option<StatusItem> {
    let counters = expect_context::<Counters>();
    let mut item = StatusItem::new()
        .with_tooltip("Menu bar example")
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
            let windows = use_windows();
            let picked = on_menu({
                let windows = windows.clone();
                move |id| match id.as_str() {
                    "show" => show(&windows),
                    "pause" => counters.paused.update(|paused| *paused = !*paused),
                    "quit" => windows.quit(),
                    _ => {}
                }
            });
            let reopened = on_reopen(move || show(&windows));
            // Held for the application's whole life.
            core::mem::forget((picked, reopened));
        })
        .with_menu(app_menu)
        .with_status_item(status_item)
        .run(|| view! { Panel() })
}
