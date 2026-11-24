//! Scrolling the focused field above the on-screen keyboard.
//!
//! When a field gets focus, when the keyboard area changes, and when the caret moves to another
//! line, the scroll containers around the field move so that the caret and a margin around it are
//! above the keyboard. A container that already shows them does not move. The scroll moves with
//! the duration and curve of the keyboard when the platform reports them.
//!
//! The measurement leaves out every transform. A surface that a transform moves while the keyboard
//! opens, for example a bottom sheet that goes to its large detent, is measured where it stops, so
//! its content scrolls only by what the keyboard covers at that position.
//!
//! The field's own scroll is the work of [`reveal`](crate::window::reveal).

use zgui_css::values::size::PositionValue;
use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_layout::FragmentKind;
use zgui_scene::{Anchoring, SpatialId, SpatialTree};
use zgui_scroll::Pace;

use crate::window::Window;

/// The space kept between the caret and the keyboard, in CSS pixels.
const MARGIN: f32 = 16.0;

/// The field, caret line and keyboard area that the last check measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AboveKeyboard {
    /// The focused field.
    node: NodeKey,
    /// The line the caret is on.
    line: usize,
    /// The part of the surface the keyboard covers.
    keyboard: Rect<DevicePx, Device>,
}

/// One scroll the check asks for.
struct Move {
    /// The container to scroll.
    container: NodeKey,
    /// Its new offset.
    to: Point<DevicePx, Device>,
}

impl Window {
    /// Scrolls the containers around the focused field so that its caret is above the keyboard, if
    /// the field, the caret line or the keyboard area changed.
    ///
    /// Call it after the fragment pass, which this frame's measurement reads.
    pub(crate) fn keep_caret_above_keyboard(&mut self) {
        let keyboard = self.virtual_keyboard.filter(|area| !area.is_empty());
        let focused = self.router.interaction().focus.focused();
        let (Some(keyboard), Some(node)) = (keyboard, focused) else {
            self.above_keyboard = None;
            return;
        };
        let Some((line, caret)) = self.caret_in_window(node) else {
            return;
        };
        let key = AboveKeyboard {
            node,
            line,
            keyboard,
        };
        if self.above_keyboard == Some(key) {
            return;
        }
        // A selecting drag scrolls at the speed of the pointer. The check runs when it ends, and
        // a touch that focuses a field starts one.
        if self.selecting.is_some() {
            return;
        }
        self.above_keyboard = Some(key);

        let pace = self
            .viewport
            .keyboard_motion
            .map_or(Pace::Standard, Pace::Keyboard);
        for Move { container, to } in self.moves_above(node, caret, keyboard) {
            self.travel(container, to, pace);
        }
    }

    /// The scrolls that put `caret` above `keyboard`, innermost container first.
    ///
    /// Each container is measured where its running smooth scroll and those of the containers
    /// around it will stop.
    fn moves_above(
        &self,
        node: NodeKey,
        caret: Rect<DevicePx, Device>,
        keyboard: Rect<DevicePx, Device>,
    ) -> Vec<Move> {
        let containers = self.containers_moving(node);
        let layout = self.layout.borrow();
        let scroller = self.scroll.borrow();
        let pending: Vec<f32> = containers
            .iter()
            .map(|container| scroller.heading(*container).y.0 - scroller.offset_of(*container).y.0)
            .collect();
        let margin = MARGIN * self.scale;
        let mut outside: f32 = pending.iter().sum();
        let mut top = caret.top().0 - margin - outside;
        let mut bottom = caret.bottom().0 + margin - outside;

        let mut moves = Vec::new();
        for (container, pending) in containers.into_iter().zip(pending) {
            outside -= pending;
            let Some(port) = self.port_in_window(container) else {
                continue;
            };
            let port_top = port.top().0 - outside;
            let mut port_bottom = port.bottom().0 - outside;
            let beside = keyboard.right() <= port.left() || keyboard.left() >= port.right();
            if !beside {
                port_bottom = port_bottom.min(keyboard.top().0);
            }
            // A port that the keyboard covers entirely shows nothing. A container around it can.
            if port_bottom <= port_top {
                continue;
            }
            let by = if top < port_top {
                top - port_top
            } else if bottom > port_bottom {
                (bottom - port_bottom).min(top - port_top)
            } else {
                0.0
            };
            if by == 0.0 {
                continue;
            }
            let Some(region) =
                zgui_layout::scroll_region::code_region_of_element(&layout, container)
            else {
                continue;
            };
            let heading = scroller.heading(container);
            let to = (heading.y.0 + by).clamp(0.0, region.limit().y.0);
            let moved = to - heading.y.0;
            if moved == 0.0 {
                continue;
            }
            top -= moved;
            bottom -= moved;
            moves.push(Move {
                container,
                to: Point::new(heading.x, DevicePx(to)),
            });
        }
        moves
    }

