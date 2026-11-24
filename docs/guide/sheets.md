# Bottom sheets

`BottomSheet` in `zgui-ui` shows a modal sheet that rises from the bottom of the window, as a sheet with detents does on iOS. This guide tells you how to open one, how to choose its heights, and how to keep it open while it holds unsaved edits.

## Open a sheet

Give the sheet a writable signal. The sheet opens when the signal is true, and sets it to false when the person closes the sheet:

```rust
let open = RwSignal::new_local(false);
view! {
    Button(on:click = move |_| open.set(true)) {"Generate"}
    BottomSheet(open = open, label = "Generate Password") {
        /* content */
    }
}
```

The sheet is a modal dialog. While it is open:

- A dimmed layer covers the window. A press on it closes the sheet.
- <kbd>Escape</kbd> closes the sheet.
- <kbd>Tab</kbd> moves focus only through the sheet. When the sheet closes, focus goes back to where it was.
- `label` names the sheet for VoiceOver.

Put your own header with Cancel and Done controls in the sheet, and set `open` to false from them.

## Detents

A detent is a height that the sheet rests at:

- `Detent::Medium` is about half the height of the window.
- `Detent::Large` is the full height of the window, less a gap at the top.

Give `detents` the heights the sheet can rest at. The default is `Large` only. The sheet opens at the smallest detent. With more than one detent, the sheet draws a grabber at its top:

```rust
view! {
    BottomSheet(open = open, detents = vec![Detent::Medium, Detent::Large]) {
        /* content */
    }
}
```

To read or set the detent, give `detent` a signal. The sheet then keeps the detent that the signal holds when it opens.

## Dragging

The person drags the sheet up or down by any part of it. When the finger lifts, the sheet goes to the detent nearest to where the drag would stop, so a flick carries it past the nearest detent. A drag down past the smallest detent closes the sheet.

A drag starts where the sheet is on the window. A finger can catch the sheet while it moves to a detent, and the sheet stops under the finger.

A drag up past the largest detent stretches the sheet. The sheet moves less for each pixel that the finger travels, and its bottom edge stays at the bottom of the window. When the finger lifts, the sheet goes back to its height.

A scroll container in the sheet shares the drag with it:

- A drag down on a list at its top moves the sheet. A drag down on a scrolled list scrolls the list.
- A drag up on a list grows the sheet until it is at its largest detent. Then the list scrolls.

## The grabber

With more than one detent, the sheet has a grabber at its top. The grabber is a button with the name "Sheet grabber", and its value is the detent: "Medium" or "Large".

- A press, <kbd>Enter</kbd> or <kbd>Space</kbd> moves the sheet to its next larger detent. At the largest detent, the sheet goes to the detent below.
- <kbd>Arrow Up</kbd> and the VoiceOver increment (swipe up) expand the sheet.
- <kbd>Arrow Down</kbd> and the VoiceOver decrement (swipe down) collapse the sheet. At the smallest detent, they close the sheet.

The grabber obeys `dismiss_blocked`, as a drag does.

## Unsaved edits

Set `dismiss_blocked` to true while the sheet holds unsaved edits. While it is true, a drag down, a press on the dimmed layer, <kbd>Escape</kbd> and a collapse of the grabber at the smallest detent leave the sheet open, and the sheet calls `on_dismiss_attempt`. Use that callback to ask the person whether to discard the edits:

```rust
view! {
    BottomSheet(
        open = open,
        dismiss_blocked = dirty,
        on_dismiss_attempt = UnsyncCallback::new(move |()| confirm.set(true))
    ) {
        /* content */
    }
}
```

Your own Cancel control can still close the sheet: set `open` to false.

## The keyboard

When a field in the sheet shows the on-screen keyboard, the sheet rises to its large detent, and its content ends above the keyboard. If the keyboard covers the field at the large detent, the content scrolls to show it. When the keyboard goes away, the sheet goes back to its detent. A drag that starts while the keyboard shows starts from the large detent.

## iPad

In a window at least 700 pixels wide and 700 pixels tall, the sheet is a form sheet: a box in the middle of the window, with no grabber. The person can drag it down to close it.

## Motion

The sheet rises on the iOS sheet spring in 0.62 s, and goes down in 0.35 s. A released drag starts its motion at the speed of the finger. The next change of detent uses the spring again. When the person turns on Reduce Motion, the sheet fades in and out in place.

## Style

The sheet carries the class `zui-bottom-sheet` and the attribute `data-detent`, which is `medium` or `large`. Its parts are `zui-bottom-sheet__grabber`, `zui-bottom-sheet__grabber-bar` and `zui-bottom-sheet__content`.
