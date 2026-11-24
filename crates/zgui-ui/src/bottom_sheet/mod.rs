//! A sheet that rises from the bottom of the window and rests at detents, as on iOS.

mod detent;
mod grabber;
mod style;

pub use crate::bottom_sheet::detent::Detent;
pub use crate::bottom_sheet::style::BottomSheetStyle;

use zgui::component;
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::RenderEffect;
use zgui::reactive::RwSignal;
use zgui::reactive::StoredValue;
use zgui::reactive::UnsyncCallback;
use zgui::view;
use zgui::view::AttrName;
use zgui::view::ClassName;
use zgui::view::CustomPropertyName;
use zgui::view::Ident;
use zgui::vocab::GesturePhase;
use zgui::vocab::ListenerOptions;
use zgui::vocab::PanEvent;
use zgui_ui_primitives::Binding;
use zgui_ui_primitives::Controllable;

use crate::bottom_sheet::detent::Geometry;
use crate::bottom_sheet::detent::Release;
use crate::bottom_sheet::detent::release;
use crate::bottom_sheet::grabber::GrabberProps;
use crate::overlay::ModalSurfaceProps;
use crate::overlay::OverlayState;
use crate::overlay::SurfaceLabels;
use crate::support::fling::fling_ease;

/// What the bottom sheet's rules are installed under.
const SHEET_NAME: &str = "zui-bottom-sheet";

