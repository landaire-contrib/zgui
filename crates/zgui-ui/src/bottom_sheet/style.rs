//! What a bottom sheet looks like, and how it moves.

use zgui::style;

style! { pub BottomSheetStyle =>
    // Pinned to the bottom of the window at its large height. A detent below large moves the sheet
    // down with `translate`, so the enter and exit animations keep `transform` to themselves. A drag
    // above the large detent stretches the sheet up, so that it still covers the bottom of the window.
    ":scope {
        position: fixed;
        left: 0;
        right: 0;
        bottom: 0;
        top: calc(env(safe-area-inset-top, 0px) + 10px - var(--zui-bottom-sheet-stretch, 0px));
        display: flex;
        flex-direction: column;
        padding: 0;
        overflow: hidden;
        --zui-surface-border: none;
        --zui-surface-radius: 12px;
        --zui-surface-fill: var(--zui-color-background);
        --zui-surface-ink: var(--zui-color-foreground);
        --zui-surface-shadow: var(--zui-shadow-lg);
        --zui-bottom-sheet-spring: linear(0, 0.115, 0.321, 0.517, 0.672, 0.784, 0.861, 0.912,
            0.945, 0.966, 0.979, 0.987, 0.992, 0.995, 0.997, 0.998, 1);
    }"

    // Up from below the window on the iOS sheet spring, and back down faster.
    ":scope {
        --zui-surface-enter-opacity: 1;
        --zui-surface-enter-scale: 1;
        --zui-surface-exit-opacity: 1;
        --zui-surface-exit-scale: 1;
        --zui-surface-enter-y: 100%;
        --zui-surface-exit-y: 100%;
        --zui-surface-enter-duration: 620ms;
        --zui-surface-exit-duration: 350ms;
        --zui-surface-enter-ease: var(--zui-bottom-sheet-spring);
        --zui-surface-exit-ease: var(--zui-bottom-sheet-spring);
    }"

    // At rest the sheet sits at its detent. While the keyboard shows, it sits at its large detent.
    ":scope {
        translate: 0px max(0px, calc(var(--zui-bottom-sheet-offset, 0px)
            - env(keyboard-inset-height, 0px) * 1000));
        transition: translate 500ms var(--zui-bottom-sheet-ease, var(--zui-bottom-sheet-spring)),
            top 500ms var(--zui-bottom-sheet-ease, var(--zui-bottom-sheet-spring));
    }"
    ":scope[data-dragging] {
        translate: 0px max(0px, var(--zui-bottom-sheet-offset, 0px));
        transition: none;
    }"

    ".zui-bottom-sheet__frame,
     .zui-bottom-sheet__body {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
    }"
    // A target larger than the bar it draws, so a finger and a pointer can take it.
    ".zui-bottom-sheet__grabber {
        flex-shrink: 0;
        align-self: center;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 64px;
        height: 20px;
        border-radius: 10px;
    }"
    ".zui-bottom-sheet__grabber:focus-visible {
        outline: none;
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ".zui-bottom-sheet__grabber-bar {
        width: 36px;
        height: 5px;
        border-radius: 2.5px;
        background-color: var(--zui-color-muted-foreground);
        opacity: 0.5;
    }"
    // The content ends above the keyboard.
    ".zui-bottom-sheet__content {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
        padding-bottom: env(keyboard-inset-height, 0px);
    }"
    // At medium the lower part of the sheet is below the window, so the content ends above it.
    ":scope[data-detent=\"medium\"] .zui-bottom-sheet__content {
        padding-bottom: max(env(keyboard-inset-height, 0px),
            calc(50vh - env(safe-area-inset-top, 0px) - 10px));
    }"

    // A regular-width window shows a form sheet: a box in the middle of the window.
    "@media (min-width: 700px) and (min-height: 700px) {
        :scope {
            left: calc(50% - 270px);
            right: auto;
            width: 540px;
            top: calc(50% - min(310px, 45vh));
            bottom: auto;
            height: min(620px, 90vh);
            --zui-surface-enter-y: 100vh;
            --zui-surface-exit-y: 100vh;
            translate: 0px 0px;
        }
        :scope[data-dragging] {
            translate: 0px max(0px, var(--zui-bottom-sheet-offset, 0px));
        }
        .zui-bottom-sheet__grabber {
            display: none;
        }
        :scope[data-detent=\"medium\"] .zui-bottom-sheet__content {
            padding-bottom: env(keyboard-inset-height, 0px);
        }
    }"

    "@media (prefers-reduced-motion: reduce) {
        :scope {
            --zui-surface-enter-opacity: 0;
            --zui-surface-exit-opacity: 0;
            --zui-surface-enter-y: 0px;
            --zui-surface-exit-y: 0px;
            --zui-surface-enter-duration: 200ms;
            --zui-surface-exit-duration: 200ms;
            --zui-surface-enter-ease: ease;
            --zui-surface-exit-ease: ease;
            transition-duration: 200ms;
        }
    }"
}
