//! A copy of the focused field's text that UIKit reads and edits between frames.
//!
//! UIKit asks a text input for its text, selection and geometry at any moment, and expects an
//! edit it makes to show at once. The window applies an edit only at its next frame. So the copy
//! applies each edit at once, gives the window the events for the edit, and takes the window's
//! next snapshot as the truth.
//!
//! Offsets are byte offsets into the copy's text. UIKit counts in UTF-16 code units, and
//! [`Mirror::offset_by_utf16`] and [`Mirror::utf16_between`] convert.

use core::ops::Range;

use zgui_geom::{Css, CssPx, Point, Rect, Size};
use zgui_platform::{SurfaceEvent, TextSelection, TextSnapshot};
use zgui_vocab::{
    ImeEvent, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp,
};

/// A direction UIKit moves a position in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    /// Toward the start of the text.
    Backward,
    /// Toward the end of the text.
    Forward,
    /// One laid-out line up.
    Up,
    /// One laid-out line down.
    Down,
}

/// What taking a snapshot changed in the copy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Changed {
    /// The text or the composition changed.
    pub(crate) text: bool,
    /// The selection changed.
    pub(crate) selection: bool,
}

/// One rectangle of a selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SelectionRect {
    /// The rectangle, in CSS pixels from the surface's top-left corner.
    pub(crate) rect: Rect<CssPx, Css>,
    /// Whether the selection starts in this rectangle.
    pub(crate) contains_start: bool,
    /// Whether the selection ends in this rectangle.
    pub(crate) contains_end: bool,
}

/// The vertical extent of one laid-out line.
#[derive(Clone, Copy, Debug)]
struct LineSpan {
    /// Which line.
    line: u32,
    /// Its top edge.
    top: f32,
    /// Its bottom edge.
    bottom: f32,
}

/// The copy of the focused field's text.
#[derive(Clone, Debug, Default)]
pub(crate) struct Mirror {
    /// The text, selection, composition and geometry, as of the last snapshot and the edits
    /// made since.
    state: TextSnapshot,
}

impl Mirror {
    /// An empty copy.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The text.
    #[cfg(test)]
    pub(crate) fn text(&self) -> &str {
        &self.state.text
    }

    /// The length of the text in bytes.
    pub(crate) fn len(&self) -> usize {
        self.state.text.len()
    }

    /// What is selected, lower end first.
    pub(crate) fn selection(&self) -> Range<usize> {
        self.state.selection.range()
    }

    /// The text an input method is composing.
    pub(crate) fn marked(&self) -> Option<Range<usize>> {
        self.state.composition.clone()
    }

    /// The box the text is laid out in.
    pub(crate) fn bounds(&self) -> Rect<CssPx, Css> {
        self.state.bounds
    }

    /// What taking `snapshot` would change, or `None` when the copy already holds it.
    pub(crate) fn changes(&self, snapshot: &TextSnapshot) -> Option<Changed> {
        if self.state == *snapshot {
            return None;
        }

        Some(Changed {
            text: self.state.text != snapshot.text
                || self.state.composition != snapshot.composition,
            selection: self.state.selection.range() != snapshot.selection.range(),
        })
    }

    /// Takes the window's snapshot as the truth, and tells what it changed.
    pub(crate) fn replace(&mut self, snapshot: TextSnapshot) -> Changed {
        let changed = self.changes(&snapshot).unwrap_or_default();
        self.state = snapshot;

        changed
    }

    /// `offset` held inside the text, moved back to the start of the character it falls in.
    pub(crate) fn clamp(&self, offset: usize) -> usize {
        let text = &self.state.text;
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }

