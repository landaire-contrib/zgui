//! A context menu opened by a finger: the region lifted above a dimmed window, an accessory above
//! it and the menu below it.

use zgui::geom::{Css, CssPx, Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, RwSignal, StoredValue, UnsyncCallback};
use zgui::view::AttrName;
use zgui::{component, slot, view};
use zgui_ui_primitives::prelude::*;

use crate::context_menu::style::ContextMenuLiftStyle;
use crate::menu::{MenuListProps, panel};
use crate::overlay::{
    ElevatedProps, OverlayState, OverlaySurfaceProps, ScrimProps, SurfaceElevation, use_scroll_lock,
};

/// The space between the preview, the accessory and the menu, in CSS pixels.
const GAP: f32 = 8.0;

/// What a [`ContextMenuContent`](crate::ContextMenuContent) shows above a dimmed window when a
/// long press opens it.
///
/// A long press by a finger lifts the region: the window dims and blurs, a copy of the region
/// stands where the region was, the [`ContextMenuAccessory`] sits above it and the items sit below
/// it. A mouse, a pen or the keyboard opens the items at the pointer as before.
///
/// The copy is what the children build, at the width of `source`. It takes no pointer: a tap on
/// it closes the menu. `source` is hidden while the copy stands in for it.
#[slot]
#[derive(Clone)]
pub struct ContextMenuPreview {
    /// The element the copy stands in for. The trigger's region when not given.
    #[prop(optional)]
    pub source: Option<NodeRef>,
    /// The edge of the copy that the accessory and the items line up with.
    #[prop(default = Align::Start)]
    pub align: Align,
    /// The copy.
    pub children: ChildrenFn,
}

/// What a lifted [`ContextMenuContent`](crate::ContextMenuContent) shows above its
/// [`ContextMenuPreview`], such as a row of reactions.
#[slot]
#[derive(Clone)]
pub struct ContextMenuAccessory {
    /// What it shows.
    pub children: ChildrenFn,
}

/// How an open context menu is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presentation {
    /// At the point it was asked for.
    AtPoint,
    /// Below a lifted preview of the region.
    Lifted,
}

/// What a [`ContextMenu`](crate::ContextMenu) shares between its trigger and its content.
#[derive(Clone, Copy)]
pub(crate) struct LiftCx {
    /// How the menu is shown, which the trigger decides as it opens it.
    pub(crate) presentation: RwSignal<Presentation, LocalStorage>,
    /// The trigger's region.
    pub(crate) area: NodeRef,
}

impl LiftCx {
    pub(crate) fn new() -> Self {
        Self {
            presentation: RwSignal::new_local(Presentation::AtPoint),
            area: NodeRef::new(),
        }
    }

    pub(crate) fn provide(self) -> Self {
        provide_local_context(self);
        self
    }

    /// The one of the enclosing context menu, or one of its own outside a context menu.
    pub(crate) fn current() -> Self {
        use_local_context::<Self>().unwrap_or_else(Self::new)
    }
}

/// Marks the scope of a lifted menu's parts.
#[derive(Clone, Copy)]
struct Lifted;

/// Whether the calling scope is inside a context menu that a finger opened below a lifted copy of
/// its region. Items can read it to differ from the ones a pointer opens, such as by showing
/// symbols.
pub fn is_lifted() -> bool {
    use_local_context::<Lifted>().is_some()
}

/// Which side of the preview the menu went to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuSide {
    Below,
    Above,
}

impl MenuSide {
    fn name(self) -> &'static str {
        match self {
            Self::Below => "bottom",
            Self::Above => "top",
        }
    }
}

/// Where the parts of a lifted menu go, in window CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Arrangement {
    /// The corner of the preview.
    pub(crate) preview: Point<CssPx, Css>,
    /// The height the preview is cut to, when it does not fit whole.
    pub(crate) clip: Option<CssPx>,
    /// The corner of the accessory.
    pub(crate) accessory: Point<CssPx, Css>,
    /// The corner of the menu.
    pub(crate) menu: Point<CssPx, Css>,
    /// The height the menu is cut to and scrolls in, when it does not fit whole.
    pub(crate) menu_clip: Option<CssPx>,
    /// Which side of the preview the menu is on.
    pub(crate) side: MenuSide,
}

