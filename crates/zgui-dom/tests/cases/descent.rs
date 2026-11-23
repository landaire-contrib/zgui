//! Does a second mark, with no phase walk between it and the first, still get descended into?
//!
//! This is the one property nothing had run before. Marking returns at its *first* early-out — the
//! node's own invalidation word — before it touches any ancestor, so a second mark of a node whose
//! own bits have not yet been retired restores nothing anywhere. Whether that matters depends
//! entirely on what the traversal descends by for a mark: if it were a flag of the engine's,
//! cleared on the engine's schedule, the second traversal would stop above the node and the change
//! would be lost silently. The traversal descends by the invalidation word for a mark, so the union
//! is still there and the traversal still descends.
//!
//! The hazard and the ancestor marking have each been shown on their own, the first against a
//! stand-in flag and the second against a traversal that never re-marked. Neither exercised both at
//! once against a real traversal. This does.
//!
//! The last case is the same question from the other side. The engine raises a descent flag of its
//! own on the *sibling* whose descendant an invalidation reached, a node that no mark leads to and
//! that a walk of the marked set never visits. That flag keeps out of the invalidation word, so a
//! later mark of that node or of a node below it still reaches the ancestors.

use zgui_bits::Dirty;
use zgui_dom::{Document, NodeIndex, NodeKind};
use zgui_interned::{ClassName, ElementName};

use crate::support::edit;
use crate::support::engine::Engine;
use crate::support::fixture;
use crate::support::read::color;

/// Whether `pass` visited the element at `index`.
fn visited(pass: &crate::support::engine::Pass, index: NodeIndex) -> bool {
    pass.visited.contains(&index.get())
}

#[test]
fn a_repeat_mark_across_a_real_traversal_still_descends() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        ".item { color: rgb(1, 1, 1) }
         .item.one { color: rgb(2, 0, 0) }
         .item.two { color: rgb(3, 0, 0) }",
    );
    engine.restyle(&mut tree.document, None);
    edit::retire(&mut tree.document);

    let target = tree.at("i1");

    edit::set_classes(
        &tree.document,
        target,
        &[ClassName::new("item"), ClassName::new("one")],
    );
    let first = engine.restyle(&mut tree.document, None);
    assert!(visited(&first, target), "the first traversal reaches it");
    assert_eq!(color(&tree.document, target), (2, 0, 0));

    // No retirement here. This is the whole point: the node's own invalidation word still carries
    // the obligation from the first mark, so the second mark returns immediately and restores no
    // ancestor state at all.
    edit::set_classes(
        &tree.document,
        target,
        &[ClassName::new("item"), ClassName::new("two")],
    );
    let second = engine.restyle(&mut tree.document, None);

    assert!(
        visited(&second, target),
        "the second traversal has to reach it too; a descent flag retired on a schedule of its own \
         would have been cleared by the first traversal and never restored"
    );
    assert_eq!(
        color(&tree.document, target),
        (3, 0, 0),
        "and the second change has to actually take effect"
    );
}

#[test]
fn retiring_between_the_two_marks_changes_nothing_about_the_answer() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        ".item { color: rgb(1, 1, 1) }
         .item.one { color: rgb(2, 0, 0) }
         .item.two { color: rgb(3, 0, 0) }",
    );
    engine.restyle(&mut tree.document, None);
    edit::retire(&mut tree.document);

    let target = tree.at("i1");
    edit::set_classes(
        &tree.document,
        target,
        &[ClassName::new("item"), ClassName::new("one")],
    );
    engine.restyle(&mut tree.document, None);
    edit::retire(&mut tree.document);

    edit::set_classes(
        &tree.document,
        target,
        &[ClassName::new("item"), ClassName::new("two")],
    );
    let second = engine.restyle(&mut tree.document, None);
    assert!(visited(&second, target));
    assert_eq!(color(&tree.document, target), (3, 0, 0));
}

/// Retires the invalidation the way a phase walk does: only at the nodes it descends to.
///
/// A walk starts at the document node and descends by the dirty-child records, so a node whose
/// subtree union was raised by something other than a mark is never visited and never retired.
fn walk_retire(document: &Document) {
    fn visit(document: &Document, index: NodeIndex) {
        let record = document.store().core(index);
        let children: Vec<NodeIndex> = record
            .dirty_children()
            .iter(document.store(), index)
            .collect();
        for child in children {
            visit(document, child);
        }
        record.dirty().clear_own(Dirty::all());
        record.dirty().retire_phase(Dirty::all(), Dirty::empty());
        record.dirty_children().clear();
    }
    visit(document, document.document_index());
}

/// `root > (x, y > z)`, the smallest shape in which an invalidation crosses to a sibling's subtree.
struct Sideways {
    /// The document.
    document: Document,
    /// The element whose class change starts the invalidation.
    x: NodeIndex,
    /// Its next sibling, which the engine raises its own descent flag on.
    y: NodeIndex,
    /// The element the combinator actually reaches.
    z: NodeIndex,
}

/// Builds the sideways shape.
fn sideways() -> Sideways {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let x = document.append(root, NodeKind::Element, ElementName::new("box"));
    let y = document.append(root, NodeKind::Element, ElementName::new("box"));
    document.set_classes(y, &[ClassName::new("b")]);
    let z = document.append(y, NodeKind::Element, ElementName::new("box"));
    document.set_classes(z, &[ClassName::new("c")]);
    Sideways { document, x, y, z }
}

#[test]
fn a_mark_on_or_below_a_node_the_engine_descended_through_reaches_the_root() {
    let mut shape = sideways();
    let mut engine = Engine::new(&shape.document);
    engine.add_author_sheet(
        ".c { color: rgb(1, 1, 1) }
         .a + .b .c { color: rgb(9, 0, 0) }
         .b.q { color: rgb(5, 0, 0) }
         .c.r.r.r { color: rgb(7, 0, 0) }",
    );
    engine.restyle(&mut shape.document, None);
    walk_retire(&shape.document);
    assert_eq!(color(&shape.document, shape.z), (1, 1, 1));

    // A class change on `x` invalidates its *sibling's* descendant through the combinator, so the
    // engine raises its own descent flag on `y` — a node the marked set never leads to.
    edit::set_classes(&shape.document, shape.x, &[ClassName::new("a")]);
    engine.restyle(&mut shape.document, None);
    assert_eq!(
        color(&shape.document, shape.z),
        (9, 0, 0),
        "the combinator applied, which is what makes the engine descend into `y` at all"
    );
    walk_retire(&shape.document);
    assert_eq!(
        shape.document.store().core(shape.y).dirty().subtree(),
        Dirty::empty(),
        "the walk never visits `y`, so a union raised on it would stay raised"
    );

    // Now `z` changes. The mark climbs through `y` to the root.
    edit::set_classes(
        &shape.document,
        shape.z,
        &[ClassName::new("c"), ClassName::new("r")],
    );
    let pass = engine.restyle(&mut shape.document, None);
    assert!(
        pass.traversed,
        "the mark on `z` has to reach the root, or nothing is traversed at all"
    );
    assert_eq!(
        color(&shape.document, shape.z),
        (7, 0, 0),
        "and the failure is a silently stale colour, not an error"
    );
    walk_retire(&shape.document);

    // And `y` itself.
    edit::set_classes(
        &shape.document,
        shape.y,
        &[ClassName::new("b"), ClassName::new("q")],
    );
    let pass = engine.restyle(&mut shape.document, None);
    assert!(pass.traversed, "the mark on `y` reaches the root as well");
    assert_eq!(color(&shape.document, shape.y), (5, 0, 0));
}
