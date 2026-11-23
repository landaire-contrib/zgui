//! The keyboard a focused field asks the iOS text input for.
//!
//! The choices are made here, apart from UIKit, so they can be checked on any machine.

use zgui_platform::{Autocapitalize, EnterKeyHint, TextContentType, TextInput, TextInputPurpose};

/// Which keys the keyboard shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Keys {
    /// Letters.
    Letters,
    /// Digits and a decimal point.
    Decimal,
    /// Digits only.
    Digits,
    /// A telephone keypad.
    Phone,
    /// Letters, with the characters of a web address.
    Url,
    /// Letters, with the characters of an electronic mail address.
    Email,
    /// Letters, with a key that searches.
    Search,
}

/// What the return key says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReturnKey {
    /// The platform's own label for a line break.
    Return,
    /// "Done".
    Done,
    /// "Go".
    Go,
    /// "Next".
    Next,
    /// "Search".
    Search,
    /// "Send".
    Send,
}

/// Everything about the keyboard of one field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyboardTraits {
    /// Which keys it shows.
    pub(crate) keys: Keys,
    /// What its return key says.
    pub(crate) return_key: ReturnKey,
    /// Whether the text is a secret: no predictions, no learning, no key previews.
    pub(crate) secure: bool,
    /// Whether the keyboard corrects, predicts and checks the spelling of words. With no value the
    /// platform decides.
    pub(crate) autocorrect: Option<bool>,
    /// Which letters the keyboard capitalizes by itself.
    pub(crate) autocapitalize: Autocapitalize,
    /// What the value is, for the platform's offer to fill it in.
    pub(crate) content_type: Option<TextContentType>,
    /// Whether the field takes line breaks.
    pub(crate) multi_line: bool,
}

impl KeyboardTraits {
    /// The keyboard `input` asks for.
    ///
    /// A secret gets no correction and no capitals, whatever the field declares, because both
    /// learn from and show what is typed. A single-line field with no hint gets "Done", because
    /// its return key ends the input.
    pub(crate) fn of(input: &TextInput) -> Self {
        let hints = input.hints;
        let secure = input.purpose.is_secret();
        let content_type = hints.content_type;

        let keys = match (input.purpose, content_type) {
            (TextInputPurpose::Pin | TextInputPurpose::Digits, _)
            | (_, Some(TextContentType::OneTimeCode)) => Keys::Digits,
            (TextInputPurpose::Number, _) => Keys::Decimal,
            (TextInputPurpose::Phone, _) | (_, Some(TextContentType::Telephone)) => Keys::Phone,
            (TextInputPurpose::Url, _) | (_, Some(TextContentType::Url)) => Keys::Url,
            (TextInputPurpose::Email, _) | (_, Some(TextContentType::Email)) => Keys::Email,
            (TextInputPurpose::Search, _) => Keys::Search,
            _ => Keys::Letters,
        };
        let return_key = match hints.enter_key {
            Some(EnterKeyHint::Done) => ReturnKey::Done,
            Some(EnterKeyHint::Go) => ReturnKey::Go,
            Some(EnterKeyHint::Next) => ReturnKey::Next,
            Some(EnterKeyHint::Search) => ReturnKey::Search,
            Some(EnterKeyHint::Send) => ReturnKey::Send,
            Some(_) => ReturnKey::Return,
            None if hints.multi_line => ReturnKey::Return,
            None => ReturnKey::Done,
        };
        // A name of an account, an address or a code keeps the letters typed.
        let keeps_case = matches!(
            content_type,
            Some(
                TextContentType::Username
                    | TextContentType::Email
                    | TextContentType::Url
                    | TextContentType::OneTimeCode
            )
        ) || matches!(keys, Keys::Url | Keys::Email);
        let autocapitalize = match hints.autocapitalize {
            _ if secure => Autocapitalize::None,
            Some(declared) => declared,
            None if keeps_case => Autocapitalize::None,
            None => Autocapitalize::Sentences,
        };
        let autocorrect = if secure {
            Some(false)
        } else {
            hints.autocorrect.or(keeps_case.then_some(false))
        };

        Self {
            keys,
            return_key,
            secure,
            autocorrect,
            autocapitalize,
            content_type,
            multi_line: hints.multi_line,
        }
    }