/// The sizes of the parts of a lifted menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Parts {
    pub(crate) preview: Size<CssPx, Css>,
    pub(crate) accessory: Option<Size<CssPx, Css>>,
    pub(crate) menu: Size<CssPx, Css>,
}

/// Arranges `parts` inside `bounds` for a region at `source`.
///
/// The preview stays where the region is when the accessory fits above it and the menu fits below
/// or above. Otherwise the preview moves the least distance that lets everything fit, with the menu
/// on the side that needs less movement. A preview too tall to fit is cut at the bottom, and a menu
/// too tall to fit beside the accessory is cut to the room left. The accessory and the menu line up
/// with the preview's edge at `align` and stay inside `bounds`.
pub(crate) fn arrange(
    source: Rect<CssPx, Css>,
    parts: Parts,
    bounds: Rect<CssPx, Css>,
    align: Align,
) -> Arrangement {
    let accessory_room = parts.accessory.map_or(0.0, |size| size.height.0 + GAP);
    let menu_room = parts.menu.height.0 + GAP;
    let height = parts.preview.height.0;
    let wanted = source.top().0;

    // The range of tops each side allows, as (lowest, highest).
    let below = (
        bounds.top().0 + accessory_room,
        bounds.bottom().0 - menu_room - height,
    );
    let above = (
        bounds.top().0 + accessory_room + menu_room,
        bounds.bottom().0 - height,
    );
    let place =
        |(lowest, highest): (f32, f32)| (lowest <= highest).then(|| wanted.clamp(lowest, highest));
    let placed = match (place(below), place(above)) {
        (Some(low), Some(high)) if (high - wanted).abs() < (low - wanted).abs() => {
            Some((high, MenuSide::Above))
        }
        (Some(low), _) => Some((low, MenuSide::Below)),
        (None, Some(high)) => Some((high, MenuSide::Above)),
        (None, None) => None,
    };
    let (top, clip, menu_clip, side) = match placed {
        Some((top, side)) => (top, None, None, side),
        None => {
            let room = bounds.height().0 - accessory_room - menu_room;
            // With no room for the preview at all, the preview is cut away and the menu takes
            // what is left below the accessory.
            let menu_clip =
                (room < 0.0).then(|| CssPx((bounds.height().0 - accessory_room - GAP).max(0.0)));
            (
                bounds.top().0 + accessory_room,
                Some(CssPx(room.max(0.0))),
                menu_clip,
                MenuSide::Below,
            )
        }
    };
    let shown = clip.map_or(height, |clip| clip.0);

    let left = fit(source.left().0, parts.preview.width.0, bounds);
    let lined_up = |width: f32| {
        let left = match align {
            Align::Start => left,
            Align::Center => left + (parts.preview.width.0 - width) / 2.0,
            Align::End => left + parts.preview.width.0 - width,
        };
        fit(left, width, bounds)
    };
    let accessory_size = parts.accessory.unwrap_or(Size::new(CssPx(0.0), CssPx(0.0)));
    let accessory_top = top - accessory_room;
    let menu_top = match side {
        MenuSide::Below => top + shown + GAP,
        MenuSide::Above => accessory_top - menu_room,
    };

    Arrangement {
        preview: Point::new(CssPx(left), CssPx(top)),
        clip,
        accessory: Point::new(
            CssPx(lined_up(accessory_size.width.0)),
            CssPx(accessory_top),
        ),
        menu: Point::new(CssPx(lined_up(parts.menu.width.0)), CssPx(menu_top)),
        menu_clip,
        side,
    }
}

/// `left` moved the least that puts `width` inside `bounds`, or at its left edge when it is wider.
fn fit(left: f32, width: f32, bounds: Rect<CssPx, Css>) -> f32 {
    left.min(bounds.right().0 - width).max(bounds.left().0)
}

/// A rectangle in device pixels as CSS pixels at `scale` device pixels per CSS pixel.
fn css_rect(rect: Rect<DevicePx, Device>, scale: f32) -> Rect<CssPx, Css> {
    Rect::new(
        Point::new(
            CssPx(rect.origin.x.0 / scale),
            CssPx(rect.origin.y.0 / scale),
        ),
        css_size(rect.size, scale),
    )
}