        offset
    }

    /// The text between two offsets.
    pub(crate) fn slice(&self, range: Range<usize>) -> &str {
        let start = self.clamp(range.start);
        let end = self.clamp(range.end).max(start);
        &self.state.text[start..end]
    }

    /// How many UTF-16 code units lie from `from` to `to`, negative when `to` is before `from`.
    pub(crate) fn utf16_between(&self, from: usize, to: usize) -> isize {
        let (start, end) = (self.clamp(from.min(to)), self.clamp(from.max(to)));
        let units = self.state.text[start..end].encode_utf16().count() as isize;
        if to < from { -units } else { units }
    }

    /// The offset `units` UTF-16 code units after `from`, or before it when negative.
    ///
    /// `None` when that is outside the text. A count that ends inside a character ends after it.
    pub(crate) fn offset_by_utf16(&self, from: usize, units: isize) -> Option<usize> {
        let from = self.clamp(from);
        let text = &self.state.text;
        let mut remaining = units.unsigned_abs();
        if units >= 0 {
            let mut offset = from;
            for character in text[from..].chars() {
                if remaining == 0 {
                    break;
                }
                remaining = remaining.saturating_sub(character.len_utf16());
                offset += character.len_utf8();
            }
            return (remaining == 0).then_some(offset);
        }

        let mut offset = from;
        for character in text[..from].chars().rev() {
            if remaining == 0 {
                break;
            }
            remaining = remaining.saturating_sub(character.len_utf16());
            offset -= character.len_utf8();
        }

        (remaining == 0).then_some(offset)
    }

    /// Inserts text the keyboard typed, in place of the composition or the selection.
    ///
    /// A lone line break is the return key. It ends a composition and presses Enter, and changes
    /// nothing in the copy: the field decides what Enter does, and its next snapshot tells.
    pub(crate) fn insert(&mut self, text: &str) -> Vec<SurfaceEvent> {
        if matches!(text, "\n" | "\r" | "\r\n") {
            let mut events = self.unmark();
            events.extend(press(NamedKey::Enter, KeyCode::Enter));
            return events;
        }

        let target = self.state.composition.take().unwrap_or(self.selection());
        self.splice(target.clone(), text);
        self.state.selection = TextSelection::caret(target.start + text.len());

        vec![SurfaceEvent::Ime(ImeEvent::Commit(text.into()))]
    }

    /// Deletes the selection, or the character before the caret.
    pub(crate) fn delete_backward(&mut self) -> Vec<SurfaceEvent> {
        let mut events = self.unmark();
        let selection = self.selection();
        let range = if selection.is_empty() {
            self.boundary_before(selection.start)
                .map_or(selection.clone(), |start| start..selection.start)
        } else {
            selection
        };
        self.splice(range.clone(), "");
        self.state.selection = TextSelection::caret(range.start);
        events.extend(press(NamedKey::Backspace, KeyCode::Backspace));

        events
    }

    /// Puts provisional text in place of the composition or the selection.
    ///
    /// `selected` is the part of `text` to select, in UTF-16 code units. Empty text ends the
    /// composition and removes what it showed.
    pub(crate) fn set_marked(&mut self, text: &str, selected: Range<usize>) -> Vec<SurfaceEvent> {
        let starts = self.state.composition.is_none();
        let target = self.state.composition.clone().unwrap_or(self.selection());
        if text.is_empty() {
            if starts {
                return Vec::new();
            }
            self.splice(target.clone(), "");
            self.state.composition = None;
            self.state.selection = TextSelection::caret(target.start);
            return vec![SurfaceEvent::Ime(ImeEvent::Commit("".into()))];
        }

        self.splice(target.clone(), text);
        let start = target.start;
        self.state.composition = Some(start..start + text.len());
        let cursor = utf16_to_bytes(text, selected.start)..utf16_to_bytes(text, selected.end);
        self.state.selection = TextSelection::new(start + cursor.start, start + cursor.end);

        let mut events = Vec::with_capacity(2);
        if starts {
            events.push(SurfaceEvent::Ime(ImeEvent::Enabled));
        }
        events.push(SurfaceEvent::Ime(ImeEvent::Preedit {
            text: text.into(),
            cursor: Some(cursor),
        }));

        events
    }

    /// Keeps the provisional text as typed text, with the caret after it.
    pub(crate) fn unmark(&mut self) -> Vec<SurfaceEvent> {
        let Some(marked) = self.state.composition.take() else {
            return Vec::new();
        };
        let text = self.slice(marked.clone()).to_owned();
        self.state.selection = TextSelection::caret(marked.end);

        vec![SurfaceEvent::Ime(ImeEvent::Commit(text.into()))]
    }

    /// Selects `range`.
    pub(crate) fn select(&mut self, range: Range<usize>) -> Vec<SurfaceEvent> {
        let range = self.clamp(range.start)..self.clamp(range.end);
        if range == self.selection() {
            return Vec::new();
        }
        self.state.selection = TextSelection::new(range.start, range.end);

        vec![SurfaceEvent::SelectText(self.state.selection)]
    }

    /// Replaces `range` with `text`, and keeps the selection on the text it was on.
    pub(crate) fn replace_range(&mut self, range: Range<usize>, text: &str) -> Vec<SurfaceEvent> {
        let mut events = self.unmark();
        let range = self.clamp(range.start)..self.clamp(range.end);
        let before = self.selection();
        let moved = |offset: usize| {
            if offset <= range.start {
                offset
            } else if offset >= range.end {
                offset - range.len() + text.len()
            } else {
                range.start + text.len()
            }
        };
        let after = moved(before.start)..moved(before.end);

        events.push(SurfaceEvent::SelectText(TextSelection::new(
            range.start,
            range.end,
        )));
        events.push(SurfaceEvent::Ime(ImeEvent::Commit(text.into())));
        self.splice(range.clone(), text);
        self.state.selection = TextSelection::new(after.start, after.end);
        if after != (range.start + text.len()..range.start + text.len()) {
            events.push(SurfaceEvent::SelectText(self.state.selection));
        }

        events
    }

    /// Where the caret at `offset` is drawn.
    ///
    /// An offset at a line wrap is drawn at the start of the later line. An offset with no caret
    /// of its own, which an edit not yet laid out makes, takes the caret before it.
    pub(crate) fn caret_rect(&self, offset: usize) -> Rect<CssPx, Css> {
        let carets = &self.state.carets;
        let found = carets
            .iter()
            .rev()
            .find(|stop| stop.offset == offset)
            .or_else(|| carets.iter().rev().find(|stop| stop.offset < offset))
            .or_else(|| carets.first());
        if let Some(stop) = found {
            return stop.rect;
        }

        let bounds = self.state.bounds;
        Rect::new(bounds.origin, Size::new(CssPx(1.0), bounds.size.height))
    }

    /// The rectangles that cover `range`, one for each line it is on.
    pub(crate) fn selection_rects(&self, range: Range<usize>) -> Vec<SelectionRect> {
        if range.is_empty() {
            return Vec::new();
        }

        let mut rects: Vec<SelectionRect> = Vec::new();
        let mut current: Option<(u32, f32, f32, f32, f32)> = None;
        let inside = self
            .state
            .carets
            .iter()
            .filter(|stop| range.contains(&stop.offset) || stop.offset == range.end);
        for stop in inside {
            let rect = stop.rect;
            current = match current {
                Some((line, left, right, top, bottom)) if line == stop.line => Some((
                    line,
                    left.min(rect.left().0),
                    right.max(rect.left().0),
                    top.min(rect.top().0),
                    bottom.max(rect.bottom().0),
                )),
                previous => {
                    if let Some(done) = previous {
                        push_line(&mut rects, done);
                    }
                    Some((
                        stop.line,
                        rect.left().0,
                        rect.left().0,
                        rect.top().0,
                        rect.bottom().0,
                    ))
                }
            };
        }
        if let Some(done) = current {
            push_line(&mut rects, done);
        }
        if let Some(first) = rects.first_mut() {
            first.contains_start = true;
        }
        if let Some(last) = rects.last_mut() {
            last.contains_end = true;
        }

        rects
    }

    /// The rectangle of the first line of `range`, or the caret when the range is empty.
    pub(crate) fn first_rect(&self, range: Range<usize>) -> Rect<CssPx, Css> {
        self.selection_rects(range.clone())
            .first()
            .map_or_else(|| self.caret_rect(range.start), |first| first.rect)
    }

    /// The offset whose caret is nearest to `point`, inside `within` when it is given.
    pub(crate) fn closest(&self, point: Point<CssPx, Css>, within: Option<Range<usize>>) -> usize {
        let allowed = |offset: usize| {
            within
                .as_ref()
                .is_none_or(|range| range.contains(&offset) || offset == range.end)
        };
        let candidates: Vec<_> = self
            .state
            .carets
            .iter()
            .filter(|stop| allowed(stop.offset))
            .collect();
        let Some(line) = nearest_line(&lines_of(candidates.iter().copied()), point.y.0) else {
            return match within {
                Some(range) => range.start,
                None if point.y.0 > self.state.bounds.top().0 => self.len(),
                None => 0,
            };
        };

        candidates
            .iter()
            .filter(|stop| stop.line == line)
            .min_by(|first, second| {
                let first = (first.rect.left().0 - point.x.0).abs();
                let second = (second.rect.left().0 - point.x.0).abs();
                first.total_cmp(&second)
            })
            .map_or(0, |stop| stop.offset)
    }

    /// The offset `count` steps from `offset` in `direction`, or `None` past either end.
    ///
    /// A step backward or forward is one character a person sees. A step up or down is one line,
    /// at the same horizontal place, and stops at the first or the last line's edge.
    pub(crate) fn step(&self, offset: usize, direction: Direction, count: usize) -> Option<usize> {
        let offset = self.clamp(offset);
        match direction {
            Direction::Backward => (0..count).try_fold(offset, |at, _| self.boundary_before(at)),
            Direction::Forward => (0..count).try_fold(offset, |at, _| self.boundary_after(at)),
            Direction::Up | Direction::Down => {
                let caret = self.caret_rect(offset);
                let lines = lines_of(self.state.carets.iter());
                let current = nearest_line(&lines, caret.top().0 + caret.height().0 / 2.0)?;
                let index = lines.iter().position(|span| span.line == current)?;
                let target = if direction == Direction::Up {
                    index.checked_sub(count)
                } else {
                    Some(index + count).filter(|target| *target < lines.len())
                };
                let Some(target) = target else {
                    return Some(if direction == Direction::Up {
                        0
                    } else {
                        self.len()
                    });
                };
                let span = lines[target];
                let x = caret.left().0;
                self.state
                    .carets
                    .iter()
                    .filter(|stop| stop.line == span.line)
                    .min_by(|first, second| {
                        let first = (first.rect.left().0 - x).abs();
                        let second = (second.rect.left().0 - x).abs();
                        first.total_cmp(&second)
                    })
                    .map(|stop| stop.offset)
            }
        }
    }

    /// The character a person sees nearest to `point`.
    pub(crate) fn character_at(&self, point: Point<CssPx, Css>) -> Range<usize> {
        let offset = self.closest(point, None);
        match self.boundary_after(offset) {
            Some(end) => offset..end,
            None => self.boundary_before(offset).unwrap_or(offset)..offset,
        }
    }

    /// The boundary between characters a person sees just before `offset`.
    fn boundary_before(&self, offset: usize) -> Option<usize> {
        if offset == 0 {
            return None;
        }
        let from_carets = self
            .state
            .carets
            .iter()
            .rev()
            .map(|stop| stop.offset)
            .find(|stop| *stop < offset);
        let from_text = self.state.text[..self.clamp(offset)]
            .char_indices()
            .next_back()
            .map(|(index, _)| index);

        from_carets.or(from_text)
    }

    /// The boundary between characters a person sees just after `offset`.
    fn boundary_after(&self, offset: usize) -> Option<usize> {
        if offset >= self.len() {
            return None;
        }
        let from_carets = self
            .state
            .carets
            .iter()
            .map(|stop| stop.offset)
            .find(|stop| *stop > offset && *stop <= self.len());
        let from_text = self.state.text[offset..]
            .chars()
            .next()
            .map(|character| offset + character.len_utf8());

        from_carets.or(from_text)
    }

    /// Replaces `range` of the text with `with`, and moves the carets after it along.
    fn splice(&mut self, range: Range<usize>, with: &str) {
        let range = self.clamp(range.start)..self.clamp(range.end);
        self.state.text.replace_range(range.clone(), with);
        self.state
            .carets
            .retain(|stop| stop.offset <= range.start || stop.offset >= range.end);
        for stop in &mut self.state.carets {
            if stop.offset >= range.end && !(range.is_empty() && stop.offset == range.start) {
                stop.offset = stop.offset - range.len() + with.len();
            }
        }
    }
}

