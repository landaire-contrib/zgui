//! The UIKit view that edits the focused field's text through `UITextInput`.
//!
//! The view is a subview of the view winit draws into. While a field is focused, the view covers
//! the field's text box and is the first responder. So the keyboard, the gestures of
//! `UITextInteraction` and the edit menu act on it. A touch that no text gesture takes goes up
//! the responder chain to winit's view. While no field is focused, the view is hidden.
//!
//! UIKit reads and edits the text through a [`Mirror`]. Each edit goes to the window as surface
//! events, and the window's next snapshot replaces the copy.

use core::cell::{Cell, OnceCell, RefCell};
use core::ops::Range;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_foundation::{
    NSArray, NSAttributedStringKey, NSComparisonResult, NSDictionary, NSInteger, NSObject,
    NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize, NSString,
};
use objc2_ui_kit::{
    NSWritingDirection, UIKeyInput, UIKeyboardType, UIPasteboard, UIResponder, UIReturnKeyType,
    UITextAutocapitalizationType, UITextAutocorrectionType, UITextContentType,
    UITextContentTypeEmailAddress, UITextContentTypeName, UITextContentTypeNewPassword,
    UITextContentTypeOneTimeCode, UITextContentTypePassword, UITextContentTypeTelephoneNumber,
    UITextContentTypeURL, UITextContentTypeUsername, UITextGranularity, UITextInput,
    UITextInputDelegate, UITextInputStringTokenizer, UITextInputTokenizer, UITextInputTraits,
    UITextInteraction, UITextInteractionMode, UITextLayoutDirection, UITextPosition, UITextRange,
    UITextSelectionRect, UITextSmartDashesType, UITextSmartQuotesType, UITextSpellCheckingType,
    UITextStorageDirection, UIView,
};
use zgui_geom::{Css, CssPx, Point, Rect};
use zgui_platform::{
    Autocapitalize, Surface, SurfaceEvent, SurfaceId, TextContentType, TextInput, TextSnapshot,
};

use crate::input::keyboard_traits::{KeyboardTraits, Keys, ReturnKey};
use crate::input::text_focus::{Keyboard, Step};
use crate::input::text_mirror::{Changed, Direction, Mirror, SelectionRect};
use crate::surface::WinitSurface;

thread_local! {
    /// The text view of each surface, removed with it.
    static VIEWS: RefCell<HashMap<SurfaceId, Retained<TextView>>> = RefCell::new(HashMap::new());
}

/// What the text view of one surface holds.
struct TextViewState {
    /// The surface the edits are reported to.
    surface: Weak<WinitSurface>,
    /// The copy of the focused field's text.
    mirror: RefCell<Mirror>,
    /// The keyboard of the focused field, and nothing while no field is focused.
    traits: Cell<Option<KeyboardTraits>>,
    /// The session the view takes part in, and when it takes the keyboard.
    keyboard: Cell<Keyboard>,
    /// How many edits were reported and not yet delivered to the window.
    ///
    /// A snapshot that arrives while edits are on their way was made before them, so it is
    /// dropped. The frame after the delivery makes a snapshot that includes them.
    in_flight: Cell<usize>,
    /// The object UIKit told about changes the window made, held weakly as UIKit asks.
    delegate: RefCell<Option<objc2::rc::Weak<ProtocolObject<dyn UITextInputDelegate>>>>,
    /// The tokenizer, made when UIKit first asks for it.
    tokenizer: OnceCell<Retained<UITextInputStringTokenizer>>,
    /// The style UIKit asked provisional text to be drawn in.
    marked_style: RefCell<Option<Retained<NSDictionary<NSAttributedStringKey, AnyObject>>>>,
}

define_class!(
    // SAFETY: UITextPosition may be subclassed, and `Position` does not implement `Drop`.
    #[unsafe(super(UITextPosition, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ZguiTextPosition"]
    #[ivars = usize]
    struct Position;
);

impl Position {
    /// A position at byte `offset` of the text.
    fn from_offset(main: MainThreadMarker, offset: usize) -> Retained<UITextPosition> {
        let this = Self::alloc(main).set_ivars(offset);
        // SAFETY: `init` is NSObject's designated initializer, and the ivars are set.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.into_super()
    }
}

define_class!(
    // SAFETY: UITextRange may be subclassed, and `TextRange` does not implement `Drop`.
    #[unsafe(super(UITextRange, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ZguiTextRange"]
    #[ivars = Range<usize>]
    struct TextRange;

    impl TextRange {
        #[unsafe(method(isEmpty))]
        fn is_empty(&self) -> bool {
            self.ivars().is_empty()
        }

        #[unsafe(method_id(start))]
        fn start(&self) -> Retained<UITextPosition> {
            Position::from_offset(self.mtm(), self.ivars().start)
        }

        #[unsafe(method_id(end))]
        fn end(&self) -> Retained<UITextPosition> {
            Position::from_offset(self.mtm(), self.ivars().end)
        }
    }
);

