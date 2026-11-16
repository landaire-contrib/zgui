//! One thing an element was told, and how it is written.

use core::cell::RefCell;
use std::rc::Rc;

use zgui_interned::{AttrName, ClassName, CustomPropertyName, Ident};
use zgui_view::binding::{
    bind_class, bind_custom_property, bind_custom_state, bind_property, bind_style_property,
    bind_ui_state,
};
use zgui_view::{
    AttrEntry, Binding, BuildCx, Classes, DomHandle, EventCx, EventKind, ListenerOptions,
    ListenerRegistration, NodeId, NodeRef, PropKey, PropValue, ReactiveValue, UiState,
};

/// One thing an element was told to be, in the order it was written.
///
/// A whole class list and an accessibility description are not here: those two *accumulate* rather
/// than happening in sequence, because a component's classes and a caller's classes have to merge
/// into one list rather than overwrite one another. Everything else applies in order, which is what
/// makes two toggles of the same class resolve caller-last.
#[allow(
    clippy::enum_variant_names,
    reason = "three of the ten really are an attribute, a state and a property of the same shape; \
               renaming them to satisfy the lint would rename them away from what they are"
)]
pub(crate) enum Attribute {
    /// One class, toggled.
    ClassToggle(ClassName, ReactiveValue<bool>),
    /// The whole inline style text.
    StyleText(ReactiveValue<Option<String>>),
    /// One inline style declaration.
    StyleProperty(String, ReactiveValue<Option<String>>),
    /// One custom property.
    CustomProperty(CustomPropertyName, ReactiveValue<Option<String>>),
    /// One attribute, which selectors can see.
    Attribute(AttrName, ReactiveValue<Option<String>>),
    /// One interaction state a view is allowed to assert.
    State(UiState, ReactiveValue<bool>),
    /// One author-defined state, matched by `:state(name)`.
    CustomState(Ident, ReactiveValue<bool>),
    /// One imperative property, which selectors cannot see.
    Property(PropKey, ReactiveValue<PropValue>),
    /// One listener.
    Listener(EventKind, ListenerOptions, Rc<dyn Fn(&mut EventCx<'_>)>),
    /// A handle to fill in with the element once it exists.
    NodeRef(NodeRef),
}

impl Attribute {
    /// Applies this to `el`, returning whatever has to be kept alive for it to keep applying.
    ///
    /// A listener's registration goes into `listeners` rather than coming back as a binding: it is
    /// not kept alive by being held, it is *removed* by being held, and the element has to take
    /// its previous listeners off before it registers this description's. A handle goes into
    /// `handles`, so that the element can release it when its view goes away. What this writes
    /// goes into `written`, so that the next description can remove it.
    pub(crate) fn apply(
        self,
        cx: &BuildCx<'_>,
        el: NodeId,
        listeners: &mut Vec<ListenerRegistration>,
        handles: &mut Vec<NodeRef>,
        written: &mut Vec<Written>,
    ) -> Option<Binding> {
        match self {
            Self::ClassToggle(class, on) => {
                written.push(Written::ClassToggle(class));
                Some(bind_class(cx, el, class, on))
            }
            Self::StyleText(css) => {
                written.push(Written::StyleText);
                let dom = cx.dom_handle();
                Some(Binding::new(cx, css, move |css: &Option<String>| {
                    dom.set_style_text(el, css.as_deref());
                }))
            }
            Self::StyleProperty(property, value) => {
                written.push(Written::StyleProperty(Ident::new(&property)));
                Some(bind_style_property(cx, el, property, value))
            }
            Self::CustomProperty(property, value) => {
                written.push(Written::CustomProperty(property));
                Some(bind_custom_property(cx, el, property, value))
            }
            Self::Attribute(name, ReactiveValue::Constant(value)) => {
                cx.dom().set_attribute(el, name, value.as_deref());
                written.push(Written::Attribute(name, LastValue::Constant(value)));
                None
            }
            Self::Attribute(name, ReactiveValue::Dynamic(compute)) => {
                let last = Rc::new(RefCell::new(None));
                let dom = cx.dom_handle();
                let binding = Binding::recorded(
                    cx,
                    move || compute(),
                    Rc::clone(&last),
                    move |value: &Option<String>| dom.set_attribute(el, name, value.as_deref()),
                );
                written.push(Written::Attribute(name, LastValue::Dynamic(last)));
                Some(binding)
            }
            Self::State(state, on) => {
                written.push(Written::State(state));
                Some(bind_ui_state(cx, el, state, on))
            }
            Self::CustomState(name, on) => {
                written.push(Written::CustomState(name));
                Some(bind_custom_state(cx, el, name, on))
            }
            Self::Property(key, value) => {
                written.push(Written::Property(key));
                Some(bind_property(cx, el, key, value))
            }
            Self::Listener(event, options, handler) => {
                listeners.push(ListenerRegistration::erased(
                    &**cx.dom(),
                    el,
                    event,
                    options,
                    handler,
                ));
                None
            }
            Self::NodeRef(handle) => {
                handle.bind(el, cx.dom(), cx.host());
                handles.push(handle);
                None
            }
        }
    }

    /// Whether this writes what `written` names, whatever the value.
    pub(crate) fn rewrites(&self, written: &Written) -> bool {
        match (self, written) {
            (Self::ClassToggle(class, _), Written::ClassToggle(other)) => class == other,
            (Self::StyleText(_), Written::StyleText) => true,
            (Self::StyleProperty(property, _), Written::StyleProperty(other)) => {
                property == other.as_str()
            }
            (Self::CustomProperty(property, _), Written::CustomProperty(other)) => {
                property == other
            }
            (Self::Attribute(name, _), Written::Attribute(other, _)) => name == other,
            (Self::State(state, _), Written::State(other)) => state == other,
            (Self::CustomState(name, _), Written::CustomState(other)) => name == other,
            (Self::Property(key, _), Written::Property(other)) => key == other,
            _ => false,
        }
    }
}

/// One thing a description wrote onto its element.
///
/// Kept so that the next description of the same element can remove what it does not write again.
/// An attribute keeps the value it was written with, because [`NodeRef::set_attribute`] and other
/// writers put attributes on a view's own elements too, and their values are not the description's
/// to remove. Nothing but the element's description writes the other kinds on it.
pub(crate) enum Written {
    /// The whole class list.
    Classes,
    /// One toggled class.
    ClassToggle(ClassName),
    /// The whole inline style text.
    StyleText,
    /// One inline style declaration.
    StyleProperty(Ident),
    /// One custom property.
    CustomProperty(CustomPropertyName),
    /// One attribute, and the value last written to it.
    Attribute(AttrName, LastValue),
    /// One interaction state.
    State(UiState),
    /// One author-defined state.
    CustomState(Ident),
    /// One imperative property.
    Property(PropKey),
    /// The accessibility description.
    Semantics,
}

impl Written {
    /// Removes this from `el`. An attribute is left alone when another writer has replaced the
    /// value since.
    pub(crate) fn remove(&self, dom: &DomHandle, el: NodeId) {
        match self {
            Self::Classes => dom.set_classes(el, &[]),
            Self::ClassToggle(class) => dom.toggle_class(el, *class, false),
            Self::StyleText => dom.set_style_text(el, None),
            Self::StyleProperty(property) => dom.set_style_property(el, property.as_str(), None),
            Self::CustomProperty(property) => dom.set_custom_property(el, *property, None),
            Self::Attribute(name, last) => last.remove(dom, el, *name),
            Self::State(state) => dom.set_ui_state(el, *state, false),
            Self::CustomState(name) => dom.set_custom_state(el, *name, false),
            Self::Property(key) => dom.set_property(el, *key, PropValue::Unset),
            Self::Semantics => dom.set_semantics(el, None),
        }
    }
}

/// The value a description last wrote to one attribute.
pub(crate) enum LastValue {
    /// A literal.
    Constant(Option<String>),
    /// A computed value, kept up to date by the binding that writes it.
    Dynamic(Rc<RefCell<Option<Option<String>>>>),
}

impl LastValue {
    /// Removes `name` from `el` while it still holds the value last written to it.
    fn remove(&self, dom: &DomHandle, el: NodeId, name: AttrName) {
        let dynamic;
        let value = match self {
            Self::Constant(value) => value.as_deref(),
            Self::Dynamic(last) => {
                dynamic = last.borrow();
                dynamic.as_ref().and_then(Option::as_deref)
            }
        };
        // The description last removed it, so nothing on the element is the description's.
        let Some(value) = value else {
            return;
        };

        if dom.attribute_is(el, name, value) {
            dom.set_attribute(el, name, None);
        }
    }
}

/// Replays a bundle a caller forwarded, appending its entries after whatever is already there.
///
/// This is the consumer side of `{..attrs}`: a component that takes a bundle and spreads it onto
/// its root element gets the caller's classes merged after its own, the caller's accessibility
/// description merged over its own, and the caller's toggles, attributes and listeners applied
/// after its own — which is what makes `<Button class="w-full" on:click=…/>` mean what it reads as.
pub(crate) fn replay(
    entries: Vec<AttrEntry>,
    forwarded: Classes,
    into: &mut Vec<Attribute>,
    classes: &mut Classes,
) {
    *classes = classes.merged(&forwarded);
    for entry in entries {
        into.push(match entry {
            AttrEntry::ClassToggle(class, on) => Attribute::ClassToggle(class, on),
            AttrEntry::StyleProperty(property, value) => Attribute::StyleProperty(property, value),
            AttrEntry::CustomProperty(property, value) => {
                Attribute::CustomProperty(property, value)
            }
            AttrEntry::Attribute(name, value) => Attribute::Attribute(name, value),
            AttrEntry::StateToggle(state, on) => Attribute::State(state, on),
            AttrEntry::CustomStateToggle(name, on) => Attribute::CustomState(name, on),
            AttrEntry::Property(key, value) => Attribute::Property(key, value),
            AttrEntry::Listener(event, options, handler) => {
                Attribute::Listener(event, options, handler)
            }
        });
    }
}