/// A press and a release of one named key.
///
/// The loop stamps each key event as it delivers it, as it stamps the events winit reports.
pub(crate) fn press(key: NamedKey, code: KeyCode) -> [SurfaceEvent; 2] {
    [KeyState::Pressed, KeyState::Released].map(|state| SurfaceEvent::Key {
        state,
        event: KeyEvent::named(key, PhysicalKey::Code(code)),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    })
}

/// The byte offset in `text` that `units` UTF-16 code units reach, held inside the text.
fn utf16_to_bytes(text: &str, units: usize) -> usize {
    let mut counted = 0;
    for (index, character) in text.char_indices() {
        if counted >= units {
            return index;
        }
        counted += character.len_utf16();
    }

    text.len()
}

/// Ends one line of a selection, leaving out a line the selection only touches.
fn push_line(rects: &mut Vec<SelectionRect>, line: (u32, f32, f32, f32, f32)) {
    let (_, left, right, top, bottom) = line;
    if right <= left {
        return;
    }
    rects.push(SelectionRect {
        rect: Rect::new(
            Point::new(CssPx(left), CssPx(top)),
            Size::new(CssPx(right - left), CssPx(bottom - top)),
        ),
        contains_start: false,
        contains_end: false,
    });
}

/// The lines some carets are on, top first.
fn lines_of<'a>(carets: impl Iterator<Item = &'a zgui_platform::CaretStop>) -> Vec<LineSpan> {
    let mut lines: Vec<LineSpan> = Vec::new();
    for stop in carets {
        let (top, bottom) = (stop.rect.top().0, stop.rect.bottom().0);
        match lines.iter_mut().find(|span| span.line == stop.line) {
            Some(span) => {
                span.top = span.top.min(top);
                span.bottom = span.bottom.max(bottom);
            }
            None => lines.push(LineSpan {
                line: stop.line,
                top,
                bottom,
            }),
        }
    }
    lines.sort_by_key(|span| span.line);

    lines
}