impl TextRange {
    /// A range over the bytes `range` of the text.
    fn from_range(main: MainThreadMarker, range: Range<usize>) -> Retained<UITextRange> {
        let this = Self::alloc(main).set_ivars(range);
        // SAFETY: `init` is NSObject's designated initializer, and the ivars are set.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.into_super()
    }
}

/// A selection rectangle in the text view's own coordinates.
#[derive(Clone, Copy)]
struct LocalSelectionRect {
    /// The rectangle.
    rect: NSRect,
    /// Whether the selection starts in it.
    contains_start: bool,
    /// Whether the selection ends in it.
    contains_end: bool,
}

define_class!(
    // SAFETY: UITextSelectionRect may be subclassed, and `SelectionRectObject` does not implement
    // `Drop`.
    #[unsafe(super(UITextSelectionRect, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ZguiTextSelectionRect"]
    #[ivars = LocalSelectionRect]
    struct SelectionRectObject;

    impl SelectionRectObject {
        #[unsafe(method(rect))]
        fn rect(&self) -> NSRect {
            self.ivars().rect
        }

        #[unsafe(method(writingDirection))]
        fn writing_direction(&self) -> NSWritingDirection {
            NSWritingDirection::LeftToRight
        }

        #[unsafe(method(containsStart))]
        fn contains_start(&self) -> bool {
            self.ivars().contains_start
        }

        #[unsafe(method(containsEnd))]
        fn contains_end(&self) -> bool {
            self.ivars().contains_end
        }

        #[unsafe(method(isVertical))]
        fn is_vertical(&self) -> bool {
            false
        }
    }
);

impl SelectionRectObject {
    /// An object for one rectangle.
    fn from_rect(
        main: MainThreadMarker,
        rect: LocalSelectionRect,
    ) -> Retained<UITextSelectionRect> {
        let this = Self::alloc(main).set_ivars(rect);
        // SAFETY: `init` is NSObject's designated initializer, and the ivars are set.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.into_super()
    }
}