    /// The scroll containers whose scroll moves `node`, innermost first, without `node` itself.
    ///
    /// The walk stops at a `position: fixed` element, which no scroll above it moves.
    fn containers_moving(&self, node: NodeKey) -> Vec<NodeKey> {
        let chain = self.scroll_chain(node);
        let document = self.document.borrow();
        let layout = self.layout.borrow();
        let store = document.store();
        let mut containers = Vec::new();
        let mut next = store.index_of(node);
        while let Some(index) = next {
            let key = store.core(index).key();
            if key != node && chain.contains(&key) {
                containers.push(key);
            }
            let fixed = layout.boxes_of(key).first().is_some_and(|box_| {
                layout
                    .get(*box_)
                    .is_some_and(|laid| laid.style.get_box().position == PositionValue::Fixed)
            });
            if fixed {
                break;
            }
            next = store.core(index).parent();
        }
        containers
    }

    /// The caret line of a focused field and the caret, in the window without transforms.
    fn caret_in_window(&self, node: NodeKey) -> Option<(usize, Rect<DevicePx, Device>)> {
        let selection = self.editors.shown_selection(node)?;
        let layout = self.layout.borrow();
        let located = crate::caret::Located::of(&layout, &self.text, node)?;
        let box_ = crate::caret::place::text_box(&layout, node)?;
        let caret = located.caret(selection.focus, selection.affinity)?;
        let line_origin = located.line_origin(caret.line)?;
        let fragment = layout
            .fragments_of_box(box_)
            .iter()
            .filter_map(|frag| layout.fragment(*frag))
            .find(|fragment| match fragment.kind {
                FragmentKind::Line { paragraph, line } => {
                    paragraph == located.paragraph && usize::from(line) == caret.line
                }
                _ => false,
            })?;
        let origin = fragment.border_box.origin;
        let rect = Rect::new(
            Point::new(
                DevicePx(origin.x.0 + caret.origin.x.0 - line_origin.x.0),
                DevicePx(origin.y.0 + caret.origin.y.0 - line_origin.y.0),
            ),
            Size::new(
                DevicePx(crate::caret::caret_width(self.scale)),
                DevicePx(caret.height.0),
            ),
        );
        let rect = untransformed(&self.scene.spatial, rect, fragment.transform)?;
        Some((caret.line, rect))
    }

    /// A container's scrollport, its padding box, in the window without transforms.
    fn port_in_window(&self, container: NodeKey) -> Option<Rect<DevicePx, Device>> {
        let layout = self.layout.borrow();
        let first = *layout.boxes_of(container).first()?;
        let fragment = layout.fragment(*layout.fragments_of_box(first).first()?)?;
        untransformed(
            &self.scene.spatial,
            fragment.padding_box(),
            fragment.transform,
        )
    }

    /// Moves one container to `to` over the next frames, at `pace`.
    fn travel(&mut self, container: NodeKey, to: Point<DevicePx, Device>, pace: Pace) {
        let heading = {
            let layout = self.layout.borrow();
            self.scroll
                .borrow_mut()
                .travel_to(&layout, container, to, pace)
        };
        if heading.is_some() && self.is_scrolling() {
            self.mark_scrolled(&[container]);
        }
    }
}

/// `rect`, measured in `space`, in the window with the scroll offsets above it and without the
/// transforms above it.
///
/// A scroll offset is the transform of a content system. The document system moves the document
/// in the poison test mode. Every other system is left out. Nothing when a system on the way is no
/// longer live.
fn untransformed(
    spatial: &SpatialTree,
    rect: Rect<DevicePx, Device>,
    space: Option<SpatialId>,
) -> Option<Rect<DevicePx, Device>> {
    let document = spatial.document();
    let (mut x, mut y) = (0.0, 0.0);
    let mut next = space;
    while let Some(id) = next {
        let node = spatial.get(id)?;
        if node.anchoring == Anchoring::Content || id == document {
            let translation = node.local.columns[3];
            x += translation[0];
            y += translation[1];
        }
        next = node.parent;
    }
    Some(rect.translate(Size::new(DevicePx(x), DevicePx(y))))
}