/// A modal sheet that rises from the bottom of the window and rests at one of its detents.
///
/// The person drags the sheet by any part of it to move it between its detents, or down to close
/// it. A drag starts where the sheet is on the window, also while it moves. A drag down that starts
/// in a scroll container at its top moves the sheet. A press on the dimmed window and
/// <kbd>Escape</kbd> close it. Focus stays in the sheet while it is open, and goes back to where it
/// was when it closes.
///
/// When a field in the sheet shows the keyboard, the sheet rises to its large detent. In a window
/// at least 700 pixels wide and tall, as on an iPad, the sheet is a form sheet in the middle of the
/// window.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A password generator at half height, which can grow to full height.
/// #[component]
/// fn Generator() -> impl IntoView {
///     let open = RwSignal::new_local(false);
///     view! {
///         Button(on:click = move |_| open.set(true)) {"Generate"}
///         BottomSheet(
///             open = open,
///             detents = vec![Detent::Medium, Detent::Large],
///             label = "Generate Password"
///         ) {
///             text {"k3#Vq9!xLm"}
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// <kbd>Escape</kbd> closes it and <kbd>Tab</kbd> is confined to it, as in a dialog. With more than
/// one detent, the sheet has a grabber at its top. <kbd>Enter</kbd> and <kbd>Space</kbd> on the
/// grabber move the sheet to its other detent, <kbd>Arrow Up</kbd> expands it and
/// <kbd>Arrow Down</kbd> collapses it.
#[component]
pub fn BottomSheet(
    /// Whether it is open, when the caller holds it.
    ///
    /// A writable signal is written back to. A value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    open: Binding<bool>,
    /// Whether it starts open, when it owns that itself.
    #[prop(default = false)]
    default_open: bool,
    /// Told whenever it opens or closes, whoever owns it.
    #[prop(optional)]
    on_open_change: Option<UnsyncCallback<bool>>,
    /// The heights it rests at. It opens at the smallest one.
    #[prop(into, default = vec![Detent::Large])]
    detents: Vec<Detent>,
    /// The detent it rests at, when the caller holds it.
    #[prop(optional)]
    detent: Option<RwSignal<Detent, LocalStorage>>,
    /// Whether the person cannot close it, for example while it holds unsaved edits.
    ///
    /// While it is true, a drag down, a press on the dimmed window, <kbd>Escape</kbd> and a collapse
    /// of the grabber at the smallest detent leave it open and tell `on_dismiss_attempt`. The caller
    /// can still close it through `open`.
    #[prop(into, default = Signal::stored_local(false))]
    dismiss_blocked: Signal<bool, LocalStorage>,
    /// Told when the person tried to close it while `dismiss_blocked` was true.
    #[prop(optional)]
    on_dismiss_attempt: Option<UnsyncCallback<()>>,
    /// What the sheet is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the sheet's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the sheet.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is in the sheet.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET_NAME, BottomSheetStyle::CSS);
    let detents = StoredValue::new_local(normalized(detents));
    let smallest = detents.with_value(|detents| detents[0]);
    let largest = detents.with_value(|detents| detents[detents.len() - 1]);

    let refuse = move || {
        if let Some(attempt) = on_dismiss_attempt {
            attempt.run(());
        }
    };
    let outer = Controllable::new(open, default_open, on_open_change);
    let gated = Binding::controlled(outer.signal(), move |next: bool| {
        if !next && dismiss_blocked.get_untracked() {
            refuse();
            return;
        }
        outer.set(next);
    });
    let state = OverlayState::new(gated, false, None).provide();
    let labels = SurfaceLabels::provide();

    let held = detent.is_some();
    let current = detent.unwrap_or_else(|| RwSignal::new_local(smallest));
    let drag = Drag::new();
    let floor = move |geometry: Geometry| {
        dismiss_blocked
            .get_untracked()
            .then(|| geometry.offset(smallest))
    };

    // Each time it opens, it starts at rest, at the smallest detent unless the caller holds it.
    let opening = RenderEffect::new(move |was: Option<bool>| {
        let open = state.is_open();
        if open && was != Some(true) {
            drag.reset();
            if !held {
                current.set(smallest);
            }
        }
        open
    });
    on_cleanup_local(move || drop(opening));

    let surface = state.content();
    let on_pan = move |pan: PanEvent| match pan.phase {
        GesturePhase::Started => {
            let Some(seen) = measure(surface) else {
                return;
            };
            // A stretched sheet is taller than at its large detent until the stretch settles.
            let geometry = drag.stretched.get_value().unwrap_or(seen.geometry);
            // The drag starts where the sheet is drawn, which differs from its detent while it
            // settles and while the keyboard holds it at its large detent.
            let drawn = seen.top - geometry.top();
            let from = geometry.unresisted(drawn, floor(geometry));
            drag.grab.set_value(Some(Grab { from, geometry }));
            drag.active.set(true);
            drag.offset.set(Some(drawn));
        }
        GesturePhase::Moved => {
            let Some(grab) = drag.grab.get_value() else {
                return;
            };
            drag.offset.set(Some(
                grab.geometry
                    .resisted(grab.from + pan.translation().height.0, floor(grab.geometry)),
            ));
        }
        GesturePhase::Ended => {
            drag.framed.set_value(false);
            let Some(grab) = drag.grab.get_value() else {
                return;
            };
            drag.grab.set_value(None);
            let at = drag.offset.get_untracked().unwrap_or(grab.from);
            let dismissable = !dismiss_blocked.get_untracked();
            // A blocked sheet resists the finger, so the finger decides whether it asked to close.
            let decide = if dismissable {
                at
            } else {
                grab.from + pan.translation().height.0
            };
            let to = detents.with_value(|detents| {
                release(decide, pan.velocity.y, detents, grab.geometry, dismissable)
            });
            match to {
                Release::Rest(detent) => {
                    drag.settle(current, detent, at, pan.velocity.y, grab.geometry)
                }
                Release::Refused(detent) => {
                    drag.settle(current, detent, at, pan.velocity.y, grab.geometry);
                    refuse();
                }
                // The sheet leaves from where the finger let go of it.
                Release::Dismiss => {
                    drag.active.set(false);
                    state.close();
                    // A caller that holds `open` may decline.
                    if state.open_signal().get_untracked() {
                        drag.settle(current, smallest, at, pan.velocity.y, grab.geometry);
                    }
                }
            }
        }
        // A cancelled drag goes back to the detent it rests at.
        _ => {
            drag.framed.set_value(false);
            let grab = drag.grab.get_value();
            drag.grab.set_value(None);
            if let (Some(grab), Some(at)) = (grab, drag.offset.get_untracked()) {
                drag.settle(current, current.get_untracked(), at, 0.0, grab.geometry);
            }
        }
    };
    let rest = UnsyncCallback::new(move |detent: Detent| drag.rest(current, detent));
    let dismiss = UnsyncCallback::new(move |()| state.close());

    let mut semantics = A11yBinding::unspecified()
        .labelled_by(labels.title())
        .described_by(labels.description());
    if let Some(label) = label {
        semantics = semantics.label(label);
    }
    let own = Attrs::new()
        .class_toggle(ClassName::new(BottomSheetStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-bottom-sheet"), true)
        .attribute(AttrName::new("data-detent"), move || {
            Some(current.get().name().to_owned())
        })
        .attribute(AttrName::new("data-dragging"), move || {
            drag.active.get().then(String::new)
        })
        .custom_property(
            CustomPropertyName::new("zui-bottom-sheet-offset"),
            move || {
                Some(match drag.offset.get() {
                    Some(offset) => format!("{offset}px"),
                    None => rest_offset(current.get()).to_owned(),
                })
            },
        )
        .custom_property(
            CustomPropertyName::new("zui-bottom-sheet-stretch"),
            move || {
                drag.offset
                    .get()
                    .filter(|offset| *offset < 0.0)
                    .map(|offset| format!("{}px", -offset))
            },
        )
        .custom_property(
            CustomPropertyName::new("zui-bottom-sheet-ease"),
            move || drag.ease.get(),
        )
        .listener(
            events::TRANSITION_END,
            ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::TransitionEnd>| {
                if ev.target == ev.current {
                    drag.settled(ev.property);
                }
            },
        )
        .listener(
            events::TRANSITION_CANCEL,
            ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::TransitionCancel>| {
                if ev.target == ev.current {
                    drag.settled(ev.property);
                }
            },
        )
        .a11y_from(semantics);

    let grabber = detents.with_value(Vec::len) > 1;
    let children = StoredValue::new_local(children);

    view! {
        ModalSurface(
            state = state,
            role = {Role::Dialog},
            dismiss_on_outside_press = true,
            dismiss_on_escape = true,
            {..own},
            {..attrs},
            class = class
        ) {
            // Offered every pan before a scroll container inside it. Takes an upward pan that can
            // grow the sheet, and leaves the others to the scroll containers and to the body.
            column(
                class = "zui-bottom-sheet__frame",
                on:vertical_pan:capture = move |ev| {
                    let pan: PanEvent = **ev;
                    if pan.phase == GesturePhase::Started {
                        let upward = pan.translation().height.0 < 0.0;
                        let grown = current.get_untracked() == largest
                            || measure(surface)
                                .is_some_and(|seen| seen.top <= seen.geometry.top());
                        if !upward || grown {
                            ev.prevent_default();
                            return;
                        }
                        drag.framed.set_value(true);
                    }
                    if drag.framed.get_value() {
                        on_pan(pan);
                    }
                }
            ) {
                // Offered a pan only when no scroll container under the finger can scroll its way,
                // so a drag down on a list at its top moves the sheet.
                column(
                    class = "zui-bottom-sheet__body",
                    on:vertical_pan = move |ev| {
                        if !drag.framed.get_value() {
                            on_pan(**ev);
                        }
                    }
                ) {
                    if move || grabber {
                        Grabber(
                            current = current,
                            detents = detents,
                            on_rest = rest,
                            on_dismiss = dismiss
                        )
                    } else {}
                    column(class = "zui-bottom-sheet__content") {
                        {children.with_value(ChildrenFn::view)}
                    }
                }
            }
        }
    }
}

/// Where a drag started, and what it is measured against.
#[derive(Copy, Clone)]
struct Grab {
    /// The offset of the finger when it took the sheet, before resistance.
    from: f32,
    /// The sizes of the sheet and the window.
    geometry: Geometry,
}

/// The sheet as the last frame drew it.
#[derive(Copy, Clone)]
struct Sighting {
    /// The sizes of the sheet and the window.
    geometry: Geometry,
    /// Where the top of the sheet is in the window.
    top: f32,
}

/// The state of a drag on the sheet.
#[derive(Copy, Clone)]
struct Drag {
    /// The drag in progress.
    grab: StoredValue<Option<Grab>, LocalStorage>,
    /// Whether the frame took the drag in progress, to grow the sheet.
    framed: StoredValue<bool, LocalStorage>,
    /// Whether a finger holds the sheet, which turns its transition off.
    active: RwSignal<bool, LocalStorage>,
    /// Where the sheet is drawn while a finger holds it, or as it leaves after a drag.
    offset: RwSignal<Option<f32>, LocalStorage>,
    /// The curve of the settle after a release, which carries the speed of the finger.
    ease: RwSignal<Option<String>, LocalStorage>,
    /// The sizes measured at the start of a drag that stretched the sheet, until the stretch
    /// settles.
    stretched: StoredValue<Option<Geometry>, LocalStorage>,
}

impl Drag {
    fn new() -> Self {
        Self {
            grab: StoredValue::new_local(None),
            framed: StoredValue::new_local(false),
            active: RwSignal::new_local(false),
            offset: RwSignal::new_local(None),
            ease: RwSignal::new_local(None),
            stretched: StoredValue::new_local(None),
        }
    }

    fn reset(self) {
        self.grab.set_value(None);
        self.framed.set_value(false);
        self.active.set(false);
        self.offset.set(None);
        self.ease.set(None);
        self.stretched.set_value(None);
    }

    /// Lets go of the sheet at `at`, moving at `velocity`, so it settles at `detent`.
    fn settle(
        self,
        current: RwSignal<Detent, LocalStorage>,
        detent: Detent,
        at: f32,
        velocity: f32,
        geometry: Geometry,
    ) {
        // The settle takes the 500 ms of the sheet's transition.
        let distance = geometry.offset(detent) - at;
        self.ease.set(fling_ease(distance, velocity, 0.5));
        if at < 0.0 {
            self.stretched.set_value(Some(geometry));
        }
        current.set(detent);
        self.offset.set(None);
        self.active.set(false);
    }

    /// Moves the sheet to `detent` on the sheet spring.
    fn rest(self, current: RwSignal<Detent, LocalStorage>, detent: Detent) {
        self.ease.set(None);
        current.set(detent);
    }

    /// Ends the settle when the transition of `property` on the sheet stops.
    fn settled(self, property: Ident) {
        if self.active.get_untracked() {
            return;
        }

        match property.as_str() {
            "translate" => self.ease.set(None),
            "top" => {
                self.ease.set(None);
                self.stretched.set_value(None);
            }
            // Another property of the sheet, such as the opacity of its entrance.
            _ => {}
        }
    }
}

/// The detents sorted from smallest to largest, without repeats. Large when none were given.
fn normalized(mut detents: Vec<Detent>) -> Vec<Detent> {
    detents.sort_unstable();
    detents.dedup();
    if detents.is_empty() {
        detents.push(Detent::Large);
    }

    detents
}

/// The offset of a sheet at rest at `detent`, as CSS. A percentage is of the sheet's own height.
fn rest_offset(detent: Detent) -> &'static str {
    match detent {
        Detent::Large => "0px",
        Detent::Medium => "calc(100% - 50vh)",
    }
}

/// The sheet as the last frame drew it, in CSS pixels.
fn measure(surface: NodeRef) -> Option<Sighting> {
    let scale = surface.scale();
    // The width and height at which the style sheet shows a form sheet.
    const FORM: f32 = 700.0;

    let drawn = surface.window_bounds()?;
    let panel = drawn.size.height.0 / scale;
    let root = surface.window_root()?.bounds()?.size;
    let window = root.height.0 / scale;
    let form = root.width.0 / scale >= FORM && window >= FORM;

    Some(Sighting {
        geometry: Geometry {
            panel,
            window,
            form,
        },
        top: drawn.origin.y.0 / scale,
    })
}