/// A size in device pixels as CSS pixels at `scale` device pixels per CSS pixel.
fn css_size(size: Size<DevicePx, Device>, scale: f32) -> Size<CssPx, Css> {
    Size::new(CssPx(size.width.0 / scale), CssPx(size.height.0 / scale))
}

/// A length as a CSS pixel value.
fn px(value: f32) -> String {
    format!("{value}px")
}

/// The menu of `state` below a lifted preview, over a dimmed window.
#[component]
pub(crate) fn LiftedMenu(
    /// The menu.
    state: OverlayState,
    /// What stands in for the region.
    preview: ContextMenuPreview,
    /// What stands above the preview.
    accessory: Option<ContextMenuAccessory>,
    /// The trigger's region, which the preview stands in for when it names no source.
    area: NodeRef,
    /// Classes of the menu panel.
    class: Classes,
    /// Attributes of the menu panel.
    attrs: Attrs,
    /// The items.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet("zui-context-menu-lift", ContextMenuLiftStyle::CSS);
    let at = SurfaceElevation::raise(OverlayLayer::Modal);
    at.publish();
    let open = state.open_signal();
    use_scroll_lock(open);
    let dismiss = UnsyncCallback::new(move |_reason: DismissReason| state.close());
    let parts = StoredValue::new_local((preview, accessory, class, attrs, children));
    let lifted = move || {
        let (preview, accessory, class, attrs, children) = parts.get_value();
        view! {
            LiftedParts(
                state = state,
                preview = preview,
                accessory = accessory,
                area = area,
                class = class,
                attrs = attrs
            ) {
                {children.view()}
            }
        }
    };

    view! {
        Portal(layer = {at.layer()}) {
            Elevated(at = at) {
                Presence(present = open, surface = {state.content()}) {
                    Scrim(class = "zui-lift__scrim")
                    DismissableLayer(
                        layer = {OverlayLayer::Modal},
                        class = "zui-overlay-layer",
                        on_dismiss = dismiss,
                        covers = true
                    ) {
                        FocusScope(options = {FocusTrapOptions::MODAL}, class = "zui-overlay-scope") {
                            {lifted()}
                        }
                    }
                }
            }
        }
    }
}

