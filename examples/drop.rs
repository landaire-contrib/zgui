//! Files dragged onto the window from outside it.
//!
//! Run it with `cargo run -p zgui-examples --example drop`, then drag files from a file manager
//! onto the zone.
//!
//! What it is worth reading for:
//!
//! * `drag_enter` and `drag_leave` follow a drag before it is dropped, so a zone can tell the user
//!   what it would take;
//! * the zone matches `:state(drag-over)` while a drag is over it, so the highlight is one rule in
//!   the sheet;
//! * `drop` goes to the element under the pointer, with every path at once.

use zgui::prelude::*;

/// What the zone says when no drag is over it.
const WAITING: &str = "Drop files here";

/// A zone that lists the files dropped on it.
#[component]
fn Zone() -> impl IntoView {
    let status = RwSignal::new(WAITING.to_owned());

    view! {
        column(
            class = "zone",
            on:drag_enter = move |cx| {
                status.set(format!("{} file(s) over the zone", cx.paths.len()));
            },
            on:drag_leave = move |_| status.set(WAITING.to_owned()),
            on:drop = move |cx| {
                let names: Vec<String> =
                    cx.paths.iter().map(|path| path.display().to_string()).collect();
                status.set(format!("Dropped {}", names.join(", ")));
            }
        ) {
            label(class = "zone__status") {{move || status.get()}}
        }
    }
}

/// How it looks.
const SHEET: &str = css!(
    ":root {
        background-color: #12141a;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .zone {
        width: 360px;
        height: 200px;
        align-items: center;
        justify-content: center;
        padding: 16px;
        border-radius: 14px;
        border: 2px dashed #2f3646;
        background-color: #191d26;
    }

    .zone:state(drag-over) {
        border-color: #3b6cf6;
        background-color: #1d2640;
    }

    .zone__status { font-size: 14px; color: #a9b2c4; text-align: center; }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Drop")
        .with_title("Drop")
        .with_size(480.0, 320.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Zone() })
}