/// The line that holds `y`, or the nearest one.
fn nearest_line(lines: &[LineSpan], y: f32) -> Option<u32> {
    lines
        .iter()
        .min_by(|first, second| distance(first, y).total_cmp(&distance(second, y)))
        .map(|span| span.line)
}

/// How far `y` is from a line, zero inside it.
fn distance(span: &LineSpan, y: f32) -> f32 {
    if y < span.top {
        span.top - y
    } else if y > span.bottom {
        y - span.bottom
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use zgui_platform::CaretStop;

    /// A caret ten pixels wide per character, twenty pixels tall per line.
    fn stop(offset: usize, line: u32, column: usize) -> CaretStop {
        CaretStop {
            offset,
            line,
            rect: Rect::new(
                Point::new(CssPx(column as f32 * 10.0), CssPx(line as f32 * 20.0)),
                Size::new(CssPx(1.0), CssPx(20.0)),
            ),
        }
    }

    /// A copy of `text` laid out on one line, with the caret at `caret`.
    fn one_line(text: &str, caret: usize) -> Mirror {
        let carets = text
            .char_indices()
            .map(|(index, _)| index)
            .chain([text.len()])
            .enumerate()
            .map(|(column, offset)| stop(offset, 0, column))
            .collect();
        let mut mirror = Mirror::new();
        mirror.replace(TextSnapshot {
            text: text.to_owned(),
            selection: TextSelection::caret(caret),
            composition: None,
            bounds: Rect::new(
                Point::new(CssPx(0.0), CssPx(0.0)),
                Size::new(CssPx(200.0), CssPx(20.0)),
            ),
            carets,
        });
        mirror
    }

    /// "abc" wrapped after "ab" onto a second line.
    fn wrapped() -> Mirror {
        let mut mirror = Mirror::new();
        mirror.replace(TextSnapshot {
            text: "abc".to_owned(),
            selection: TextSelection::caret(0),
            composition: None,
            bounds: Rect::new(
                Point::new(CssPx(0.0), CssPx(0.0)),
                Size::new(CssPx(20.0), CssPx(40.0)),
            ),
            carets: vec![
                stop(0, 0, 0),
                stop(1, 0, 1),
                stop(2, 0, 2),
                stop(2, 1, 0),
                stop(3, 1, 1),
            ],
        });
        mirror
    }

    fn commit(text: &str) -> SurfaceEvent {
        SurfaceEvent::Ime(ImeEvent::Commit(text.into()))
    }

    fn select(anchor: usize, focus: usize) -> SurfaceEvent {
        SurfaceEvent::SelectText(TextSelection::new(anchor, focus))
    }

    fn keys(events: &[SurfaceEvent]) -> Vec<(KeyState, NamedKey)> {
        events
            .iter()
            .filter_map(|event| match event {
                SurfaceEvent::Key { state, event, .. } => match event.key {
                    zgui_vocab::Key::Named(named) => Some((*state, named)),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn rendered(events: &[SurfaceEvent]) -> Vec<String> {
        events.iter().map(|event| format!("{event:?}")).collect()
    }

    #[test]
    fn utf16_counts_cross_characters_of_every_width() {
        // "a", "é" (two bytes, one unit), "😀" (four bytes, two units), "b".
        let mirror = one_line("a\u{e9}\u{1F600}b", 0);
        assert_eq!(mirror.utf16_between(0, mirror.len()), 5);
        assert_eq!(mirror.utf16_between(mirror.len(), 0), -5);
        assert_eq!(mirror.offset_by_utf16(0, 2), Some(3));
        assert_eq!(mirror.offset_by_utf16(3, 2), Some(7));
        assert_eq!(
            mirror.offset_by_utf16(3, 1),
            Some(7),
            "half an emoji ends after it"
        );
        assert_eq!(mirror.offset_by_utf16(8, -3), Some(3));
        assert_eq!(mirror.offset_by_utf16(0, 6), None);
        assert_eq!(mirror.offset_by_utf16(1, -2), None);
        assert_eq!(
            mirror.clamp(5),
            3,
            "an offset inside a character moves to its start"
        );
    }

    #[test]
    fn typed_text_replaces_the_selection_and_commits() {
        let mut mirror = one_line("abcd", 1);
        mirror.select(1..3);
        let events = mirror.insert("X");
        assert_eq!(rendered(&events), rendered(&[commit("X")]));
        assert_eq!(mirror.text(), "aXd");
        assert_eq!(mirror.selection(), 2..2);
    }

    #[test]
    fn the_return_key_presses_enter_and_leaves_the_text_to_the_field() {
        let mut mirror = one_line("pw", 2);
        let events = mirror.insert("\n");
        assert_eq!(
            keys(&events),
            vec![
                (KeyState::Pressed, NamedKey::Enter),
                (KeyState::Released, NamedKey::Enter)
            ]
        );
        assert_eq!(events.len(), 2);
        assert_eq!(mirror.text(), "pw");
    }

    #[test]
    fn the_return_key_keeps_a_composition_before_it_presses_enter() {
        let mut mirror = one_line("", 0);
        mirror.set_marked("\u{306b}", 0..1);
        let events = mirror.insert("\n");
        assert_eq!(rendered(&events[..1]), rendered(&[commit("\u{306b}")]));
        assert_eq!(keys(&events).len(), 2);
        assert_eq!(mirror.marked(), None);
        assert_eq!(mirror.text(), "\u{306b}");
    }

    #[test]
    fn a_composition_starts_updates_and_commits() {
        let mut mirror = one_line("ab", 1);
        let events = mirror.set_marked("\u{306b}", 1..1);
        assert_eq!(
            rendered(&events),
            rendered(&[
                SurfaceEvent::Ime(ImeEvent::Enabled),
                SurfaceEvent::Ime(ImeEvent::Preedit {
                    text: "\u{306b}".into(),
                    cursor: Some(3..3),
                }),
            ])
        );
        assert_eq!(mirror.text(), "a\u{306b}b");
        assert_eq!(mirror.marked(), Some(1..4));
        assert_eq!(mirror.selection(), 4..4);

        let events = mirror.set_marked("\u{65e5}\u{672c}", 0..2);
        assert_eq!(events.len(), 1, "the composition only started once");
        assert_eq!(mirror.text(), "a\u{65e5}\u{672c}b");
        assert_eq!(mirror.selection(), 1..7);

        let events = mirror.unmark();
        assert_eq!(rendered(&events), rendered(&[commit("\u{65e5}\u{672c}")]));
        assert_eq!(mirror.marked(), None);
        assert_eq!(mirror.selection(), 7..7);
    }

    #[test]
    fn emptying_a_composition_removes_what_it_showed() {
        let mut mirror = one_line("ab", 2);
        mirror.set_marked("x", 1..1);
        let events = mirror.set_marked("", 0..0);
        assert_eq!(rendered(&events), rendered(&[commit("")]));
        assert_eq!(mirror.text(), "ab");
        assert_eq!(mirror.marked(), None);
    }

    #[test]
    fn delete_backward_takes_the_character_before_the_caret() {
        let mut mirror = one_line("a\u{1F600}", 5);
        let events = mirror.delete_backward();
        assert_eq!(
            keys(&events),
            vec![
                (KeyState::Pressed, NamedKey::Backspace),
                (KeyState::Released, NamedKey::Backspace)
            ]
        );
        assert_eq!(mirror.text(), "a");
        assert_eq!(mirror.selection(), 1..1);
    }

    #[test]
    fn a_replaced_range_is_selected_committed_and_the_caret_put_back() {
        // An autocorrection of the first word while the caret is at the end.
        let mut mirror = one_line("teh cat", 7);
        let events = mirror.replace_range(0..3, "the");
        assert_eq!(
            rendered(&events),
            rendered(&[select(0, 3), commit("the"), select(7, 7)])
        );
        assert_eq!(mirror.text(), "the cat");
        assert_eq!(mirror.selection(), 7..7);

        // The caret right after the replaced word needs no second selection.
        let mut mirror = one_line("teh", 3);
        let events = mirror.replace_range(0..3, "the");
        assert_eq!(rendered(&events), rendered(&[select(0, 3), commit("the")]));
    }

    #[test]
    fn a_selection_is_sent_once() {
        let mut mirror = one_line("abc", 0);
        assert_eq!(rendered(&mirror.select(1..2)), rendered(&[select(1, 2)]));
        assert!(mirror.select(1..2).is_empty());
    }

    #[test]
    fn a_snapshot_tells_what_it_changed() {
        let mut mirror = one_line("abc", 0);
        let mut snapshot = mirror.state.clone();
        snapshot.selection = TextSelection::caret(3);
        assert_eq!(
            mirror.replace(snapshot.clone()),
            Changed {
                text: false,
                selection: true
            }
        );
        snapshot.text = "abcd".to_owned();
        assert_eq!(
            mirror.replace(snapshot),
            Changed {
                text: true,
                selection: false
            }
        );
    }

    #[test]
    fn a_caret_at_a_wrap_is_drawn_on_the_later_line() {
        let mirror = wrapped();
        assert_eq!(mirror.caret_rect(2).top(), CssPx(20.0));
        assert_eq!(mirror.caret_rect(1).left(), CssPx(10.0));
        assert_eq!(mirror.bounds().height(), CssPx(40.0));
    }

    #[test]
    fn a_selection_over_a_wrap_has_one_rectangle_per_line() {
        let mirror = wrapped();
        let rects = mirror.selection_rects(1..3);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].rect.left(), CssPx(10.0));
        assert_eq!(rects[0].rect.width(), CssPx(10.0));
        assert!(rects[0].contains_start && !rects[0].contains_end);
        assert_eq!(rects[1].rect.top(), CssPx(20.0));
        assert!(rects[1].contains_end);
        assert_eq!(mirror.first_rect(1..3), rects[0].rect);
        assert_eq!(mirror.first_rect(1..1), mirror.caret_rect(1));

        // A selection that ends at the wrap does not reach the second line.
        assert_eq!(mirror.selection_rects(0..2).len(), 1);
        assert!(mirror.selection_rects(1..1).is_empty());
    }

    #[test]
    fn a_point_finds_the_nearest_caret_on_its_line() {
        let mirror = wrapped();
        assert_eq!(mirror.closest(Point::new(CssPx(14.0), CssPx(5.0)), None), 1);
        assert_eq!(mirror.closest(Point::new(CssPx(16.0), CssPx(5.0)), None), 2);
        assert_eq!(mirror.closest(Point::new(CssPx(9.0), CssPx(25.0)), None), 3);
        assert_eq!(
            mirror.closest(Point::new(CssPx(0.0), CssPx(500.0)), None),
            2
        );
        assert_eq!(
            mirror.closest(Point::new(CssPx(0.0), CssPx(5.0)), Some(1..3)),
            1,
            "a point outside the range takes the range's nearest caret"
        );
        assert_eq!(
            mirror.character_at(Point::new(CssPx(1.0), CssPx(1.0))),
            0..1
        );
    }

    #[test]
    fn steps_move_by_character_and_by_line() {
        let mirror = wrapped();
        assert_eq!(mirror.step(0, Direction::Forward, 2), Some(2));
        assert_eq!(mirror.step(3, Direction::Forward, 1), None);
        assert_eq!(mirror.step(1, Direction::Backward, 1), Some(0));
        assert_eq!(mirror.step(0, Direction::Backward, 1), None);
        assert_eq!(mirror.step(1, Direction::Down, 1), Some(3));
        assert_eq!(mirror.step(3, Direction::Up, 1), Some(1));
        assert_eq!(mirror.step(1, Direction::Up, 1), Some(0));
        assert_eq!(mirror.step(3, Direction::Down, 1), Some(3));
    }

    #[test]
    fn an_edit_moves_the_carets_after_it() {
        let mut mirror = one_line("ab", 1);
        mirror.insert("XY");
        let offsets: Vec<_> = mirror.state.carets.iter().map(|stop| stop.offset).collect();
        assert_eq!(offsets, vec![0, 1, 4]);
        assert_eq!(
            mirror.caret_rect(3).left(),
            CssPx(10.0),
            "a new offset takes the caret before it"
        );
    }
}
