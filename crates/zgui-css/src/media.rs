//! The media features a stylesheet can query that the style engine does not know.
//!
//! The engine's media-feature list is fixed and has no `prefers-reduced-motion`. A query that names
//! it is a parse error, and the engine drops the whole `@media` rule. [`substitute`] thus writes
//! each `(prefers-reduced-motion)` condition in the prelude of an `@media` or `@import` rule as a
//! condition the engine knows and that has the same answer: `(not (scan))` when it matches, and
//! `(scan)` when it does not. The engine never matches `scan` on a screen.
//!
//! The answer is fixed when the sheet is parsed. A sheet that names the feature is parsed again
//! when the preference changes, which [`Substituted::reads_motion`] tells the caller to do.
//!
//! The substitution keeps the byte offset of every token, so a parse error reports the location it
//! has in the source text.

use std::borrow::Cow;
use std::ops::Range;

use cssparser::ParseError;
use cssparser::Parser;
use cssparser::ParserInput;
use cssparser::Token;

/// The name of the media feature.
const FEATURE: &str = "prefers-reduced-motion";

/// The condition that is true on every device the engine styles for.
const ALWAYS: &str = "(not (scan))";

/// The condition that is false on every device the engine styles for.
const NEVER: &str = "(scan)";

/// Whether the user asked the system to reduce motion.
///
/// This is what `prefers-reduced-motion` answers. A platform that cannot be asked reports
/// [`MotionPreference::NoPreference`], which is also what the media feature means when nothing is
/// known.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum MotionPreference {
    /// The user has expressed no preference.
    #[default]
    NoPreference,
    /// The user asked for less motion.
    Reduce,
}

/// Stylesheet text after [`substitute`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Substituted<'a> {
    /// The text, with each condition written in terms the engine knows.
    pub text: Cow<'a, str>,
    /// Whether the text queries `prefers-reduced-motion`, so that its rules change when the
    /// preference changes.
    pub reads_motion: bool,
}

/// `css` with each `prefers-reduced-motion` condition replaced by its answer under `motion`.
///
/// A condition with a value other than `reduce` or `no-preference` stays in place, and the engine
/// rejects it as it rejects any malformed query. The result has the same length as `css`, and text
/// with no condition is returned as it is.
pub fn substitute(css: &str, motion: MotionPreference) -> Substituted<'_> {
    let unchanged = Substituted {
        text: Cow::Borrowed(css),
        reads_motion: false,
    };
    // An identifier that names the feature contains `motion`, or an escape.
    let bytes = css.as_bytes();
    let mentions = bytes
        .windows(6)
        .any(|window| window.eq_ignore_ascii_case(b"motion"));
    if !mentions && !bytes.contains(&b'\\') {
        return unchanged;
    }

    let mut conditions = Vec::new();
    let mut input = ParserInput::new(css);
    collect(&mut Parser::new(&mut input), false, &mut conditions);
    if conditions.is_empty() {
        return unchanged;
    }

    let mut substituted = String::with_capacity(css.len());
    let mut copied = 0;
    for Condition { span, query } in conditions {
        let text = if query.matches(motion) { ALWAYS } else { NEVER };
        substituted.push_str(&css[copied..span.start]);
        substituted.push_str(text);
        substituted.extend(std::iter::repeat_n(' ', span.len() - text.len()));
        copied = span.end;
    }
    substituted.push_str(&css[copied..]);

    Substituted {
        text: Cow::Owned(substituted),
        reads_motion: true,
    }
}

/// One `(prefers-reduced-motion)` condition, by its span in the source text.
struct Condition {
    /// The span from the opening parenthesis to the closing one.
    ///
    /// The shortest condition, `(prefers-reduced-motion)`, is longer than [`ALWAYS`] and
    /// [`NEVER`], so either fits in its bytes.
    span: Range<usize>,
    /// What the condition asks.
    query: Query,
}

/// What a condition asks of the preference.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Query {
    /// `(prefers-reduced-motion)`, which matches when the preference is other than
    /// `no-preference`.
    Boolean,
    /// `(prefers-reduced-motion: <value>)`.
    Value(MotionPreference),
}

impl Query {
    /// Whether the condition is true under `motion`.
    fn matches(self, motion: MotionPreference) -> bool {
        match self {
            Self::Boolean => motion != MotionPreference::NoPreference,
            Self::Value(value) => value == motion,
        }
    }
}

/// Records each condition in a media query prelude in what `parser` has left, in source order.
///
/// `in_prelude` says that `parser` reads a media query list. A prelude starts after an `@media` or
/// `@import` keyword and ends at the block or the semicolon after it. The parser gives the contents
/// of a block only to a nested parser, so this descends once for each nested block.
fn collect(parser: &mut Parser<'_, '_>, in_prelude: bool, conditions: &mut Vec<Condition>) {
    let mut in_prelude = in_prelude;
    loop {
        let start = parser.position();
        let Ok(token) = parser.next_including_whitespace_and_comments() else {
            return;
        };
        let nested_prelude = match token {
            Token::AtKeyword(name) => {
                in_prelude =
                    name.eq_ignore_ascii_case("media") || name.eq_ignore_ascii_case("import");
                continue;
            }
            Token::Semicolon => {
                in_prelude = false;
                continue;
            }
            Token::CurlyBracketBlock => {
                in_prelude = false;
                false
            }
            Token::ParenthesisBlock => in_prelude,
            Token::Function(_) | Token::SquareBracketBlock => false,
            _ => continue,
        };

        let condition = parser
            .parse_nested_block(|block| {
                if nested_prelude && let Some(query) = query(block) {
                    return Ok(Some(query));
                }
                collect(block, nested_prelude, conditions);
                Ok::<Option<Query>, ParseError<'_, ()>>(None)
            })
            .ok()
            .flatten();
        if let Some(query) = condition {
            conditions.push(Condition {
                span: start.byte_index()..parser.position().byte_index(),
                query,
            });
        }
    }
}

