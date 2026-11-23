//! A flex item with an `aspect-ratio` takes its automatic size from its other size after
//! `min-*` and `max-*` clamp that size.
//!
//! A 300px wide 3:2 box in a 200px column is 200px wide, so it is 133px high. The column and the
//! next box use that height.

mod support;

use support::find::rect;
use support::{Element, Fixture, lay_out, measurer};

#[test]
fn a_max_width_sets_the_height_of_a_column_item() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("box")
                .classes(&["column"])
                .children(vec![Element::new("box").classes(&["item"])]),
            Element::new("box").classes(&["next"]),
        ]),
        "root { display: block; width: 200px }
         .column { display: flex; flex-direction: column }
         .item { width: 300px; max-width: 100%; aspect-ratio: 3 / 2 }
         .next { height: 10px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 800.0, 600.0);

    let (_, _, width, height) = rect(&store, &fixture, "item");
    assert_eq!((width, height), (200.0, 133.0));
    assert_eq!(
        rect(&store, &fixture, "column").3,
        133.0,
        "the column holds the item at its clamped height"
    );
    assert_eq!(
        rect(&store, &fixture, "next").1,
        133.0,
        "the next box starts below the item"
    );
}