/// The preview, the accessory and the menu of a [`LiftedMenu`], placed once all three are measured.
///
/// A component of its own so that it reads the state of the enclosing presence, which runs the
/// exit.
#[component]
fn LiftedParts(
    state: OverlayState,
    preview: ContextMenuPreview,
    accessory: Option<ContextMenuAccessory>,
    area: NodeRef,
    class: Classes,
    attrs: Attrs,
    children: ChildrenFn,
) -> impl IntoView {
    provide_local_context(Lifted);
    let presence = use_presence();
    let data_state = move || {
        Some(
            presence
                .map_or("open", |presence| presence.state_name())
                .to_owned(),
        )
    };
    let source = preview.source.unwrap_or(area);
    let align = preview.align;
    let has_accessory = accessory.is_some();
    // The attribute that hides the source while the preview stands in for it.
    let lifted = AttrName::new("data-zui-lifted");
    // The space the parts are kept inside: the window less its safe area and a margin.
    let bounds = NodeRef::new();
    let preview_ref = NodeRef::new();
    let accessory_ref = NodeRef::new();
    let bounds_box = bounds.observe_border_box();
    let source_box = source.observe_border_box();
    let preview_size = preview_ref.observe_border_size();
    let accessory_size = accessory_ref.observe_border_size();
    let menu_size = state.content().observe_border_size();

    let source_css = Memo::new(move |_| Some(css_rect(source_box.get()?, bounds.scale())));
    // Made once, from the first layout that has all the parts, and kept: the parts grow out of a
    // scale on their entrance and the preview is cut to the arrangement, so a size read after the
    // first would be one the arrangement itself changed, and the parts would move as they enter.
    let arrangement = Memo::new(move |kept: Option<&Option<Arrangement>>| {
        if let Some(Some(kept)) = kept {
            return Some(*kept);
        }
        let scale = bounds.scale();
        let source = source_css.get()?;
        // An accessory that shows nothing takes no room.
        let accessory = if has_accessory {
            Some(css_size(accessory_size.get()?, scale)).filter(|size| size.height.0 > 0.0)
        } else {
            None
        };
        let parts = Parts {
            preview: css_size(preview_size.get()?, scale),
            accessory,
            menu: css_size(menu_size.get()?, scale),
        };
        // Before the first layout the parts have no size, and before the layout that applies the
        // source's width the preview has the height of a different width. An arrangement made from
        // either would place the parts and then move them.
        let unlaid = |size: Size<CssPx, Css>| size.height.0 <= 0.0;
        let unwidened = (parts.preview.width.0 - source.width().0).abs() >= 0.5;
        if unlaid(parts.menu) || unlaid(parts.preview) || unwidened {
            return None;
        }

        Some(arrange(
            source,
            parts,
            css_rect(bounds_box.get()?, scale),
            align,
        ))
    });

    // The source is hidden from the frame the preview first stands in for it until the preview
    // has gone.
    let hiding = RenderEffect::new(move |hidden: Option<bool>| {
        if hidden == Some(true) {
            return true;
        }
        let placed = arrangement.get().is_some();
        if placed {
            source.set_attribute(lifted, "");
        }
        placed
    });
    on_cleanup_local(move || {
        drop(hiding);
        source.remove_attribute(lifted);
    });

    let placed = move || arrangement.get().is_some().then(String::new);
    let visibility = move || arrangement.get().is_none().then(|| "hidden".to_owned());
    let left_of = move |of: fn(&Arrangement) -> Point<CssPx, Css>| {
        move || arrangement.get().map(|placed| px(of(&placed).x.0))
    };
    let top_of = move |of: fn(&Arrangement) -> Point<CssPx, Css>| {
        move || arrangement.get().map(|placed| px(of(&placed).y.0))
    };
    // How far the preview has moved from the region, which its entrance starts from and its exit
    // returns to.
    let from_x = move || {
        let source = source_css.get()?;
        arrangement
            .get()
            .map(|placed| px(source.left().0 - placed.preview.x.0))
    };
    let from_y = move || {
        let source = source_css.get()?;
        arrangement
            .get()
            .map(|placed| px(source.top().0 - placed.preview.y.0))
    };
    let width = move || source_css.get().map(|source| px(source.width().0));
    let clip = move || {
        arrangement
            .get()
            .and_then(|placed| placed.clip)
            .map(|clip| px(clip.0))
    };
    let menu_clip = move || {
        arrangement
            .get()
            .and_then(|placed| placed.menu_clip)
            .map(|clip| px(clip.0))
    };
    let side = move || {
        arrangement
            .get()
            .map(|placed| placed.side.name().to_owned())
    };
    let align_name = align.name();

    let accessory = accessory.map(|accessory| {
        view! {
            box(
                node_ref = accessory_ref,
                class = "zui-lift__accessory",
                attr:data-state = data_state,
                attr:data-placed = placed,
                attr:data-align = align_name,
                style:left = left_of(|placed| placed.accessory),
                style:top = top_of(|placed| placed.accessory),
                style:visibility = visibility
            ) {
                {accessory.children.view()}
            }
        }
    });
    let own = panel(state);

    view! {
        box(node_ref = bounds, class = "zui-lift__bounds", a11y:hidden = true)
        box(
            node_ref = preview_ref,
            class = "zui-lift__preview",
            attr:data-state = data_state,
            attr:data-placed = placed,
            style:left = left_of(|placed| placed.preview),
            style:top = top_of(|placed| placed.preview),
            style:width = width,
            style:max-height = clip,
            style:visibility = visibility,
            style:--zui-lift-from-x = from_x,
            style:--zui-lift-from-y = from_y,
            a11y:hidden = true
        ) {
            {preview.children.view()}
        }
        {accessory}
        OverlaySurface(
            state = state,
            role = {Role::Menu},
            modal = true,
            class = "zui-lift__menu",
            attr:data-placed = placed,
            attr:data-side = side,
            attr:data-align = align_name,
            style:left = left_of(|placed| placed.menu),
            style:top = top_of(|placed| placed.menu),
            style:max-height = menu_clip,
            style:visibility = visibility,
            {..own},
            {..attrs},
            class = class
        ) {
            MenuList {{children.view()}}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f32, top: f32, width: f32, height: f32) -> Rect<CssPx, Css> {
        Rect::new(Point::new(CssPx(left), CssPx(top)), size(width, height))
    }

    fn size(width: f32, height: f32) -> Size<CssPx, Css> {
        Size::new(CssPx(width), CssPx(height))
    }

    /// A phone's screen less its safe area.
    fn screen() -> Rect<CssPx, Css> {
        rect(8.0, 60.0, 384.0, 760.0)
    }

    fn parts() -> Parts {
        Parts {
            preview: size(200.0, 60.0),
            accessory: Some(size(300.0, 48.0)),
            menu: size(250.0, 300.0),
        }
    }

    #[test]
    fn a_region_with_room_around_it_stays_put_with_the_menu_below() {
        let placed = arrange(
            rect(20.0, 300.0, 200.0, 60.0),
            parts(),
            screen(),
            Align::Start,
        );

        assert_eq!(placed.preview, Point::new(CssPx(20.0), CssPx(300.0)));
        assert_eq!(placed.side, MenuSide::Below);
        assert_eq!(placed.accessory.y.0, 300.0 - GAP - 48.0);
        assert_eq!(placed.menu.y.0, 300.0 + 60.0 + GAP);
        assert_eq!(placed.clip, None);
    }

    #[test]
    fn a_region_near_the_bottom_keeps_its_place_and_puts_the_menu_above() {
        let placed = arrange(
            rect(20.0, 700.0, 200.0, 60.0),
            parts(),
            screen(),
            Align::Start,
        );

        assert_eq!(placed.preview.y.0, 700.0);
        assert_eq!(placed.side, MenuSide::Above);
        assert_eq!(placed.accessory.y.0, 700.0 - GAP - 48.0);
        assert_eq!(placed.menu.y.0, placed.accessory.y.0 - GAP - 300.0);
    }

    #[test]
    fn a_region_partly_off_the_top_moves_down_into_view() {
        let placed = arrange(
            rect(20.0, 10.0, 200.0, 60.0),
            parts(),
            screen(),
            Align::Start,
        );

        assert_eq!(
            placed.preview.y.0,
            60.0 + 48.0 + GAP,
            "under the accessory, inside the bounds"
        );
        assert_eq!(placed.side, MenuSide::Below);
    }

    #[test]
    fn a_region_partly_off_the_bottom_moves_up_into_view() {
        let placed = arrange(
            rect(20.0, 790.0, 200.0, 60.0),
            parts(),
            screen(),
            Align::Start,
        );

        assert_eq!(placed.preview.y.0, 820.0 - 60.0);
        assert_eq!(placed.side, MenuSide::Above);
    }

    #[test]
    fn a_region_taller_than_the_room_left_is_cut() {
        let tall = Parts {
            preview: size(200.0, 900.0),
            ..parts()
        };
        let placed = arrange(rect(20.0, 0.0, 200.0, 900.0), tall, screen(), Align::Start);

        let room = 760.0 - (48.0 + GAP) - (300.0 + GAP);
        assert_eq!(placed.clip, Some(CssPx(room)));
        assert_eq!(placed.menu_clip, None);
        assert_eq!(placed.preview.y.0, 60.0 + 48.0 + GAP);
        assert_eq!(placed.menu.y.0, placed.preview.y.0 + room + GAP);
    }

    #[test]
    fn a_menu_taller_than_the_room_left_is_cut_and_the_preview_cut_away() {
        let long = Parts {
            menu: size(250.0, 800.0),
            ..parts()
        };
        let placed = arrange(rect(20.0, 300.0, 200.0, 60.0), long, screen(), Align::Start);

        assert_eq!(placed.clip, Some(CssPx(0.0)));
        assert_eq!(placed.menu_clip, Some(CssPx(760.0 - (48.0 + GAP) - GAP)));
        assert_eq!(placed.menu.y.0, 60.0 + 48.0 + GAP + GAP);
    }

    #[test]
    fn the_accessory_and_the_menu_line_up_with_an_edge_and_stay_inside() {
        let source = rect(180.0, 300.0, 200.0, 60.0);

        let end = arrange(source, parts(), screen(), Align::End);
        assert_eq!(end.menu.x.0, 180.0 + 200.0 - 250.0);
        assert_eq!(end.accessory.x.0, 180.0 + 200.0 - 300.0);

        let start = arrange(source, parts(), screen(), Align::Start);
        assert_eq!(
            start.menu.x.0,
            392.0 - 250.0,
            "moved left to stay on screen"
        );
        assert_eq!(start.accessory.x.0, 392.0 - 300.0);
    }
}