/// What the contents of one parenthesised condition ask of the preference, when the condition is
/// `prefers-reduced-motion` with a known value or with none.
///
/// The parser is left where it was when the condition is another one.
fn query(block: &mut Parser<'_, '_>) -> Option<Query> {
    let state = block.state();
    let query = read_query(block);
    if query.is_none() {
        block.reset(&state);
    }

    query
}

/// The same, leaving the parser wherever reading stopped.
fn read_query(block: &mut Parser<'_, '_>) -> Option<Query> {
    match block.next().ok()? {
        Token::Ident(name) if name.eq_ignore_ascii_case(FEATURE) => {}
        _ => return None,
    }
    let query = match block.next() {
        Err(_) => return Some(Query::Boolean),
        Ok(Token::Colon) => {
            let value = match block.next().ok()? {
                Token::Ident(value) if value.eq_ignore_ascii_case("reduce") => {
                    MotionPreference::Reduce
                }
                Token::Ident(value) if value.eq_ignore_ascii_case("no-preference") => {
                    MotionPreference::NoPreference
                }
                _ => return None,
            };
            Query::Value(value)
        }
        Ok(_) => return None,
    };

    block.expect_exhausted().ok()?;

    Some(query)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::MotionPreference;
    use super::substitute;

    #[test]
    fn text_with_no_condition_is_returned_as_it_is() {
        let css = "@media (width > 400px) { .a { transition: none; } }";
        let substituted = substitute(css, MotionPreference::Reduce);
        assert!(matches!(substituted.text, Cow::Borrowed(text) if text == css));
        assert!(!substituted.reads_motion);
    }

    #[test]
    fn each_condition_becomes_its_answer_in_the_same_bytes() {
        let css = "@media (prefers-reduced-motion: reduce) { .a { animation: none; } }\n\
                   @media (prefers-reduced-motion: no-preference) { .b { animation: spin 1s; } }\n\
                   @media (prefers-reduced-motion) { .c { transition: none; } }";

        let reduced = substitute(css, MotionPreference::Reduce);
        assert!(reduced.reads_motion);
        assert_eq!(reduced.text.len(), css.len());
        assert_eq!(
            reduced.text,
            format!(
                "@media {:<32} {{ .a {{ animation: none; }} }}\n\
                 @media {:<39} {{ .b {{ animation: spin 1s; }} }}\n\
                 @media {:<24} {{ .c {{ transition: none; }} }}",
                "(not (scan))", "(scan)", "(not (scan))"
            )
        );

        let moving = substitute(css, MotionPreference::NoPreference);
        assert_eq!(
            moving.text,
            format!(
                "@media {:<32} {{ .a {{ animation: none; }} }}\n\
                 @media {:<39} {{ .b {{ animation: spin 1s; }} }}\n\
                 @media {:<24} {{ .c {{ transition: none; }} }}",
                "(scan)", "(not (scan))", "(scan)"
            )
        );
    }

    #[test]
    fn a_condition_inside_a_larger_query_and_a_nested_rule_is_replaced() {
        let css = "@layer base { @media screen and (not (PREFERS-reduced-motion : reduce)), \
                   print { .a { color: red } } } @import url(a.css) (prefers-reduced-motion);";
        assert_eq!(
            substitute(css, MotionPreference::Reduce).text,
            format!(
                "@layer base {{ @media screen and (not {:<33}), print {{ .a {{ color: red }} }} }} \
                 @import url(a.css) {:<24};",
                "(not (scan))", "(not (scan))"
            )
        );
    }

    #[test]
    fn strings_declarations_and_unknown_values_do_not_change() {
        let css = "@media (prefers-reduced-motion: fast) { .a { color: red } } \
                   .b { content: \"(prefers-reduced-motion)\"; } \
                   @supports (prefers-reduced-motion: reduce) { .c { color: red } } \
                   .d { --x: (prefers-reduced-motion); }";
        let substituted = substitute(css, MotionPreference::Reduce);
        assert_eq!(substituted.text, css);
        assert!(!substituted.reads_motion);
    }

    #[test]
    fn an_escaped_feature_name_is_a_condition() {
        assert_eq!(
            substitute(
                "@media (\\70 refers-reduced-motion) { a { color: red } }",
                MotionPreference::Reduce
            )
            .text,
            format!("@media {:<27} {{ a {{ color: red }} }}", "(not (scan))")
        );
    }

    #[test]
    fn the_engine_takes_both_answers_whole() {
        crate::enable_css_features();
        let css = "@media (prefers-reduced-motion: reduce) { a { color: red } }";
        for motion in [MotionPreference::Reduce, MotionPreference::NoPreference] {
            let substituted = substitute(css, motion);
            assert_eq!(
                crate::parity::complaints(&substituted.text),
                Vec::<String>::new()
            );
        }
    }
}