    /// Whether the keyboard puts in smart quotes and dashes, and checks spelling.
    pub(crate) fn smart_punctuation(self) -> bool {
        self.autocorrect != Some(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use zgui_geom::{CssPx, Point, Size};
    use zgui_platform::TextInputHints;

    fn input(purpose: TextInputPurpose, hints: TextInputHints) -> TextInput {
        TextInput::new(
            Point::new(CssPx(0.0), CssPx(0.0)),
            Size::new(CssPx(1.0), CssPx(16.0)),
        )
        .with_purpose(purpose)
        .with_hints(hints)
    }

    #[test]
    fn a_password_field_is_secure_and_neither_corrects_nor_capitalizes() {
        let hints = TextInputHints {
            autocorrect: Some(true),
            autocapitalize: Some(Autocapitalize::Words),
            content_type: Some(TextContentType::Password),
            ..TextInputHints::default()
        };
        let traits = KeyboardTraits::of(&input(TextInputPurpose::Password, hints));
        assert!(traits.secure);
        assert_eq!(traits.autocorrect, Some(false));
        assert_eq!(traits.autocapitalize, Autocapitalize::None);
        assert!(!traits.smart_punctuation());
        assert_eq!(traits.content_type, Some(TextContentType::Password));
        assert_eq!(traits.return_key, ReturnKey::Done);
    }

    #[test]
    fn a_plain_field_leaves_correction_to_the_platform() {
        let traits =
            KeyboardTraits::of(&input(TextInputPurpose::Normal, TextInputHints::default()));
        assert!(!traits.secure);
        assert_eq!(traits.autocorrect, None);
        assert_eq!(traits.autocapitalize, Autocapitalize::Sentences);
        assert_eq!(traits.keys, Keys::Letters);
        assert_eq!(traits.return_key, ReturnKey::Done);
    }

    #[test]
    fn declared_hints_reach_the_keyboard() {
        let hints = TextInputHints {
            enter_key: Some(EnterKeyHint::Go),
            autocorrect: Some(false),
            autocapitalize: Some(Autocapitalize::None),
            ..TextInputHints::default()
        };
        let traits = KeyboardTraits::of(&input(TextInputPurpose::Normal, hints));
        assert_eq!(traits.return_key, ReturnKey::Go);
        assert_eq!(traits.autocorrect, Some(false));
        assert_eq!(traits.autocapitalize, Autocapitalize::None);
    }

    #[test]
    fn an_input_mode_picks_the_keys() {
        let keys = |mode: &str| {
            let purpose = TextInputPurpose::parse(mode).expect("a known input mode");
            KeyboardTraits::of(&input(purpose, TextInputHints::default())).keys
        };
        assert_eq!(keys("numeric"), Keys::Digits);
        assert_eq!(keys("decimal"), Keys::Decimal);
        assert_eq!(keys("tel"), Keys::Phone);
        assert_eq!(keys("email"), Keys::Email);
        assert_eq!(keys("url"), Keys::Url);
        assert_eq!(keys("search"), Keys::Search);
    }

    #[test]
    fn a_user_name_keeps_its_case_and_a_code_gets_digits() {
        let username = TextInputHints {
            content_type: Some(TextContentType::Username),
            ..TextInputHints::default()
        };
        let traits = KeyboardTraits::of(&input(TextInputPurpose::Normal, username));
        assert_eq!(traits.autocapitalize, Autocapitalize::None);
        assert_eq!(traits.autocorrect, Some(false));

        let code = TextInputHints {
            content_type: Some(TextContentType::OneTimeCode),
            ..TextInputHints::default()
        };
        assert_eq!(
            KeyboardTraits::of(&input(TextInputPurpose::Normal, code)).keys,
            Keys::Digits
        );
    }

    #[test]
    fn a_multi_line_field_keeps_its_return_key() {
        let hints = TextInputHints {
            multi_line: true,
            ..TextInputHints::default()
        };
        let traits = KeyboardTraits::of(&input(TextInputPurpose::Normal, hints));
        assert_eq!(traits.return_key, ReturnKey::Return);
        assert!(traits.multi_line);

        let previous = TextInputHints {
            enter_key: Some(EnterKeyHint::Previous),
            ..TextInputHints::default()
        };
        assert_eq!(
            KeyboardTraits::of(&input(TextInputPurpose::Normal, previous)).return_key,
            ReturnKey::Return,
            "UIKit has no label for previous"
        );
    }
}
