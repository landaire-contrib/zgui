//! The safe area insets a stylesheet reads with `env()`.
//!
//! The style engine answers `env(safe-area-inset-*)` from a device that always reports zero, and
//! the device has no setter. [`substitute`] thus writes each of the four references as a `var()` of
//! a custom property before the text is parsed. [`safe_area_rule`] gives those properties their
//! values on the root element, and every other element inherits them.
//!
//! The substitution keeps the byte offset of every token, so a parse error reports the location it
//! has in the source text.

use std::borrow::Cow;
use std::ops::Range;

use cssparser::{ParseError, Parser, ParserInput, Token};
use zgui_geom::{CssPx, Edges};

/// One safe area inset, by the name `env()` reads it with and by its custom property.
struct Inset {
    /// The environment variable name.
    variable: &'static str,
    /// The custom property it is substituted with.
    ///
    /// Each name is shorter than its variable name, so the substitution fits in the bytes of the
    /// reference.
    property: &'static str,
}

/// The four insets, in the order top, right, bottom, left.
const INSETS: [Inset; 4] = [
    Inset {
        variable: "safe-area-inset-top",
        property: "--zgui-safe-top",
    },
    Inset {
        variable: "safe-area-inset-right",
        property: "--zgui-safe-right",
    },
    Inset {
        variable: "safe-area-inset-bottom",
        property: "--zgui-safe-bottom",
    },
    Inset {
        variable: "safe-area-inset-left",
        property: "--zgui-safe-left",
    },
];

/// One span of source text and the text that replaces it.
struct Replacement {
    /// The bytes replaced.
    span: Range<usize>,
    /// The text put at the start of the span. Spaces fill the remainder.
    text: &'static str,
}

/// `css` with each `env(safe-area-inset-*)` reference written as a `var()` of its custom property.
///
/// A fallback stays in place. Strings, URLs and other environment variables do not change. The
/// result has the same length as `css`, and text with no reference is returned as it is.
pub fn substitute(css: &str) -> Cow<'_, str> {
    // A function token named `env` contains those three letters, or an escape.
    let bytes = css.as_bytes();
    let mentions_env = bytes
        .windows(3)
        .any(|window| window.eq_ignore_ascii_case(b"env"));
    if !mentions_env && !bytes.contains(&b'\\') {
        return Cow::Borrowed(css);
    }

    let mut replacements = Vec::new();
    let mut input = ParserInput::new(css);
    collect(&mut Parser::new(&mut input), &mut replacements);
    if replacements.is_empty() {
        return Cow::Borrowed(css);
    }

    let mut substituted = String::with_capacity(css.len());
    let mut copied = 0;
    for Replacement { span, text } in replacements {
        substituted.push_str(&css[copied..span.start]);
        substituted.push_str(text);
        substituted.extend(std::iter::repeat_n(' ', span.len() - text.len()));
        copied = span.end;
    }
    substituted.push_str(&css[copied..]);

    Cow::Owned(substituted)
}

/// A rule that sets the custom property of each inset on the root element.
pub fn safe_area_rule(insets: Edges<CssPx>) -> String {
    let [top, right, bottom, left] = insets.into_array().map(|inset| inset.0);
    let [top_name, right_name, bottom_name, left_name] = INSETS.map(|inset| inset.property);

    format!(
        ":root {{ {top_name}: {top}px; {right_name}: {right}px; {bottom_name}: {bottom}px; \
         {left_name}: {left}px; }}"
    )
}

/// Records the replacements for every safe area reference in what `parser` has left, in source
/// order.
///
/// The parser gives the contents of a block only to a nested parser, so this descends once for
/// each nested block.
fn collect(parser: &mut Parser<'_, '_>, replacements: &mut Vec<Replacement>) {
    loop {
        let start = parser.position();
        let Ok(token) = parser.next_including_whitespace_and_comments() else {
            return;
        };
        let is_env = matches!(token, Token::Function(name) if name.eq_ignore_ascii_case("env"));
        let opens_block = matches!(
            token,
            Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock
        );
        if !opens_block {
            continue;
        }

        let function = start.byte_index()..parser.position().byte_index();
        let _ = parser.parse_nested_block(|block| {
            if is_env {
                reference(block, function, replacements);
            }
            collect(block, replacements);
            Ok::<(), ParseError<'_, ()>>(())
        });
    }
}

/// Records the replacements for the arguments of one `env()`, when they name a safe area inset.
///
/// `function` is the span of the function token. The parser is left after the name when the first
/// argument is an ident, and where it was otherwise.
fn reference(
    block: &mut Parser<'_, '_>,
    function: Range<usize>,
    replacements: &mut Vec<Replacement>,
) {
    loop {
        let state = block.state();
        let start = block.position();
        let Ok(token) = block.next_including_whitespace_and_comments() else {
            return;
        };
        let name = match token {
            Token::WhiteSpace(_) | Token::Comment(_) => continue,
            Token::Ident(name) => name,
            _ => {
                block.reset(&state);
                return;
            }
        };
        let Some(inset) = INSETS.iter().find(|inset| inset.variable == &**name) else {
            return;
        };

        replacements.push(Replacement {
            span: function,
            text: "var(",
        });
        replacements.push(Replacement {
            span: start.byte_index()..block.position().byte_index(),
            text: inset.property,
        });
        return;
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use zgui_geom::{CssPx, Edges};

    use super::{safe_area_rule, substitute};

    #[test]
    fn text_with_no_reference_is_returned_as_it_is() {
        let css = ".a { padding: 4px; color: var(--accent); }";
        assert!(matches!(substitute(css), Cow::Borrowed(text) if text == css));
    }

    #[test]
    fn each_inset_becomes_its_custom_property_in_the_same_bytes() {
        let css = ".a { padding: env(safe-area-inset-top) env(safe-area-inset-right) \
                   env(safe-area-inset-bottom) env(safe-area-inset-left); }";
        let substituted = substitute(css);
        assert_eq!(
            substituted,
            ".a { padding: var(--zgui-safe-top    ) var(--zgui-safe-right    ) \
             var(--zgui-safe-bottom    ) var(--zgui-safe-left    ); }"
        );
        assert_eq!(substituted.len(), css.len());
    }

    #[test]
    fn a_fallback_and_a_nested_reference_are_kept() {
        assert_eq!(
            substitute("calc(1px + ENV( safe-area-inset-bottom, env(safe-area-inset-left, 2px)))"),
            "calc(1px + var( --zgui-safe-bottom    , var(--zgui-safe-left    , 2px)))"
        );
    }

    #[test]
    fn strings_urls_and_other_variables_do_not_change() {
        let css = "a { content: \"env(safe-area-inset-top)\"; background: \
                   url(env(safe-area-inset-top)); top: env(keyboard-inset-top); \
                   left: env(1px, env(titlebar-area-x)); }";
        assert_eq!(substitute(css), css);
    }

    #[test]
    fn an_escaped_function_name_is_a_reference() {
        assert_eq!(
            substitute("top: \\65nv(safe-area-inset-top)"),
            "top: var(  --zgui-safe-top    )"
        );
    }

    #[test]
    fn the_rule_sets_each_property_on_the_root() {
        let insets = Edges::new(CssPx(59.0), CssPx(0.0), CssPx(34.0), CssPx(0.5));
        assert_eq!(
            safe_area_rule(insets),
            ":root { --zgui-safe-top: 59px; --zgui-safe-right: 0px; --zgui-safe-bottom: 34px; \
             --zgui-safe-left: 0.5px; }"
        );
    }
}
