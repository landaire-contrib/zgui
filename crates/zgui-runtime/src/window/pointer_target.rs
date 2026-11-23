//! The element under the pointer that a platform pointer takes its shape from.
//!
//! # Which elements change the pointer
//!
//! `cursor: pointer` gives a highlight and `cursor: text` gives a beam. A document writes these
//! values to show the elements that react to a press and the text that takes a caret, so the iPad
//! pointer reads them as a desktop cursor does. `auto` and `default` give no information about the
//! element. Under `auto`, text that can be selected gets the beam, as it gets the text cursor on a
//! desktop. In other cases the role decides, and a control that a press operates gets the
//! highlight.
//!
//! A listener for a press does not decide. A scrim that closes a dialog when pressed has one, and
//! a highlight over the full window is incorrect.
//!
//! # Which element gives the area
//!
//! `cursor` is inherited, so the label inside a button computes the button's `pointer`. The
//! highlight covers the nearest control at or above the hovered element, up to the element that
//! sets `pointer`, and that element when no control is between them.

use zgui_css::values::ui::CursorKind;
use zgui_dom::{Document, NodeIndex, NodeKey};
use zgui_geom::{Corners, DevicePx, Vec2};
use zgui_platform::{PointerShape, PointerTarget, PointerTargetId};
use zgui_vocab::{Role, SemanticFlags, UiState};

use crate::window::Window;

impl Window {
    /// Gives the surface the element under the pointer, if it has changed.
    ///
    /// Returns whether the surface was told. A surface that takes no target is told nothing.
    pub(crate) fn update_pointer_target(&mut self) -> bool {
        if !self.surface.takes_pointer_target() {
            return false;
        }

        let wanted = self.wanted_pointer_target();
        if self.pointer_target == wanted {
            return false;
        }
        self.pointer_target = wanted;
        self.handle.set_pointer_target(wanted);
        true
    }

    /// The element under the pointer that changes the pointer, with its area and shape.
    fn wanted_pointer_target(&mut self) -> Option<PointerTarget> {
        let hovered = self.router.interaction().hover.target()?;
        let (element, fit) = self.pointer_element(hovered)?;

        let layout = self.layout.borrow();
        let first = *layout.boxes_of(element).first()?;
        let area = zgui_layout::fragment::transform::placed::window_box(
            &layout,
            first,
            &self.host.placements(),
        )?;
        let style = &layout.get(first)?.style;
        let shape = match fit {
            Fit::Highlight => {
                let corners = zgui_layout::fragment::clip::radii(style, area, self.scale);
                PointerShape::Highlight {
                    corner_radius: corner_radius(corners),
                }
            }
            Fit::Beam => {
                let text_style = zgui_text_style::lower::set::text_style(style);
                let line_height = self.text.strut(&text_style).line_height;
                PointerShape::Beam {
                    line_height: DevicePx(line_height.0 * self.scale),
                }
            }
        };

        Some(PointerTarget {
            id: PointerTargetId::new(element.as_u64()),
            area,
            shape,
        })
    }

    /// The element that gives the area for a pointer over `hovered`, and how the pointer fits it.
    ///
    /// `None` when the pointer keeps its shape there.
    fn pointer_element(&self, hovered: NodeKey) -> Option<(NodeKey, Fit)> {
        let document = self.document.borrow();
        let index = document.store().index_of(hovered)?;
        let key = |index: NodeIndex| document.store().key_of(index);

        match cursor_of(&document, index)? {
            CursorKind::Pointer => {
                let origin = origin_of(&document, index, CursorKind::Pointer);
                match control_at(&document, index, Some(origin)) {
                    Control::Operable(control) => Some((key(control), Fit::Highlight)),
                    Control::Disabled => None,
                    Control::Absent => Some((key(origin), Fit::Highlight)),
                }
            }
            CursorKind::Text => {
                let element = match self.selectable_text_at(Some(hovered)) {
                    Some(text) => text,
                    None => key(origin_of(&document, index, CursorKind::Text)),
                };
                Some((element, Fit::Beam))
            }
            CursorKind::Auto => {
                if let Some(text) = self.selectable_text_at(Some(hovered)) {
                    return Some((text, Fit::Beam));
                }
                match control_at(&document, index, None) {
                    Control::Operable(control) => Some((key(control), Fit::Highlight)),
                    Control::Disabled | Control::Absent => None,
                }
            }
            CursorKind::Default => match control_at(&document, index, None) {
                Control::Operable(control) => Some((key(control), Fit::Highlight)),
                Control::Disabled | Control::Absent => None,
            },
            _ => None,
        }
    }
}

/// How the pointer fits an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fit {
    /// A highlight on the area of the element.
    Highlight,
    /// A text beam.
    Beam,
}

/// The nearest control at or above an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    /// A control that a press operates.
    Operable(NodeIndex),
    /// A disabled control, which a press does not operate.
    Disabled,
    /// No control.
    Absent,
}

/// The `cursor` keyword an element computes.
fn cursor_of(document: &Document, index: NodeIndex) -> Option<CursorKind> {
    let style = document.node(index).primary_style()?;
    Some(style.get_inherited_ui().cursor.keyword)
}

/// The highest element at or above `index` that computes `keyword` with every element between.
///
/// `cursor` is inherited, so this is the element that sets the keyword.
fn origin_of(document: &Document, index: NodeIndex, keyword: CursorKind) -> NodeIndex {
    let mut origin = index;
    while let Some(parent) = document.store().core(origin).parent()
        && cursor_of(document, parent) == Some(keyword)
    {
        origin = parent;
    }

    origin
}

/// The nearest control at or above `index`, up to and including `last`, or up to the root.
fn control_at(document: &Document, index: NodeIndex, last: Option<NodeIndex>) -> Control {
    let mut current = index;
    loop {
        if let Some(semantics) = document
            .store()
            .columns()
            .semantics
            .get(document.store().key_of(current))
            .and_then(|slot| slot.as_deref())
            && is_control(semantics.role)
        {
            let disabled = semantics.flags.contains(SemanticFlags::DISABLED)
                || document
                    .store()
                    .core(current)
                    .ui_state()
                    .contains(UiState::DISABLED);
            return if disabled {
                Control::Disabled
            } else {
                Control::Operable(current)
            };
        }
        if last == Some(current) {
            return Control::Absent;
        }
        let Some(parent) = document.store().core(current).parent() else {
            return Control::Absent;
        };
        current = parent;
    }
}

/// Whether an element of `role` is a control that a press operates.
const fn is_control(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::DefaultButton
            | Role::Link
            | Role::MenuItem
            | Role::MenuItemCheckBox
            | Role::MenuItemRadio
            | Role::CheckBox
            | Role::RadioButton
            | Role::Switch
            | Role::Tab
            | Role::ListBoxOption
            | Role::MenuListOption
            | Role::TreeItem
            | Role::ComboBox
            | Role::DisclosureTriangle
    )
}

/// One corner radius for the four corners of an element.
///
/// A platform pointer has one radius. The largest corner gives it, and a corner whose two radii
/// differ gives the smaller of them.
fn corner_radius(corners: Corners<Vec2<DevicePx>>) -> DevicePx {
    let corner = |radius: Vec2<DevicePx>| radius.x.0.min(radius.y.0);

    DevicePx(
        corner(corners.top_left)
            .max(corner(corners.top_right))
            .max(corner(corners.bottom_right))
            .max(corner(corners.bottom_left)),
    )
}
