//! Telling the platform where text is being typed, and what kind.

use zgui_geom::{Css, CssPx, Point, Size};

/// What kind of text a field expects, so the platform can offer the right keyboard.
///
/// On a desktop with a hardware keyboard this changes little. On a touch platform it changes
/// everything: a field that asks for a number and gets a full keyboard is a field that is painful
/// to use, and the platform has no other way to know.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextInputPurpose {
    /// Ordinary text.
    #[default]
    Normal,
    /// A secret, which must not be remembered, suggested or shown.
    Password,
    /// A personal identification number, which is a secret made of digits.
    Pin,
    /// A number.
    Number,
    /// A telephone number.
    Phone,
    /// A web address.
    Url,
    /// An electronic mail address.
    Email,
    /// A term to search for.
    Search,
}

impl TextInputPurpose {
    /// Whether what is typed here must never be recorded or suggested.
    pub const fn is_secret(self) -> bool {
        matches!(self, Self::Password | Self::Pin)
    }
}

/// What the return key of an on-screen keyboard says, after the values of HTML's `enterkeyhint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EnterKeyHint {
    /// The key puts in a line break.
    Enter,
    /// The key ends the input.
    Done,
    /// The key goes to the target of the text typed.
    Go,
    /// The key moves to the next field.
    Next,
    /// The key moves to the previous field.
    Previous,
    /// The key searches for the text typed.
    Search,
    /// The key sends the text typed.
    Send,
}

impl EnterKeyHint {
    /// The hint an `enterkeyhint` attribute value names.
    ///
    /// ```
    /// use zgui_platform::EnterKeyHint;
    ///
    /// assert_eq!(EnterKeyHint::parse("go"), Some(EnterKeyHint::Go));
    /// assert_eq!(EnterKeyHint::parse("Send"), Some(EnterKeyHint::Send));
    /// assert_eq!(EnterKeyHint::parse("submit"), None);
    /// ```
    pub fn parse(value: &str) -> Option<Self> {
        const NAMES: [(&str, EnterKeyHint); 7] = [
            ("enter", EnterKeyHint::Enter),
            ("done", EnterKeyHint::Done),
            ("go", EnterKeyHint::Go),
            ("next", EnterKeyHint::Next),
            ("previous", EnterKeyHint::Previous),
            ("search", EnterKeyHint::Search),
            ("send", EnterKeyHint::Send),
        ];

        let value = value.trim();
        NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(value))
            .map(|(_, hint)| *hint)
    }
}

/// Which letters an on-screen keyboard capitalizes by itself, after HTML's `autocapitalize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Autocapitalize {
    /// No letter.
    None,
    /// The first letter of each sentence.
    Sentences,
    /// The first letter of each word.
    Words,
    /// Every letter.
    Characters,
}

impl Autocapitalize {
    /// The value an `autocapitalize` attribute value names.
    ///
    /// ```
    /// use zgui_platform::Autocapitalize;
    ///
    /// assert_eq!(Autocapitalize::parse("off"), Some(Autocapitalize::None));
    /// assert_eq!(Autocapitalize::parse("words"), Some(Autocapitalize::Words));
    /// assert_eq!(Autocapitalize::parse("loud"), None);
    /// ```
    pub fn parse(value: &str) -> Option<Self> {
        const NAMES: [(&str, Autocapitalize); 6] = [
            ("none", Autocapitalize::None),
            ("off", Autocapitalize::None),
            ("sentences", Autocapitalize::Sentences),
            ("on", Autocapitalize::Sentences),
            ("words", Autocapitalize::Words),
            ("characters", Autocapitalize::Characters),
        ];

        let value = value.trim();
        NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(value))
            .map(|(_, capitalize)| *capitalize)
    }
}

/// What a field's value is, so the platform can offer to fill it in, after HTML's
/// `autocomplete`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextContentType {
    /// The name of an account.
    Username,
    /// The password of an account.
    Password,
    /// A password for a new account, or a changed password.
    NewPassword,
    /// A code sent once, for example by a text message.
    OneTimeCode,
    /// An electronic mail address.
    Email,
    /// The name of a person.
    Name,
    /// A telephone number.
    Telephone,
    /// A web address.
    Url,
}

impl TextContentType {
    /// The type an `autocomplete` attribute value names.
    ///
    /// The last token of the value names the type, so `section-login username` is a user name.
    ///
    /// ```
    /// use zgui_platform::TextContentType;
    ///
    /// assert_eq!(TextContentType::parse("current-password"), Some(TextContentType::Password));
    /// assert_eq!(TextContentType::parse("section-a username"), Some(TextContentType::Username));
    /// assert_eq!(TextContentType::parse("off"), None);
    /// ```
    pub fn parse(value: &str) -> Option<Self> {
        const NAMES: [(&str, TextContentType); 9] = [
            ("username", TextContentType::Username),
            ("current-password", TextContentType::Password),
            ("password", TextContentType::Password),
            ("new-password", TextContentType::NewPassword),
            ("one-time-code", TextContentType::OneTimeCode),
            ("email", TextContentType::Email),
            ("name", TextContentType::Name),
            ("tel", TextContentType::Telephone),
            ("url", TextContentType::Url),
        ];

        let token = value.split_ascii_whitespace().next_back()?;
        NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(token))
            .map(|(_, content)| *content)
    }
}

