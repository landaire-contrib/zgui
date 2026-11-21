//! What a bottom-up list looks like.

use zgui::style;

style! { pub BottomUpListStyle =>
    // A column, so that the fill can take the room the rows leave when they are short. The rows
    // then stand at the bottom of the port.
    ":scope {
        display: flex;
        flex-direction: column;
        overflow-y: auto;
        overflow-x: hidden;
        position: relative;
    }"
    ":scope > .zui-bottom-up-list__fill {
        flex: 1 0 0px;
    }"
    ":scope > .zui-bottom-up-list__head {
        display: flex;
        flex-direction: column;
        flex: none;
    }"
    ":scope > .zui-bottom-up-list__pane {
        display: flex;
        flex-direction: column;
        flex: none;
    }"
    ":scope > .zui-bottom-up-list__pane > * {
        flex: none;
        box-sizing: border-box;
    }"
    ":scope > .zui-bottom-up-list__foot {
        display: flex;
        flex-direction: column;
        flex: none;
    }"
}