define_class!(
    // SAFETY: UIView may be subclassed, and `TextView` does not implement `Drop`.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ZguiTextView"]
    #[ivars = TextViewState]
    struct TextView;

    impl TextView {
        #[unsafe(method(canBecomeFirstResponder))]
        fn can_become_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(canPerformAction:withSender:))]
        fn can_perform_action(&self, action: Sel, _sender: Option<&AnyObject>) -> bool {
            self.performs(action)
        }

        #[unsafe(method(copy:))]
        fn copy(&self, _sender: Option<&AnyObject>) {
            self.copy_selection();
        }

        #[unsafe(method(cut:))]
        fn cut(&self, _sender: Option<&AnyObject>) {
            if self.copy_selection() {
                let events = self.ivars().mirror.borrow_mut().delete_backward();
                self.report(events);
            }
        }

        #[unsafe(method(paste:))]
        fn paste(&self, _sender: Option<&AnyObject>) {
            // SAFETY: reading the pasteboard's text has no requirements.
            let Some(text) = (unsafe { UIPasteboard::generalPasteboard().string() }) else {
                return;
            };
            let text = text.to_string();
            let multi_line = self.traits().is_some_and(|traits| traits.multi_line);
            // A single-line field joins the lines, as a paste from the keyboard does.
            let text = if multi_line {
                text
            } else {
                text.split(['\r', '\n'])
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            if text.is_empty() {
                return;
            }
            let events = self.ivars().mirror.borrow_mut().insert(&text);
            self.report(events);
        }

        #[unsafe(method(select:))]
        fn select(&self, _sender: Option<&AnyObject>) {
            let caret = self.ivars().mirror.borrow().selection().start;
            let position = Position::from_offset(self.mtm(), caret);
            let tokenizer = self.tokenizer_object();
            let word = tokenizer
                .rangeEnclosingPosition_withGranularity_inDirection(
                    &position,
                    UITextGranularity::Word,
                    UITextStorageDirection::Backward.0,
                )
                .or_else(|| {
                    tokenizer.rangeEnclosingPosition_withGranularity_inDirection(
                        &position,
                        UITextGranularity::Word,
                        UITextStorageDirection::Forward.0,
                    )
                });
            if let Some(range) = word.as_deref().and_then(range_of) {
                let events = self.ivars().mirror.borrow_mut().select(range);
                self.report(events);
            }
        }

        #[unsafe(method(selectAll:))]
        fn select_all(&self, _sender: Option<&AnyObject>) {
            let events = {
                let mut mirror = self.ivars().mirror.borrow_mut();
                let end = mirror.len();
                mirror.select(0..end)
            };
            self.report(events);
        }
    }

    unsafe impl NSObjectProtocol for TextView {}

    unsafe impl UITextInputTraits for TextView {
        #[unsafe(method(keyboardType))]
        fn keyboard_type(&self) -> UIKeyboardType {
            match self.traits().map(|traits| traits.keys) {
                Some(Keys::Decimal) => UIKeyboardType::DecimalPad,
                Some(Keys::Digits) => UIKeyboardType::NumberPad,
                Some(Keys::Phone) => UIKeyboardType::PhonePad,
                Some(Keys::Url) => UIKeyboardType::URL,
                Some(Keys::Email) => UIKeyboardType::EmailAddress,
                Some(Keys::Search) => UIKeyboardType::WebSearch,
                Some(Keys::Letters) | None => UIKeyboardType::Default,
            }
        }

        #[unsafe(method(returnKeyType))]
        fn return_key_type(&self) -> UIReturnKeyType {
            match self.traits().map(|traits| traits.return_key) {
                Some(ReturnKey::Done) => UIReturnKeyType::Done,
                Some(ReturnKey::Go) => UIReturnKeyType::Go,
                Some(ReturnKey::Next) => UIReturnKeyType::Next,
                Some(ReturnKey::Search) => UIReturnKeyType::Search,
                Some(ReturnKey::Send) => UIReturnKeyType::Send,
                Some(ReturnKey::Return) | None => UIReturnKeyType::Default,
            }
        }

        #[unsafe(method(isSecureTextEntry))]
        fn is_secure_text_entry(&self) -> bool {
            self.traits().is_some_and(|traits| traits.secure)
        }

        #[unsafe(method(autocorrectionType))]
        fn autocorrection_type(&self) -> UITextAutocorrectionType {
            match self.traits().and_then(|traits| traits.autocorrect) {
                Some(true) => UITextAutocorrectionType::Yes,
                Some(false) => UITextAutocorrectionType::No,
                None => UITextAutocorrectionType::Default,
            }
        }

        #[unsafe(method(spellCheckingType))]
        fn spell_checking_type(&self) -> UITextSpellCheckingType {
            if self.smart_punctuation() {
                UITextSpellCheckingType::Default
            } else {
                UITextSpellCheckingType::No
            }
        }

        #[unsafe(method(smartQuotesType))]
        fn smart_quotes_type(&self) -> UITextSmartQuotesType {
            if self.smart_punctuation() {
                UITextSmartQuotesType::Default
            } else {
                UITextSmartQuotesType::No
            }
        }

        #[unsafe(method(smartDashesType))]
        fn smart_dashes_type(&self) -> UITextSmartDashesType {
            if self.smart_punctuation() {
                UITextSmartDashesType::Default
            } else {
                UITextSmartDashesType::No
            }
        }

        #[unsafe(method(autocapitalizationType))]
        fn autocapitalization_type(&self) -> UITextAutocapitalizationType {
            match self.traits().map(|traits| traits.autocapitalize) {
                Some(Autocapitalize::Words) => UITextAutocapitalizationType::Words,
                Some(Autocapitalize::Characters) => UITextAutocapitalizationType::AllCharacters,
                Some(Autocapitalize::Sentences) | None => UITextAutocapitalizationType::Sentences,
                Some(_) => UITextAutocapitalizationType::None,
            }
        }

        #[unsafe(method_id(textContentType))]
        fn text_content_type(&self) -> Option<Retained<UITextContentType>> {
            self.content_type_name()
        }
    }

    unsafe impl UIKeyInput for TextView {
        #[unsafe(method(hasText))]
        fn has_text(&self) -> bool {
            self.ivars().mirror.borrow().len() > 0
        }

        #[unsafe(method(insertText:))]
        fn insert_text(&self, text: &NSString) {
            let text = text.to_string();
            if matches!(text.as_str(), "\n" | "\r" | "\r\n") {
                // The keyboard counts the line break as text, and the field takes it as Enter.
                let mut keyboard = self.ivars().keyboard.get();
                keyboard.returned();
                self.ivars().keyboard.set(keyboard);
            }
            let events = self.ivars().mirror.borrow_mut().insert(&text);
            self.report(events);
        }

        #[unsafe(method(deleteBackward))]
        fn delete_backward(&self) {
            let events = self.ivars().mirror.borrow_mut().delete_backward();
            self.report(events);
        }
    }

    unsafe impl UITextInput for TextView {
        #[unsafe(method_id(textInRange:))]
        fn text_in_range(&self, range: &UITextRange) -> Option<Retained<NSString>> {
            range_of(range).map(|range| NSString::from_str(self.ivars().mirror.borrow().slice(range)))
        }

        #[unsafe(method(replaceRange:withText:))]
        fn replace_range(&self, range: &UITextRange, text: &NSString) {
            let Some(range) = range_of(range) else {
                return;
            };
            let events = self
                .ivars()
                .mirror
                .borrow_mut()
                .replace_range(range, &text.to_string());
            self.report(events);
        }

        #[unsafe(method_id(selectedTextRange))]
        fn selected_text_range(&self) -> Option<Retained<UITextRange>> {
            let selection = self.ivars().mirror.borrow().selection();
            Some(TextRange::from_range(self.mtm(), selection))
        }

        #[unsafe(method(setSelectedTextRange:))]
        fn set_selected_text_range(&self, range: Option<&UITextRange>) {
            let Some(range) = range.and_then(range_of) else {
                return;
            };
            let events = self.ivars().mirror.borrow_mut().select(range);
            self.report(events);
        }

        #[unsafe(method_id(markedTextRange))]
        fn marked_text_range(&self) -> Option<Retained<UITextRange>> {
            let marked = self.ivars().mirror.borrow().marked();
            marked.map(|marked| TextRange::from_range(self.mtm(), marked))
        }

        #[unsafe(method_id(markedTextStyle))]
        fn marked_text_style(
            &self,
        ) -> Option<Retained<NSDictionary<NSAttributedStringKey, AnyObject>>> {
            self.ivars().marked_style.borrow().clone()
        }

        #[unsafe(method(setMarkedTextStyle:))]
        fn set_marked_text_style(
            &self,
            style: Option<&NSDictionary<NSAttributedStringKey, AnyObject>>,
        ) {
            *self.ivars().marked_style.borrow_mut() = style.map(Message::retain);
        }

        #[unsafe(method(setMarkedText:selectedRange:))]
        fn set_marked_text(&self, text: Option<&NSString>, selected: NSRange) {
            let text = text.map(NSString::to_string).unwrap_or_default();
            let selected = selected.location..selected.location + selected.length;
            let events = self.ivars().mirror.borrow_mut().set_marked(&text, selected);
            self.report(events);
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            let events = self.ivars().mirror.borrow_mut().unmark();
            self.report(events);
        }

        #[unsafe(method_id(beginningOfDocument))]
        fn beginning_of_document(&self) -> Retained<UITextPosition> {
            Position::from_offset(self.mtm(), 0)
        }

        #[unsafe(method_id(endOfDocument))]
        fn end_of_document(&self) -> Retained<UITextPosition> {
            Position::from_offset(self.mtm(), self.ivars().mirror.borrow().len())
        }

        #[unsafe(method_id(textRangeFromPosition:toPosition:))]
        fn text_range_from_position(
            &self,
            from: &UITextPosition,
            to: &UITextPosition,
        ) -> Option<Retained<UITextRange>> {
            offset_of(from)
                .zip(offset_of(to))
                .map(|(from, to)| TextRange::from_range(self.mtm(), from.min(to)..from.max(to)))
        }

        #[unsafe(method_id(positionFromPosition:offset:))]
        fn position_from_position(
            &self,
            position: &UITextPosition,
            offset: NSInteger,
        ) -> Option<Retained<UITextPosition>> {
            let moved = offset_of(position)
                .and_then(|from| self.ivars().mirror.borrow().offset_by_utf16(from, offset));
            moved.map(|offset| Position::from_offset(self.mtm(), offset))
        }

        #[unsafe(method_id(positionFromPosition:inDirection:offset:))]
        fn position_in_direction(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
            offset: NSInteger,
        ) -> Option<Retained<UITextPosition>> {
            let direction = direction_of(direction, offset < 0);
            let moved = offset_of(position).and_then(|from| {
                self.ivars()
                    .mirror
                    .borrow()
                    .step(from, direction, offset.unsigned_abs())
            });
            moved.map(|offset| Position::from_offset(self.mtm(), offset))
        }

        #[unsafe(method(comparePosition:toPosition:))]
        fn compare_position(
            &self,
            position: &UITextPosition,
            other: &UITextPosition,
        ) -> NSComparisonResult {
            let (first, second) = (offset_of(position), offset_of(other));
            match first.cmp(&second) {
                core::cmp::Ordering::Less => NSComparisonResult::Ascending,
                core::cmp::Ordering::Equal => NSComparisonResult::Same,
                core::cmp::Ordering::Greater => NSComparisonResult::Descending,
            }
        }

        #[unsafe(method(offsetFromPosition:toPosition:))]
        fn offset_from_position(&self, from: &UITextPosition, to: &UITextPosition) -> NSInteger {
            let (Some(from), Some(to)) = (offset_of(from), offset_of(to)) else {
                return 0;
            };
            self.ivars().mirror.borrow().utf16_between(from, to)
        }

        #[unsafe(method_id(inputDelegate))]
        fn input_delegate(&self) -> Option<Retained<ProtocolObject<dyn UITextInputDelegate>>> {
            self.delegate()
        }

        #[unsafe(method(setInputDelegate:))]
        fn set_input_delegate(&self, delegate: Option<&ProtocolObject<dyn UITextInputDelegate>>) {
            *self.ivars().delegate.borrow_mut() = delegate.map(objc2::rc::Weak::new);
        }

        #[unsafe(method_id(tokenizer))]
        fn tokenizer(&self) -> Retained<ProtocolObject<dyn UITextInputTokenizer>> {
            self.tokenizer_object()
        }

        #[unsafe(method_id(positionWithinRange:farthestInDirection:))]
        fn position_within_range(
            &self,
            range: &UITextRange,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextPosition>> {
            let far = direction_of(direction, false);
            range_of(range).map(|range| {
                let offset = match far {
                    Direction::Backward | Direction::Up => range.start,
                    Direction::Forward | Direction::Down => range.end,
                };
                Position::from_offset(self.mtm(), offset)
            })
        }

        #[unsafe(method_id(characterRangeByExtendingPosition:inDirection:))]
        fn character_range_by_extending(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextRange>> {
            let direction = direction_of(direction, false);
            let ends = offset_of(position).and_then(|from| {
                let other = self.ivars().mirror.borrow().step(from, direction, 1)?;
                Some(from.min(other)..from.max(other))
            });
            ends.map(|range| TextRange::from_range(self.mtm(), range))
        }

        #[unsafe(method(baseWritingDirectionForPosition:inDirection:))]
        fn base_writing_direction(
            &self,
            _position: &UITextPosition,
            _direction: UITextStorageDirection,
        ) -> NSWritingDirection {
            NSWritingDirection::LeftToRight
        }

        #[unsafe(method(setBaseWritingDirection:forRange:))]
        fn set_base_writing_direction(&self, _direction: NSWritingDirection, _range: &UITextRange) {
            // The field's style decides the direction of its text.
        }

        #[unsafe(method(firstRectForRange:))]
        fn first_rect_for_range(&self, range: &UITextRange) -> NSRect {
            let mirror = self.ivars().mirror.borrow();
            let range = range_of(range).unwrap_or(0..0);
            local_rect(mirror.first_rect(range), mirror.bounds())
        }

        #[unsafe(method(caretRectForPosition:))]
        fn caret_rect_for_position(&self, position: &UITextPosition) -> NSRect {
            let mirror = self.ivars().mirror.borrow();
            let offset = offset_of(position).unwrap_or(0);
            local_rect(mirror.caret_rect(offset), mirror.bounds())
        }

        #[unsafe(method_id(selectionRectsForRange:))]
        fn selection_rects_for_range(
            &self,
            range: &UITextRange,
        ) -> Retained<NSArray<UITextSelectionRect>> {
            let rects: Vec<SelectionRect> = {
                let mirror = self.ivars().mirror.borrow();
                range_of(range).map_or_else(Vec::new, |range| mirror.selection_rects(range))
            };
            let bounds = self.ivars().mirror.borrow().bounds();
            let objects: Vec<Retained<UITextSelectionRect>> = rects
                .into_iter()
                .map(|rect| {
                    SelectionRectObject::from_rect(
                        self.mtm(),
                        LocalSelectionRect {
                            rect: local_rect(rect.rect, bounds),
                            contains_start: rect.contains_start,
                            contains_end: rect.contains_end,
                        },
                    )
                })
                .collect();
            NSArray::from_retained_slice(&objects)
        }

        #[unsafe(method_id(closestPositionToPoint:))]
        fn closest_position(&self, point: NSPoint) -> Option<Retained<UITextPosition>> {
            let mirror = self.ivars().mirror.borrow();
            let offset = mirror.closest(surface_point(point, mirror.bounds()), None);
            Some(Position::from_offset(self.mtm(), offset))
        }

        #[unsafe(method_id(closestPositionToPoint:withinRange:))]
        fn closest_position_within(
            &self,
            point: NSPoint,
            range: &UITextRange,
        ) -> Option<Retained<UITextPosition>> {
            let mirror = self.ivars().mirror.borrow();
            let within = range_of(range);
            let offset = mirror.closest(surface_point(point, mirror.bounds()), within);
            Some(Position::from_offset(self.mtm(), offset))
        }

        #[unsafe(method_id(characterRangeAtPoint:))]
        fn character_range_at_point(&self, point: NSPoint) -> Option<Retained<UITextRange>> {
            let mirror = self.ivars().mirror.borrow();
            let range = mirror.character_at(surface_point(point, mirror.bounds()));
            Some(TextRange::from_range(self.mtm(), range))
        }

        #[unsafe(method_id(textInputView))]
        fn text_input_view(&self) -> Retained<UIView> {
            self.retain().into_super()
        }

        #[unsafe(method(selectionAffinity))]
        fn selection_affinity(&self) -> UITextStorageDirection {
            UITextStorageDirection::Forward
        }

        #[unsafe(method(setSelectionAffinity:))]
        fn set_selection_affinity(&self, _affinity: UITextStorageDirection) {
            // A caret at a line wrap is drawn on the later line.
        }
    }
);

impl TextView {
    /// A hidden text view that reports edits to `surface`, with the text interactions of an
    /// editable text.
    fn new(main: MainThreadMarker, surface: Weak<WinitSurface>) -> Retained<Self> {
        let this = Self::alloc(main).set_ivars(TextViewState {
            surface,
            mirror: RefCell::new(Mirror::new()),
            traits: Cell::new(None),
            keyboard: Cell::new(Keyboard::new()),
            in_flight: Cell::new(0),
            delegate: RefCell::new(None),
            tokenizer: OnceCell::new(),
            marked_style: RefCell::new(None),
        });
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
        // SAFETY: `initWithFrame:` is UIView's designated initializer, and the ivars are set.
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        this.setHidden(true);
        this.setOpaque(false);

        let interaction =
            UITextInteraction::textInteractionForMode(UITextInteractionMode::Editable, main);
        // SAFETY: the view conforms to `UITextInput`, which the interaction needs of its input.
        unsafe { interaction.setTextInput(Some(&this)) };
        this.addInteraction(ProtocolObject::from_ref(&*interaction));

        this
    }

    /// Whether an action of the edit menu applies to the text now.
    fn performs(&self, action: Sel) -> bool {
        let secure = self.traits().is_some_and(|traits| traits.secure);
        let (selected, empty, all) = {
            let mirror = self.ivars().mirror.borrow();
            let selection = mirror.selection();
            (
                !selection.is_empty(),
                mirror.len() == 0,
                selection == (0..mirror.len()),
            )
        };
        if action == sel!(copy:) || action == sel!(cut:) {
            return selected && !secure;
        }
        if action == sel!(paste:) {
            // SAFETY: reading whether the pasteboard holds text has no requirements.
            return unsafe { UIPasteboard::generalPasteboard().hasStrings() };
        }
        if action == sel!(select:) {
            return !selected && !empty && !secure;
        }
        if action == sel!(selectAll:) {
            return !empty && !all;
        }

        false
    }

    /// UIKit's name for what the focused field's value is.
    fn content_type_name(&self) -> Option<Retained<UITextContentType>> {
        let content = self.traits()?.content_type?;
        // SAFETY: the names are UIKit's own constants.
        let name: &UITextContentType = unsafe {
            match content {
                TextContentType::Username => UITextContentTypeUsername,
                TextContentType::Password => UITextContentTypePassword,
                TextContentType::NewPassword => UITextContentTypeNewPassword,
                TextContentType::OneTimeCode => UITextContentTypeOneTimeCode,
                TextContentType::Email => UITextContentTypeEmailAddress,
                TextContentType::Name => UITextContentTypeName,
                TextContentType::Telephone => UITextContentTypeTelephoneNumber,
                TextContentType::Url => UITextContentTypeURL,
                _ => return None,
            }
        };

        Some(name.retain())
    }

    /// The keyboard of the focused field.
    fn traits(&self) -> Option<KeyboardTraits> {
        self.ivars().traits.get()
    }

    /// Whether the keyboard checks spelling and puts in smart punctuation.
    fn smart_punctuation(&self) -> bool {
        self.traits().is_some_and(KeyboardTraits::smart_punctuation)
    }

    /// The object UIKit told about changes the window made, while it lives.
    fn delegate(&self) -> Option<Retained<ProtocolObject<dyn UITextInputDelegate>>> {
        self.ivars()
            .delegate
            .borrow()
            .as_ref()
            .and_then(objc2::rc::Weak::load)
    }

    /// The tokenizer UIKit finds words and sentences with.
    fn tokenizer_object(&self) -> Retained<ProtocolObject<dyn UITextInputTokenizer>> {
        let tokenizer = self.ivars().tokenizer.get_or_init(|| {
            // SAFETY: the view conforms to `UITextInput`, which the tokenizer reads the text of.
            unsafe {
                UITextInputStringTokenizer::initWithTextInput(
                    UITextInputStringTokenizer::alloc(self.mtm()),
                    self,
                )
            }
        });
        ProtocolObject::from_retained(Retained::clone(tokenizer))
    }

    /// Puts the selected text on the pasteboard, and answers whether there was any.
    fn copy_selection(&self) -> bool {
        if self.traits().is_some_and(|traits| traits.secure) {
            return false;
        }
        let text = {
            let mirror = self.ivars().mirror.borrow();
            let selection = mirror.selection();
            if selection.is_empty() {
                return false;
            }
            NSString::from_str(mirror.slice(selection))
        };
        // SAFETY: writing text to the pasteboard has no requirements.
        unsafe { UIPasteboard::generalPasteboard().setString(Some(&text)) };

        true
    }

    /// Gives the window the events of one edit.
    fn report(&self, events: Vec<SurfaceEvent>) {
        if events.is_empty() {
            return;
        }
        let Some(surface) = self.ivars().surface.upgrade() else {
            return;
        };
        for event in events {
            let in_flight = &self.ivars().in_flight;
            in_flight.set(in_flight.get() + 1);
            surface.report_typed(event);
        }
        self.hide_native_marks();
    }

    /// Takes the window's snapshot, and tells UIKit what changed.
    ///
    /// The first snapshot of a session replaces the copy before the view becomes the first
    /// responder, so the keyboard reads the text of the field it serves.
    fn take(&self, snapshot: Option<TextSnapshot>) {
        let mut keyboard = self.ivars().keyboard.get();
        let step = keyboard.snapshot();
        self.ivars().keyboard.set(keyboard);

        match step {
            Step::Begin { restart } => {
                if restart && self.isFirstResponder() {
                    self.resignFirstResponder();
                }
                if let Some(snapshot) = snapshot {
                    let bounds = snapshot.bounds;
                    self.ivars().mirror.borrow_mut().replace(snapshot);
                    self.setFrame(frame_of(bounds));
                }
                self.setHidden(false);
                self.becomeFirstResponder();
                self.hide_native_marks();
            }
            Step::Refresh { announce } => {
                if let Some(snapshot) = snapshot {
                    self.refresh(snapshot, announce);
                }
            }
        }
    }

    /// Replaces the copy with a snapshot of the same session, and tells UIKit what changed.
    ///
    /// With `announce`, UIKit is told the text changed even when the copy holds it already.
    fn refresh(&self, snapshot: TextSnapshot, announce: bool) {
        if self.ivars().in_flight.get() > 0 {
            return;
        }
        let changed = self.ivars().mirror.borrow().changes(&snapshot);
        let changed = match (changed, announce) {
            (Some(changed), _) => Changed {
                text: changed.text || announce,
                ..changed
            },
            (None, true) => Changed {
                text: true,
                selection: false,
            },
            (None, false) => return,
        };

        let bounds = snapshot.bounds;
        let delegate = self.delegate();
        let input = ProtocolObject::from_ref(self);
        if let Some(delegate) = &delegate {
            if changed.selection {
                delegate.selectionWillChange(Some(input));
            }
            if changed.text {
                delegate.textWillChange(Some(input));
            }
        }
        self.ivars().mirror.borrow_mut().replace(snapshot);
        self.setFrame(frame_of(bounds));
        if let Some(delegate) = &delegate {
            if changed.text {
                delegate.textDidChange(Some(input));
            }
            if changed.selection {
                delegate.selectionDidChange(Some(input));
            }
        }
        let mut keyboard = self.ivars().keyboard.get();
        keyboard.announced();
        self.ivars().keyboard.set(keyboard);
        self.hide_native_marks();
    }

    /// Hides the caret and the selection highlight UIKit draws, because the window draws its own.
    /// The selection handles and the loupe stay.
    ///
    /// UIKit draws them through a selection display interaction from iOS 17 on.
    fn hide_native_marks(&self) {
        let Some(display) = AnyClass::get(c"UITextSelectionDisplayInteraction") else {
            return;
        };
        for interaction in self.interactions().to_vec() {
            if !interaction.isKindOfClass(display) {
                continue;
            }
            let object: &AnyObject = interaction.as_ref();
            // SAFETY: the interaction is a UITextSelectionDisplayInteraction, whose cursor and
            // highlight views are UIViews.
            let marks: [Option<Retained<UIView>>; 2] = unsafe {
                [
                    msg_send![object, cursorView],
                    msg_send![object, highlightView],
                ]
            };
            for view in marks.into_iter().flatten() {
                view.setHidden(true);
            }
        }
    }
}

/// Makes the text view of `surface`, hidden, inside the view winit draws into.
pub(crate) fn attach(surface: &Arc<WinitSurface>) {
    let Some(main) = MainThreadMarker::new() else {
        return;
    };
    let Some(parent) = super::ui_view(surface.window()) else {
        return;
    };
    let view = TextView::new(main, Arc::downgrade(surface));
    parent.addSubview(&view);
    VIEWS.with(|views| {
        views
            .borrow_mut()
            .insert(Surface::id(surface.as_ref()), view)
    });
}

/// Removes the text view of `surface`.
pub(crate) fn forget(surface: SurfaceId) {
    let Some(view) = VIEWS.with(|views| views.borrow_mut().remove(&surface)) else {
        return;
    };
    if view.isFirstResponder() {
        view.resignFirstResponder();
    }
    view.removeFromSuperview();
}

/// Shows the keyboard of the focused field of `surface`, or hides it when no field is focused.
///
/// The view becomes the first responder when the snapshot of the field arrives, which is in the
/// same frame.
pub(crate) fn set_text_input(surface: SurfaceId, state: Option<TextInput>) {
    let Some(view) = view(surface) else {
        return;
    };
    let Some(state) = state else {
        view.ivars().traits.set(None);
        let mut keyboard = view.ivars().keyboard.get();
        keyboard.blur();
        view.ivars().keyboard.set(keyboard);
        // Every path that leaves no field focused gives the keyboard up, whatever made the view
        // the first responder.
        if view.isFirstResponder() {
            view.resignFirstResponder();
        }
        view.setHidden(true);
        // The next field starts from its own snapshot.
        view.ivars().mirror.replace(Mirror::new());
        return;
    };

    let traits = KeyboardTraits::of(&state);
    let previous = view.ivars().traits.replace(Some(traits));
    let responding = view.isFirstResponder();
    let mut keyboard = view.ivars().keyboard.get();
    keyboard.focus(state.session, responding);
    view.ivars().keyboard.set(keyboard);
    if responding && previous != Some(traits) {
        // UIKit reads the traits when the view becomes the first responder, and again here.
        view.reloadInputViews();
    }
}

/// Gives the text view of `surface` the focused field's snapshot.
pub(crate) fn set_text_snapshot(
    surface: SurfaceId,
    build: &mut dyn FnMut() -> Option<TextSnapshot>,
) {
    let Some(view) = view(surface) else {
        return;
    };
    if view.traits().is_none() {
        return;
    }
    view.take(build());
}

/// Counts one edit of `surface` as delivered to the window.
pub(crate) fn delivered(surface: SurfaceId) {
    let Some(view) = view(surface) else {
        return;
    };
    let in_flight = &view.ivars().in_flight;
    in_flight.set(in_flight.get().saturating_sub(1));
}

/// The text view of `surface`, on the main thread.
fn view(surface: SurfaceId) -> Option<Retained<TextView>> {
    MainThreadMarker::new()?;
    VIEWS.with(|views| views.borrow().get(&surface).cloned())
}

/// The byte offset a position of this view names.
fn offset_of(position: &UITextPosition) -> Option<usize> {
    let object: &AnyObject = position.as_ref();
    object
        .downcast_ref::<Position>()
        .map(|position| *position.ivars())
}

/// The bytes a range of this view names.
fn range_of(range: &UITextRange) -> Option<Range<usize>> {
    let object: &AnyObject = range.as_ref();
    object
        .downcast_ref::<TextRange>()
        .map(|range| range.ivars().clone())
}

/// The mirror's direction for a UIKit direction, turned around for a negative count.
fn direction_of(direction: UITextLayoutDirection, reversed: bool) -> Direction {
    let direction = match direction {
        UITextLayoutDirection::Left => Direction::Backward,
        UITextLayoutDirection::Up => Direction::Up,
        UITextLayoutDirection::Down => Direction::Down,
        _ => Direction::Forward,
    };
    if !reversed {
        return direction;
    }
    match direction {
        Direction::Backward => Direction::Forward,
        Direction::Forward => Direction::Backward,
        Direction::Up => Direction::Down,
        Direction::Down => Direction::Up,
    }
}

/// The frame of a box given in CSS pixels of the surface, in points of winit's view.
fn frame_of(rect: Rect<CssPx, Css>) -> NSRect {
    NSRect::new(
        NSPoint::new(f64::from(rect.origin.x.0), f64::from(rect.origin.y.0)),
        NSSize::new(f64::from(rect.size.width.0), f64::from(rect.size.height.0)),
    )
}

/// A rectangle in CSS pixels of the surface, in the text view's own coordinates.
fn local_rect(rect: Rect<CssPx, Css>, bounds: Rect<CssPx, Css>) -> NSRect {
    NSRect::new(
        NSPoint::new(
            f64::from(rect.origin.x.0 - bounds.origin.x.0),
            f64::from(rect.origin.y.0 - bounds.origin.y.0),
        ),
        NSSize::new(f64::from(rect.size.width.0), f64::from(rect.size.height.0)),
    )
}

/// A point in the text view's own coordinates, in CSS pixels of the surface.
fn surface_point(point: NSPoint, bounds: Rect<CssPx, Css>) -> Point<CssPx, Css> {
    Point::new(
        CssPx(point.x as f32 + bounds.origin.x.0),
        CssPx(point.y as f32 + bounds.origin.y.0),
    )
}
