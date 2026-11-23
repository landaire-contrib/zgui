# Touch gestures

The window reads taps, long presses, pans and pinches from the touch stream. This guide tells you which events your elements receive and how to take a pan from the scrolling.

## What the window does with a touch

- A tap is a press and a release that did not travel past the slop. It clicks the element.
- A long press is a contact that stays still for half a second. It sends `context_menu` to the element under it. If a listener calls `prevent_default` on that event, the contact does not pan, so the finger can move onto the menu that the listener opened.
- A pan is a contact that travels past the slop. It scrolls the scroll containers under the point where it started. A pan never clicks.
- The scroll moves as a scroll view on iOS does. The content follows the finger and stretches past an end. When the finger lifts, the content keeps its speed and slows down, and it bounces at an end that it reaches.
- A container stretches and bounces only along an axis where its content is larger than the container.
- A touch on content that moves stops it. That touch does not click.
- A pinch sends `pinch` events to the element under it.

## Taking a pan from the scrolling

Listen for `horizontal_pan` or `vertical_pan` to receive the pans along that axis:

```rust
view! {
    column(class = "row", on:horizontal_pan = move |event| {
        let travel = event.translation().width.0;
        // Move the row with the finger.
    }) {
        text { "Swipe me" }
    }
}
```

When a pan starts, the window compares how far the contact travelled across and down. The larger distance gives the axis of the pan. A diagonal pan is vertical. Then the window examines the element under the start point and each of its ancestors, from the innermost outwards:

- If the element listens for pans along the axis, it gets the pan.
- If the element is a scroll container that can scroll in the direction the contact moves, the pan scrolls.

The decision stays until the contact lifts. Thus a row in a vertical list gets the sideways pans that start on it, and the list keeps the vertical pans. A horizontal carousel between the row and the list keeps the sideways pans until it reaches its end.

## The pan events

The window sends each event of a pan to the element under the start point, and the event bubbles to your listener. If that element moves out of your listener or out of the document during the pan, the window sends the remaining events directly to your listener. A `PanEvent` holds:

- `phase`: `Started`, `Moved`, `Ended` or `Cancelled`. A pan ends with `Cancelled` when the platform cancels the contact or an element captures the pointer.
- `origin`: where the contact went down.
- `position`: where the contact is now. `translation()` gives the distance from `origin`.
- `velocity`: the speed of the contact in CSS pixels per second. On `Ended`, it is the speed of the lift. Use it to tell a flick from a slow release.
- `kind`: the kind of pointer.

A listener on an ancestor can stop the propagation of an event. For example, a pane that opens from its left edge can listen in the capture phase, take the pans that start near the edge, and let the other pans reach the rows inside it.

## Trackpads

A trackpad sends its two-finger swipes as `wheel` events with a phase, and these events do not become pans. The platform gives no axis and no start point for a swipe, and the momentum that follows a swipe arrives as more `wheel` events. To follow a swipe on a trackpad, listen for `wheel`, read `ScrollPhase`, and call `prevent_default` on the events that you use.
