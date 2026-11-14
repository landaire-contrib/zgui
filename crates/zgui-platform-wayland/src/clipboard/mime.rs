//! Which media types mean text, and which one to ask for.

use zgui_platform::ClipboardFormat;

/// The media types this backend offers when it takes a selection, best first.
///
/// Five names for one thing, and all five are needed. The first is what every modern toolkit
/// looks for; the last three are what applications written against X11's selection conventions
/// look for, and a great many still are. Offering only the modern name means a paste into an older
/// application silently produces nothing.
pub const OFFERED: [&str; 5] = [
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];

/// The media type for a list of addresses being dragged, files among them.
pub const URI_LIST: &str = "text/uri-list";

/// The media type for markup being dragged.
pub const HTML: &str = "text/html";

/// The media type to ask an offer for, given what it says it has.
///
/// In preference order rather than first-match, because an offer usually lists several and the
/// difference between them is the encoding: `STRING` is Latin-1 by X11 convention and
/// `text/plain` is unspecified, so asking for either when a declared UTF-8 name is on the list
/// turns every accented character into a question mark.
pub fn best_text(offered: &[String]) -> Option<String> {
    PREFERRED_TEXT
        .iter()
        .find(|wanted| offered.iter().any(|has| has.eq_ignore_ascii_case(wanted)))
        .map(|found| (*found).to_owned())
        .or_else(|| {
            // Nothing named, but something that is still text: a compositor or an application may
            // offer `text/plain;charset=UTF-8` with different capitalisation, or a subtype nobody
            // enumerated. Anything under the text tree can at least be shown.
            offered
                .iter()
                .find(|has| has.to_ascii_lowercase().starts_with("text/"))
                .cloned()
        })
}

/// The names of plain text, best first. See [`best_text`] for why the order is what it is.
const PREFERRED_TEXT: [&str; 5] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
    "TEXT",
];

/// The best of the media types an offer has that name plain text, spelled as the offer spells it.
///
/// The names only, without [`best_text`]'s fallback to anything under `text/`: a drag offers its
/// address list and its markup under that tree too, and both are read as forms of their own.
pub fn named_text(offered: &[String]) -> Option<String> {
    PREFERRED_TEXT
        .iter()
        .find_map(|wanted| spelled(offered, wanted))
}

/// The entry of `offered` that is `name` in any case.
///
/// A source answers a request for the name it advertised, so a request is made in its spelling.
pub fn spelled(offered: &[String], name: &str) -> Option<String> {
    offered
        .iter()
        .find(|has| has.eq_ignore_ascii_case(name))
        .cloned()
}

/// Whether an offer carries a list of addresses.
pub fn has_uri_list(offered: &[String]) -> bool {
    offered.iter().any(|has| has.eq_ignore_ascii_case(URI_LIST))
}

/// Whether an offer carries markup.
pub fn has_html(offered: &[String]) -> bool {
    offered.iter().any(|has| has.eq_ignore_ascii_case(HTML))
}

/// Whether this backend can answer a request for `format` at all.
///
/// Only plain text crosses this boundary, which is what the contract promises and no more. Markup,
/// images and file lists on the *clipboard* are refused with a reason a caller can fall back from,
/// rather than answered with an empty value it cannot tell from an empty clipboard. Content being
/// dragged is a different path and arrives in each form the drag reader takes.
pub const fn is_supported(format: ClipboardFormat) -> bool {
    matches!(format, ClipboardFormat::Text)
}

#[cfg(test)]
mod tests {
    use super::{HTML, best_text, has_html, has_uri_list, is_supported, named_text, spelled};
    use zgui_platform::ClipboardFormat;

    fn offered(types: &[&str]) -> Vec<String> {
        types.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn an_offer_with_nothing_on_it_has_nothing_to_ask_for() {
        assert_eq!(best_text(&offered(&[])), None);
        assert_eq!(best_text(&offered(&["image/png"])), None);
    }

    #[test]
    fn a_declared_encoding_wins_over_one_that_has_to_be_guessed() {
        // `STRING` is Latin-1 by convention and `text/plain` says nothing, so taking either while
        // a declared UTF-8 name is on the list turns every accented character into a question mark.
        let both = offered(&["STRING", "text/plain", "text/plain;charset=utf-8"]);
        assert_eq!(
            best_text(&both).as_deref(),
            Some("text/plain;charset=utf-8")
        );
        let older = offered(&["STRING", "TEXT"]);
        assert_eq!(best_text(&older).as_deref(), Some("STRING"));
    }

    #[test]
    fn a_name_in_another_case_is_still_that_name() {
        let shouted = offered(&["TEXT/PLAIN;CHARSET=UTF-8"]);
        assert_eq!(
            best_text(&shouted).as_deref(),
            Some("text/plain;charset=utf-8")
        );
    }

    #[test]
    fn a_text_subtype_nobody_enumerated_is_still_text() {
        let unusual = offered(&["text/markdown"]);
        assert_eq!(best_text(&unusual).as_deref(), Some("text/markdown"));
    }

    #[test]
    fn a_list_of_addresses_and_markup_are_recognised_whatever_their_case() {
        assert!(has_uri_list(&offered(&["text/uri-list"])));
        assert!(has_uri_list(&offered(&["TEXT/URI-LIST"])));
        assert!(!has_uri_list(&offered(&["text/plain"])));
        assert!(has_html(&offered(&["TEXT/HTML"])));
        assert!(!has_html(&offered(&["text/plain"])));
    }

    #[test]
    fn a_name_is_asked_for_as_the_offer_spells_it() {
        let shouted = offered(&["TEXT/PLAIN;CHARSET=UTF-8", "TEXT/HTML"]);
        assert_eq!(
            named_text(&shouted).as_deref(),
            Some("TEXT/PLAIN;CHARSET=UTF-8")
        );
        assert_eq!(spelled(&shouted, HTML).as_deref(), Some("TEXT/HTML"));
    }

    #[test]
    fn a_drag_s_address_list_and_markup_are_not_taken_for_its_text() {
        // Both live under `text/`, which the clipboard's fallback would take as plain text.
        let link = offered(&["text/uri-list", "text/html"]);
        assert_eq!(named_text(&link), None);
        assert_eq!(best_text(&link).as_deref(), Some("text/uri-list"));
    }

    #[test]
    fn only_plain_text_crosses_the_clipboard_boundary() {
        assert!(is_supported(ClipboardFormat::Text));
        assert!(!is_supported(ClipboardFormat::Html));
        assert!(!is_supported(ClipboardFormat::Image));
        assert!(!is_supported(ClipboardFormat::FileList));
    }
}