/// How the keyboard of a field behaves, as the field declares it.
///
/// Each part with no value leaves the choice to the platform. A platform turns autocorrection and
/// autocapitalization off for a [secret](TextInputPurpose::is_secret) purpose whatever the field
/// declares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextInputHints {
    /// Whether the field takes line breaks.
    pub multi_line: bool,
    /// What the return key says. With no value the platform picks a label from
    /// [`multi_line`](TextInputHints::multi_line).
    pub enter_key: Option<EnterKeyHint>,
    /// Whether the keyboard corrects and predicts words.
    pub autocorrect: Option<bool>,
    /// Which letters the keyboard capitalizes by itself.
    pub autocapitalize: Option<Autocapitalize>,
    /// What the value is, so the platform can offer to fill it in.
    pub content_type: Option<TextContentType>,
}

/// Everything the platform needs to know about the text being typed right now.
///
/// The parts travel together because they are set together. A platform that is told the
/// caret moved but not that the field is still active, or told a purpose before it has an area to
/// place a candidate window in, shows its input method in the wrong place — which for a language
/// that composes its characters means typing over the top of what is being typed.
///
/// The area is where the *caret* is, not where the field is, so a candidate list appears beside
/// the insertion point rather than beside the control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInput {
    /// Where the caret sits, in CSS pixels from the surface's top-left corner.
    pub caret_origin: Point<CssPx, Css>,
    /// How large the caret is, so a candidate window can avoid covering it.
    pub caret_size: Size<CssPx, Css>,
    /// What kind of text is expected.
    pub purpose: TextInputPurpose,
    /// How the keyboard behaves.
    pub hints: TextInputHints,
}

impl TextInput {
    /// Ordinary single-line text with a caret of the given size at the given place.
    pub const fn new(caret_origin: Point<CssPx, Css>, caret_size: Size<CssPx, Css>) -> Self {
        Self {
            caret_origin,
            caret_size,
            purpose: TextInputPurpose::Normal,
            hints: TextInputHints {
                multi_line: false,
                enter_key: None,
                autocorrect: None,
                autocapitalize: None,
                content_type: None,
            },
        }
    }

    /// The same state with a different purpose.
    pub const fn with_purpose(mut self, purpose: TextInputPurpose) -> Self {
        self.purpose = purpose;
        self
    }

    /// The same state with different keyboard hints.
    pub const fn with_hints(mut self, hints: TextInputHints) -> Self {
        self.hints = hints;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{TextInput, TextInputPurpose};
    use zgui_geom::{CssPx, Point, Size};

    #[test]
    fn only_the_secret_purposes_are_secret() {
        assert!(TextInputPurpose::Password.is_secret());
        assert!(TextInputPurpose::Pin.is_secret());
        assert!(!TextInputPurpose::Email.is_secret());
        assert!(!TextInputPurpose::default().is_secret());
    }

    #[test]
    fn every_kind_of_field_is_something_a_backend_can_be_told_about() {
        // A backend maps each of these onto whatever its platform offers, and a kind nothing ever
        // names is a keyboard nobody can ask for.
        let kinds = [
            TextInputPurpose::Normal,
            TextInputPurpose::Password,
            TextInputPurpose::Pin,
            TextInputPurpose::Number,
            TextInputPurpose::Phone,
            TextInputPurpose::Url,
            TextInputPurpose::Email,
            TextInputPurpose::Search,
        ];
        for kind in kinds {
            let state = TextInput::new(
                Point::new(CssPx(0.0), CssPx(0.0)),
                Size::new(CssPx(1.0), CssPx(16.0)),
            )
            .with_purpose(kind);
            assert_eq!(state.purpose, kind);
        }
        assert_eq!(kinds.iter().filter(|kind| kind.is_secret()).count(), 2);
    }

    #[test]
    fn the_state_carries_the_caret_rather_than_the_field() {
        let state = TextInput::new(
            Point::new(CssPx(120.0), CssPx(48.0)),
            Size::new(CssPx(1.0), CssPx(16.0)),
        )
        .with_purpose(TextInputPurpose::Search);
        assert_eq!(state.caret_size.width, CssPx(1.0));
        assert_eq!(state.purpose, TextInputPurpose::Search);
    }
}
