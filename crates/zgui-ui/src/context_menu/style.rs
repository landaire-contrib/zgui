//! What the area a context menu opens over looks like, in tokens.

use zgui::style;

style! { pub ContextMenuStyle =>
    // The area is positioned so the anchor inside it can be placed at the pointer. It has no
    // appearance of its own beyond that.
    ":scope { position: relative; display: flex; flex-direction: column; }"
    // A point rather than a box: what the menu is anchored to is where the pointer was, and a
    // surface placed against something with size would be offset by that size.
    ".zui-context-menu__anchor { position: absolute; width: 0px; height: 0px; }"
}

style! { pub ContextMenuLiftStyle =>
    // The motion is a spring with a response of 0.35 s and a small overshoot, written as
    // `linear()` stops over its 500 ms. It goes back faster, without the overshoot.
    ":scope {}"
    ".zui-lift__scrim, .zui-lift__preview, .zui-lift__accessory, .zui-lift__menu {
        --zui-lift-spring: linear(0, 0.079, 0.248, 0.436, 0.608, 0.747, 0.851, 0.922, 0.968,
            0.995, 1.009, 1.014, 1.015, 1.013, 1.01, 1.008, 1.005, 1.003, 1.002, 1.001, 1);
        --zui-lift-enter: 500ms;
        --zui-lift-exit: 250ms;
    }"
    // The window behind is dimmed and blurred enough that the lifted region reads as above it.
    ".zui-lift__scrim { --zui-scrim-blur: 12px; }"
    // The space the parts are kept inside.
    ".zui-lift__bounds {
        position: fixed;
        left: calc(env(safe-area-inset-left, 0px) + 8px);
        right: calc(env(safe-area-inset-right, 0px) + 8px);
        top: calc(env(safe-area-inset-top, 0px) + 8px);
        bottom: calc(env(safe-area-inset-bottom, 0px) + 8px);
        pointer-events: none;
    }"

    // The preview takes no pointer, so a tap on it is a tap outside the menu, which closes it.
    // It moves from where the region is to its place, and grows a little as it lifts.
    ".zui-lift__preview {
        position: fixed;
        display: flex;
        flex-direction: column;
        overflow: hidden;
        pointer-events: none;
        transform: none;
        z-index: var(--zui-overlay-depth, 0);
        animation: none;
    }"
    ".zui-lift__preview[data-placed] {
        animation: zui-lift-preview-enter var(--zui-lift-enter) var(--zui-lift-spring) both;
    }"
    ".zui-lift__preview[data-state=\"closed\"] {
        animation: zui-lift-preview-exit var(--zui-lift-exit) ease-out both;
    }"
    "@keyframes zui-lift-preview-enter {
        from { transform: translate(var(--zui-lift-from-x, 0px), var(--zui-lift-from-y, 0px)) scale(1); }
        to { transform: translate(0px, 0px) scale(1.03); }
    }"
    "@keyframes zui-lift-preview-exit {
        from { transform: translate(0px, 0px) scale(1.03); }
        to { transform: translate(var(--zui-lift-from-x, 0px), var(--zui-lift-from-y, 0px)) scale(1); }
    }"

    // The accessory and the menu grow out of the edge of the preview they stand at.
    ".zui-lift__accessory {
        position: fixed;
        display: flex;
        transform-origin: left bottom;
        z-index: var(--zui-overlay-depth, 0);
        animation: none;
    }"
    ".zui-lift__accessory[data-align=\"end\"] { transform-origin: right bottom; }"
    ".zui-lift__accessory[data-align=\"center\"] { transform-origin: center bottom; }"
    ".zui-lift__accessory[data-placed] {
        animation: zui-lift-part-enter var(--zui-lift-enter) var(--zui-lift-spring) both;
    }"
    ".zui-lift__accessory[data-state=\"closed\"] {
        animation: zui-lift-part-exit var(--zui-lift-exit) ease-out both;
        pointer-events: none;
    }"
    "@keyframes zui-lift-part-enter {
        from { opacity: 0; transform: scale(0.5); }
        to { opacity: 1; transform: scale(1); }
    }"
    "@keyframes zui-lift-part-exit {
        from { opacity: 1; transform: scale(1); }
        to { opacity: 0; transform: scale(0.5); }
    }"

    // A panel the size of a finger's rows, on the surface's own entrance with the lift's timing.
    ".zui-lift__menu.zui-lift__menu {
        position: fixed;
        min-width: 220px;
        max-height: none;
        overflow-y: auto;
        --zui-surface-radius: 14px;
        --zui-surface-shadow: var(--zui-shadow-xl);
        --zui-surface-enter-scale: 0.5;
        --zui-surface-exit-scale: 0.5;
        --zui-surface-enter-duration: var(--zui-lift-enter);
        --zui-surface-exit-duration: var(--zui-lift-exit);
        --zui-surface-enter-ease: var(--zui-lift-spring);
        --zui-surface-exit-ease: ease-out;
        --zui-surface-origin-x: left;
        --zui-surface-origin-y: top;
    }"
    ".zui-lift__menu.zui-lift__menu:not([data-placed]) { animation: none; }"
    ".zui-lift__menu[data-align=\"end\"] { --zui-surface-origin-x: right; }"
    ".zui-lift__menu[data-align=\"center\"] { --zui-surface-origin-x: center; }"
    ".zui-lift__menu[data-side=\"top\"] { --zui-surface-origin-y: bottom; }"
    ".zui-lift__menu .zui-menu__item {
        min-height: 44px;
        padding: var(--zui-space-sm) var(--zui-space-md);
        font-size: var(--zui-type-size-md);
    }"

    // With less motion the parts fade in and out where they are.
    "@media (prefers-reduced-motion: reduce) {
        .zui-lift__preview[data-placed] {
            animation: zui-lift-fade-in 200ms ease both;
            transform: none;
        }
        .zui-lift__preview[data-state=\"closed\"] {
            animation: zui-lift-fade-out 200ms ease both;
        }
        .zui-lift__accessory[data-placed] {
            animation: zui-lift-fade-in 200ms ease both;
        }
        .zui-lift__accessory[data-state=\"closed\"] {
            animation: zui-lift-fade-out 200ms ease both;
        }
        .zui-lift__menu.zui-lift__menu {
            --zui-surface-enter-scale: 1;
            --zui-surface-exit-scale: 1;
            --zui-surface-enter-duration: 200ms;
            --zui-surface-exit-duration: 200ms;
            --zui-surface-enter-ease: ease;
            --zui-surface-exit-ease: ease;
        }
    }"
    "@keyframes zui-lift-fade-in { from { opacity: 0; } to { opacity: 1; } }"
    "@keyframes zui-lift-fade-out { from { opacity: 1; } to { opacity: 0; } }"

    // The region a preview stands in for is hidden while the preview is up.
    "[data-zui-lifted] { visibility: hidden; }"
}
