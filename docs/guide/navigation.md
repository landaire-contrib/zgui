# Navigation stacks

`NavStack` in `zgui-ui` shows a stack of pages under one nav bar, as a `UINavigationController` does on iOS. This guide tells you how to push and pop pages, how a page declares its bar, and how to block back while a page holds unsaved edits.

## Make a stack

A `NavPath` holds the routes of the stack. The route type is yours. The stack calls `page` once for each route that you push, and keeps that page mounted until you pop it:

```rust
#[derive(Clone)]
enum Route {
    Groups,
    Group(GroupId),
    Entry(EntryId),
}

let path = NavPath::new(Route::Groups);
view! {
    NavStack(path = path, page = move |route: Route| match route {
        Route::Groups => AnyView::new(view! { GroupsPage(path = path) }),
        Route::Group(id) => AnyView::new(view! { GroupPage(path = path, id = id) }),
        Route::Entry(id) => AnyView::new(view! { EntryPage(id = id) }),
    })
}
```

A page below the top page stays mounted. Thus it keeps its signals and its scroll position. Because the stack builds a page only once, read changing data from signals inside the page.

`NavPath` is `Copy`, so a click handler can hold it:

- `path.push(route)` slides a new page in.
- `path.pop()` slides the top page out.
- `path.pop_to_root()` slides the top page out and removes the pages between it and the root page at once.
- `path.top()`, `path.pages()`, `path.count()` and `path.is_at_root()` read the stack and subscribe to it.

`pop` and `pop_to_root` return `Err(PopError::AtRoot)` when only the root page is left, and `Err(PopError::Blocked)` while the top page blocks back.

## Declare the bar of a page

Wrap the content of a page in `NavPage`. The stack draws one bar for all pages:

```rust
view! {
    NavPage(title = "Bank") {
        NavTrailing(slot) { Button(on:click = add) {"Add"} }
        ScrollArea { /* rows */ }
    }
}
```

- `title` is the title in the bar. The page above uses it as the label of its back button.
- `NavTrailing` holds the controls at the trailing end of the bar.
- `NavLeading` holds the controls at the leading end, after the back button.

The page owns its scroll container. The stack gives the page the height below the bar.

## Block back for unsaved edits

Give `back_blocked` a signal that is true while the page holds unsaved edits. While it is true, the stack removes the back button, and `pop` and `pop_to_root` refuse. Put Cancel in `NavLeading` and Done in `NavTrailing`, as iOS does in edit mode:

```rust
let editing = RwSignal::new_local(false);
view! {
    NavPage(title = "Checking", back_blocked = editing) {
        NavLeading(slot) {
            Show(when = move || editing.get()) {
                Button(on:click = move |_| editing.set(false)) {"Cancel"}
            }
        }
        NavTrailing(slot) {
            Button(on:click = move |_| editing.update(|on| *on = !*on)) {
                {move || if editing.get() { "Done" } else { "Edit" }}
            }
        }
        /* fields */
    }
}
```

## Motion

A push slides the new page in from the trailing edge. The page below moves 30 percent of the width the other way and dims. A pop does the reverse. The bar titles cross-fade. The motion uses the iOS navigation spring and takes 0.5 s.

When the person turns on Reduce Motion, the pages cross-fade in place.

The motion ends when the animation of the moving page ends. If no animation ends it, the motion ends after 0.9 s.

## Pages that are not on top

The stack hides each page below the top page with `visibility: hidden`, which removes it from pointer input and the focus order. It also sets `inert` on the page and marks it hidden for accessibility, so VoiceOver reaches only the top page. During a motion, no page takes pointer input.

## Style

The stack is a flex column that fills its parent. The elements carry these classes:

| Class | Element |
|---|---|
| `zui-nav-bar` | the bar |
| `zui-nav-bar__row` | the bar of one page, with `data-place` |
| `zui-nav-bar__title` | the title |
| `zui-nav-back` | the back button |
| `zui-nav-pages` | the area below the bar |
| `zui-nav-page` | one page, with `data-place` |

`data-place` is `current`, `covered`, `push-in`, `push-out`, `pop-in` or `pop-out`. Set `--zui-nav-ease` on the stack to change the curve of the motion.
