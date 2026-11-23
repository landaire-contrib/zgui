//! The grabber at the top of a bottom sheet, which a reader and the keyboard can operate.

use zgui::component;
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::RwSignal;
use zgui::reactive::StoredValue;
use zgui::reactive::UnsyncCallback;
use zgui::view;
use zgui::vocab::Key;
use zgui::vocab::NamedKey;

use crate::bottom_sheet::detent::Detent;
use crate::bottom_sheet::detent::larger;
use crate::bottom_sheet::detent::smaller;

/// The grabber of a sheet with more than one detent.
///
/// A press, <kbd>Enter</kbd> and <kbd>Space</kbd> move the sheet to its next larger detent, or
/// from its largest detent to the one below. <kbd>Arrow Up</kbd> and an increment expand the sheet.
/// <kbd>Arrow Down</kbd> and a decrement collapse it, and at its smallest detent they close it.
#[component]
pub(crate) fn Grabber(
    /// The detent the sheet rests at.
    current: RwSignal<Detent, LocalStorage>,
    /// The detents, from smallest to largest.
    detents: StoredValue<Vec<Detent>, LocalStorage>,
    /// Moves the sheet to a detent.
    on_rest: UnsyncCallback<Detent>,
    /// Asks to close the sheet.
    on_dismiss: UnsyncCallback<()>,
) -> impl IntoView {
    let expand = move || {
        let to = detents.with_value(|detents| larger(detents, current.get_untracked()));
        on_rest.run(to);
    };
    let collapse = move || {
        match detents.with_value(|detents| smaller(detents, current.get_untracked())) {
            Some(to) => on_rest.run(to),
            None => on_dismiss.run(()),
        }
    };
    let toggle = move || {
        let detent = current.get_untracked();
        let largest = detents.with_value(|detents| detents.last().copied());
        if largest == Some(detent) {
            if let Some(to) = detents.with_value(|detents| smaller(detents, detent)) {
                on_rest.run(to);
            }
        } else {
            expand();
        }
    };

    // The step value is the index of the detent. One step below the smallest closes the sheet.
    let index = move || position(detents, current.get());
    let top = detents.with_value(Vec::len) as f64 - 1.0;
    let semantics = A11yBinding::new(Role::Button)
        .label("Sheet grabber")
        .value(move || match current.get() {
            Detent::Medium => "Medium".to_owned(),
            Detent::Large => "Large".to_owned(),
        })
        .numeric_value(index)
        .step(move |a11y| a11y.numeric_range(-1.0, top).numeric_step(1.0));
    let own = Attrs::new().a11y_from(semantics);

    view! {
        control(
            class = "zui-bottom-sheet__grabber",
            tabindex = {Focus::Sequential},
            on:click = move |_| toggle(),
            on:key_down = move |ev| {
                match &ev.key {
                    Key::Named(NamedKey::ArrowUp) => expand(),
                    Key::Named(NamedKey::ArrowDown) => collapse(),
                    _ => return,
                }
                // Claimed, so the key does not also scroll the content.
                ev.prevent_default();
                ev.stop_propagation();
            },
            // An increment or a decrement from a reader, as the stepped index.
            on:change = move |ev| {
                let Ok(asked) = ev.value.parse::<f64>() else {
                    return;
                };
                let now = position(detents, current.get_untracked());
                if asked > now {
                    expand();
                } else if asked < now {
                    collapse();
                }
            },
            {..own}
        ) {
            box(class = "zui-bottom-sheet__grabber-bar", a11y:hidden = true)
        }
    }
}

/// The index of `detent` among `detents`, as a step value.
fn position(detents: StoredValue<Vec<Detent>, LocalStorage>, detent: Detent) -> f64 {
    detents.with_value(|detents| detents.iter().filter(|each| **each < detent).count()) as f64
}
