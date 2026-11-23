//! Content that reaches past a scroll container's scrollport, and the width the container scrolls.
//!
//! The white space at the end of a wrapped line hangs and takes no room. The white space of a line
//! that may not wrap keeps its room, and content wider than the scrollport scrolls.

mod support;

use support::{Element, Fixture, lay_out, measurer};
use zgui_layout::scroll_region::{ScrollRegion, region_of};

/// One character's advance at the initial font size, in the fixed face.
const ADVANCE: f32 = 8.0;

/// The width of the scrollport: a 402 px page, less 16 px of padding each side and a 15 px gutter.
const PORT: f32 = 355.0;

/// The page: a vertical scroll container 402 px wide, taller content, and `paragraph` styled with
/// `css`.
fn page(paragraph: Element, css: &str) -> ScrollRegion {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("tall"), paragraph]),
        &format!(
            "root {{ display: block; width: 402px; height: 300px; padding: 16px; overflow-y: auto }}
             tall {{ display: block; height: 600px }}
             {css}"
        ),
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 800.0, 600.0);
    let root = store.root().expect("a root");
    let region = region_of(&store, root).expect("the page scrolls");
    assert_eq!(
        region.scrollport.size.width.0, PORT,
        "the page reserves its gutter"
    );
    region
}

#[test]
fn the_spaces_at_the_end_of_a_preformatted_line_scroll_its_container() {
    const TEXT: &str = "0123456789abcdef0123456789abcdef0123456789ab  ";
    let region = page(
        Element::new("text").text(TEXT),
        "text { display: block; white-space: pre }",
    );

    assert_eq!(
        region.content.width.0,
        TEXT.len() as f32 * ADVANCE,
        "a line that may not wrap keeps the room of its spaces"
    );
    assert_eq!(region.limit().x.0, TEXT.len() as f32 * ADVANCE - PORT);
}

#[test]
fn a_line_wider_than_the_scrollport_scrolls_its_container() {
    const TEXT: &str = "0123456789abcdef0123456789abcdef0123456789abcdef no wrap";
    let region = page(
        Element::new("text").text(TEXT),
        "text { display: block; white-space: nowrap }",
    );

    assert_eq!(region.limit().x.0, TEXT.len() as f32 * ADVANCE - PORT);
}

#[test]
fn a_child_wider_than_the_scrollport_scrolls_its_container() {
    let region = page(
        Element::new("wide"),
        "wide { display: block; width: 500px; height: 10px }",
    );

    assert_eq!(region.limit().x.0, 500.0 - PORT);
}
