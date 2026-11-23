//! What a navigation stack looks like, and how its pages move.

use zgui::style;

style! { pub NavStackStyle =>
    // The motion is the iOS navigation spring: critically damped, response 0.35 s, written as
    // `linear()` stops.
    ":scope {
        position: relative;
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
        overflow: hidden;
        --zui-nav-duration: 500ms;
        --zui-nav-ease: linear(0, 0.115, 0.321, 0.517, 0.672, 0.784, 0.861, 0.912, 0.945, 0.966,
            0.979, 0.987, 0.992, 0.995, 0.997, 0.998, 1);
        --zui-nav-fade-duration: 200ms;
    }"

    ".zui-nav-bar {
        position: relative;
        flex-shrink: 0;
        height: 44px;
        background-color: var(--zui-color-background);
    }"
    ".zui-nav-bar__row {
        position: absolute;
        inset: 0;
        display: grid;
        grid-template-columns: 1fr auto 1fr;
        align-items: center;
        gap: var(--zui-space-sm);
        padding-left: var(--zui-space-sm);
        padding-right: var(--zui-space-sm);
    }"
    ".zui-nav-bar__leading {
        display: flex;
        align-items: center;
        justify-self: start;
        gap: var(--zui-space-sm);
        min-width: 0;
    }"
    ".zui-nav-bar__trailing {
        display: flex;
        align-items: center;
        justify-self: end;
        gap: var(--zui-space-sm);
        min-width: 0;
    }"
    ".zui-nav-bar__title {
        font-size: 17px;
        font-weight: 600;
        color: var(--zui-color-foreground);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        min-width: 0;
    }"
    ".zui-nav-back {
        display: flex;
        align-items: center;
        gap: 2px;
        min-height: 44px;
        min-width: 44px;
        color: var(--zui-color-primary);
        font-size: 17px;
    }"
    ".zui-nav-back__title {
        max-width: 140px;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }"

    // A bar row cross-fades with its page, and its title moves a short way.
    ".zui-nav-bar__row[data-place=\"covered\"] { visibility: hidden; }"
    ".zui-nav-bar__row[data-place=\"push-in\"],
     .zui-nav-bar__row[data-place=\"pop-in\"] {
        animation: zui-nav-fade-in var(--zui-nav-duration) var(--zui-nav-ease) both;
        pointer-events: none;
    }"
    ".zui-nav-bar__row[data-place=\"push-out\"],
     .zui-nav-bar__row[data-place=\"pop-out\"] {
        animation: zui-nav-fade-out var(--zui-nav-duration) var(--zui-nav-ease) both;
        pointer-events: none;
    }"
    ".zui-nav-bar__row[data-place=\"push-in\"] .zui-nav-bar__title {
        animation: zui-nav-title-from-trailing var(--zui-nav-duration) var(--zui-nav-ease) both;
    }"
    ".zui-nav-bar__row[data-place=\"push-out\"] .zui-nav-bar__title {
        animation: zui-nav-title-to-leading var(--zui-nav-duration) var(--zui-nav-ease) both;
    }"
    ".zui-nav-bar__row[data-place=\"pop-out\"] .zui-nav-bar__title {
        animation: zui-nav-title-to-trailing var(--zui-nav-duration) var(--zui-nav-ease) both;
    }"
    ".zui-nav-bar__row[data-place=\"pop-in\"] .zui-nav-bar__title {
        animation: zui-nav-title-from-leading var(--zui-nav-duration) var(--zui-nav-ease) both;
    }"

    ".zui-nav-pages {
        position: relative;
        flex: 1;
        min-height: 0;
        overflow: hidden;
    }"
    ".zui-nav-page {
        position: absolute;
        inset: 0;
        display: flex;
        flex-direction: column;
        background-color: var(--zui-color-background);
    }"
    ".zui-nav-content {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
    }"
    ".zui-nav-page__dim {
        position: absolute;
        inset: 0;
        background-color: rgba(0, 0, 0, 0.1);
        opacity: 0;
        pointer-events: none;
    }"

    // A covered page keeps its layout, so its scroll position stays, and is hidden.
    ".zui-nav-page[data-place=\"covered\"] { visibility: hidden; }"
    ".zui-nav-page[data-place=\"push-in\"],
     .zui-nav-page[data-place=\"push-out\"],
     .zui-nav-page[data-place=\"pop-in\"],
     .zui-nav-page[data-place=\"pop-out\"] {
        pointer-events: none;
    }"
    ".zui-nav-page[data-place=\"push-in\"] {
        animation: zui-nav-slide-in var(--zui-nav-duration) var(--zui-nav-ease) both;
        box-shadow: -1px 0 12px rgba(0, 0, 0, 0.12);
    }"
    ".zui-nav-page[data-place=\"pop-out\"] {
        animation: zui-nav-slide-out var(--zui-nav-duration) var(--zui-nav-ease) both;
    }"
    ".zui-nav-page[data-place=\"push-out\"] {
        animation: zui-nav-recede var(--zui-nav-duration) var(--zui-nav-ease) both;
    }"
    ".zui-nav-page[data-place=\"pop-in\"] {
        animation: zui-nav-return var(--zui-nav-duration) var(--zui-nav-ease) both;
    }"
    ".zui-nav-page[data-place=\"push-out\"] > .zui-nav-page__dim {
        animation: zui-nav-fade-in var(--zui-nav-duration) var(--zui-nav-ease) both;
    }"
    ".zui-nav-page[data-place=\"pop-in\"] > .zui-nav-page__dim {
        animation: zui-nav-fade-out var(--zui-nav-duration) var(--zui-nav-ease) both;
    }"

    "@keyframes zui-nav-slide-in {
        from { transform: translateX(100%); }
        to { transform: translateX(0%); }
    }"
    // The shadow fades as the page goes, so no shadow stays at the trailing edge.
    "@keyframes zui-nav-slide-out {
        from { transform: translateX(0%); box-shadow: -1px 0 12px rgba(0, 0, 0, 0.12); }
        to { transform: translateX(100%); box-shadow: -1px 0 12px rgba(0, 0, 0, 0); }
    }"
    "@keyframes zui-nav-recede {
        from { transform: translateX(0%); }
        to { transform: translateX(-30%); }
    }"
    "@keyframes zui-nav-return {
        from { transform: translateX(-30%); }
        to { transform: translateX(0%); }
    }"
    "@keyframes zui-nav-fade-in { from { opacity: 0; } to { opacity: 1; } }"
    "@keyframes zui-nav-fade-out { from { opacity: 1; } to { opacity: 0; } }"
    "@keyframes zui-nav-title-from-trailing {
        from { transform: translateX(48px); }
        to { transform: translateX(0px); }
    }"
    "@keyframes zui-nav-title-to-leading {
        from { transform: translateX(0px); }
        to { transform: translateX(-48px); }
    }"
    "@keyframes zui-nav-title-to-trailing {
        from { transform: translateX(0px); }
        to { transform: translateX(48px); }
    }"
    "@keyframes zui-nav-title-from-leading {
        from { transform: translateX(-48px); }
        to { transform: translateX(0px); }
    }"

    // With Reduce Motion, pages cross-fade in place.
    "@media (prefers-reduced-motion: reduce) {
        .zui-nav-page[data-place=\"push-in\"] {
            animation: zui-nav-fade-in var(--zui-nav-fade-duration) ease both;
            box-shadow: none;
        }
        .zui-nav-page[data-place=\"pop-out\"] {
            animation: zui-nav-fade-out var(--zui-nav-fade-duration) ease both;
        }
        .zui-nav-page[data-place=\"push-out\"],
        .zui-nav-page[data-place=\"pop-in\"],
        .zui-nav-page[data-place=\"push-out\"] > .zui-nav-page__dim,
        .zui-nav-page[data-place=\"pop-in\"] > .zui-nav-page__dim,
        .zui-nav-bar__row[data-place=\"push-in\"] .zui-nav-bar__title,
        .zui-nav-bar__row[data-place=\"push-out\"] .zui-nav-bar__title,
        .zui-nav-bar__row[data-place=\"pop-in\"] .zui-nav-bar__title,
        .zui-nav-bar__row[data-place=\"pop-out\"] .zui-nav-bar__title {
            animation: none;
        }
        .zui-nav-bar__row[data-place=\"push-in\"],
        .zui-nav-bar__row[data-place=\"pop-in\"] {
            animation: zui-nav-fade-in var(--zui-nav-fade-duration) ease both;
        }
        .zui-nav-bar__row[data-place=\"push-out\"],
        .zui-nav-bar__row[data-place=\"pop-out\"] {
            animation: zui-nav-fade-out var(--zui-nav-fade-duration) ease both;
        }
    }"
}
