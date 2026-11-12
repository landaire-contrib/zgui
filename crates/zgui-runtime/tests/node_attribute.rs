//! An attribute set through a `NodeRef` on the window root, matched by a descendant rule.
//!
//! A theme root writes a preference such as `[reduced-motion]` on the document root, which it does
//! not build. The change goes through the document's attribute path, so only the elements that a
//! rule naming the attribute selects are restyled.

mod support;

use zgui_platform_headless::Harness;
use zgui_profile::counter::Counters;
use zgui_reactive::prelude::{Get, Set};
use zgui_reactive::{RenderEffect, RwSignal, on_cleanup_local};
use zgui_runtime::Runtime;
use zgui_view::{AttrName, BuildCx, IntoView, NodeRef, View};

const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   .page { display: block }
                   .x { display: block; width: 10px; height: 10px }
                   .other { display: block; width: 10px; height: 1px }
                   [reduced-motion] .x { width: 120px }";

/// The elements beside the one the rule selects.
const OTHERS: usize = 200;

/// A page of one `.x` element and [`OTHERS`] more, with one root attribute per signal.
///
/// Each signal sets or removes its attribute on the window root from an effect, which is how a
/// theme root writes a preference.
fn page(attributes: Vec<(&'static str, RwSignal<bool>)>) -> (Harness<Runtime>, NodeRef) {
    let target = NodeRef::new();
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        for (name, present) in attributes.clone() {
            let writer = RenderEffect::new(move |_| {
                let present = present.get();
                // Read tracked, because the window root is reachable only after the target binds.
                if target.get().is_none() {
                    return;
                }
                let Some(root) = target.window_root() else {
                    return;
                };
                let name = AttrName::new(name);
                // Spelled as a path call: the `mutation` ledger check reads a method call of
                // this name as a direct document write, and this call goes through the view's
                // DOM, which edits the document in a batch.
                if present {
                    NodeRef::set_attribute(&root, name, "");
                } else {
                    NodeRef::remove_attribute(&root, name);
                }
            });
            on_cleanup_local(move || drop(writer));
        }
        let mut page = zgui_elements::column()
            .class("page")
            .child(zgui_elements::column().class("x").node_ref(target));
        for _ in 0..OTHERS {
            page = page.child(zgui_elements::column().class("other"));
        }
        Box::new(page.into_view().build(cx))
    });
    harness.settle(8);

    (harness, target)
}

/// Writes `value` to `signal`, settles, and returns what the frames counted.
fn write(harness: &mut Harness<Runtime>, signal: RwSignal<bool>, value: bool) -> Counters {
    let before = zgui_profile::counter::snapshot();
    signal.set(value);
    harness.settle(8);

    before.delta(&zgui_profile::counter::snapshot())
}

/// The width of the element `handle` names, as of the last frame.
fn width_of(handle: NodeRef) -> f32 {
    handle
        .bounds()
        .expect("the element was laid out")
        .size
        .width
        .0
}

#[test]
fn an_attribute_on_the_window_root_restyles_only_the_elements_its_rules_select() {
    let _turn = zgui_profile::counter::exclusive();
    let reduced = RwSignal::new(false);
    let (mut harness, target) = page(vec![("reduced-motion", reduced)]);
    assert_eq!(width_of(target), 10.0);

    let set = write(&mut harness, reduced, true);
    assert_eq!(
        width_of(target),
        120.0,
        "the rule under the root attribute did not apply"
    );

    let removed = write(&mut harness, reduced, false);
    assert_eq!(
        width_of(target),
        10.0,
        "the rule under the root attribute still applies after removal"
    );

    if zgui_profile::COUNTERS_ENABLED {
        for (change, counted) in [("set", set), ("removal", removed)] {
            assert_eq!(
                counted.elements_restyled, 1,
                "the {change} restyled elements other than the one the rule selects"
            );
            assert_eq!(
                counted.elements_recascaded, 0,
                "the {change} recascaded elements"
            );
        }
    }
    harness.shut_down();
}

#[test]
fn an_attribute_no_rule_names_restyles_nothing() {
    let _turn = zgui_profile::counter::exclusive();
    let unread = RwSignal::new(false);
    let (mut harness, target) = page(vec![("data-unread", unread)]);

    let set = write(&mut harness, unread, true);

    assert_eq!(width_of(target), 10.0);
    if zgui_profile::COUNTERS_ENABLED {
        // The style pass visits the root for its attribute change, and no selector depends on the
        // attribute, so the visit goes no deeper and restyles nothing.
        assert_eq!(set.elements_traversed, 1);
        assert_eq!(set.elements_restyled, 0);
        assert_eq!(set.elements_recascaded, 0);
    }
    harness.shut_down();
}
