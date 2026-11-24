//! The safe area, the keyboard and the text scale a stylesheet reads with `env()`.
//!
//! The style engine answers `env(safe-area-inset-*)` from a device that always reports zero, does
//! not know the other variables here, and the device has no setter. [`substitute`] thus writes
//! each reference to these variables as a `var()` of a custom property before the text is parsed.
//! [`rule`] gives those properties their values on the root element, and every other element
//! inherits them.
//!
//! | Variable | Value |
//! |---|---|
//! | `safe-area-inset-top`, `-right`, `-bottom`, `-left` | a length |
//! | `keyboard-inset-top`, `-right`, `-bottom`, `-left`, `-width`, `-height` | a length |
//! | `keyboard-animation-duration` | the time the keyboard takes to move, while one is known |
//! | `keyboard-animation-easing` | the curve the keyboard moves along, while one is known |
//! | `preferred-text-scale` | the number the user's text size setting multiplies text by |
//!
//! The substitution keeps the byte offset of every token, so a parse error reports the location it
//! has in the source text.

use std::borrow::Cow;
use std::fmt::Write;
use std::ops::Range;
use std::time::Duration;

use cssparser::{ParseError, Parser, ParserInput, Token};
use zgui_geom::{Css, CssPx, Edges, Rect, Size};

/// One environment variable, by the name `env()` reads it with and by its custom property.
struct Variable {
    /// The environment variable name.
    name: &'static str,
    /// The custom property it is substituted with.
    ///
    /// Each property is shorter than its variable name, so the substitution fits in the bytes of
    /// the reference.
    property: &'static str,
}

/// The safe area insets, in the order top, right, bottom, left.
const SAFE_AREA: [Variable; 4] = [
    Variable {
        name: "safe-area-inset-top",
        property: "--zgui-safe-top",
    },
    Variable {
        name: "safe-area-inset-right",
        property: "--zgui-safe-right",
    },
    Variable {
        name: "safe-area-inset-bottom",
        property: "--zgui-safe-bottom",
    },
    Variable {
        name: "safe-area-inset-left",
        property: "--zgui-safe-left",
    },
];

/// The keyboard insets of the VirtualKeyboard API, in the order top, right, bottom, left, width,
/// height.
const KEYBOARD: [Variable; 6] = [
    Variable {
        name: "keyboard-inset-top",
        property: "--zgui-kb-top",
    },
    Variable {
        name: "keyboard-inset-right",
        property: "--zgui-kb-right",
    },
    Variable {
        name: "keyboard-inset-bottom",
        property: "--zgui-kb-bottom",
    },
    Variable {
        name: "keyboard-inset-left",
        property: "--zgui-kb-left",
    },
    Variable {
        name: "keyboard-inset-width",
        property: "--zgui-kb-width",
    },
    Variable {
        name: "keyboard-inset-height",
        property: "--zgui-kb-height",
    },
];

/// The movement of the keyboard, in the order duration, easing.
const KEYBOARD_MOTION: [Variable; 2] = [
    Variable {
        name: "keyboard-animation-duration",
        property: "--zgui-kb-duration",
    },
    Variable {
        name: "keyboard-animation-easing",
        property: "--zgui-kb-easing",
    },
];

/// The text scale.
const TEXT_SCALE: Variable = Variable {
    name: "preferred-text-scale",
    property: "--zgui-text-scale",
};

/// The values each variable has, as of one frame.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Environment {
    /// The size of the viewport.
    pub viewport: Size<CssPx, Css>,
    /// How far the system interface covers each edge of the viewport.
    pub safe_area: Edges<CssPx>,
    /// The part of the viewport an on-screen keyboard covers, or `None` when no keyboard shows.
    pub keyboard: Option<Rect<CssPx, Css>>,
    /// How the keyboard moved last, or `None` when the platform did not say.
    pub keyboard_motion: Option<KeyboardMotion>,
    /// The number the user's text size setting multiplies text by.
    pub text_scale: TextScale,
}

/// How an on-screen keyboard moves between two frames.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct KeyboardMotion {
    /// The time the movement takes.
    pub duration: Duration,
    /// The curve of the movement.
    pub easing: KeyboardEasing,
}

impl KeyboardMotion {
    /// How far the movement has come `elapsed` after its start, where zero is the start and one is
    /// the end. A spring can go past one before it settles.
    pub fn progress(self, elapsed: Duration) -> f32 {
        if elapsed >= self.duration {
            return 1.0;
        }
        match self.easing {
            KeyboardEasing::CubicBezier(CubicBezier { x1, y1, x2, y2 }) => {
                let linear = elapsed.as_secs_f64() / self.duration.as_secs_f64();
                style::bezier::Bezier::calculate_bezier_output(linear, 1e-6, x1, y1, x2, y2) as f32
            }
            KeyboardEasing::Spring(spring) => spring.progress(elapsed.as_secs_f64()) as f32,
        }
    }
}

/// The curve an on-screen keyboard moves along.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum KeyboardEasing {
    /// A curve `cubic-bezier()` gives.
    CubicBezier(CubicBezier),
    /// A spring released from rest at the start, which `env()` gives as a `linear()` curve.
    Spring(Spring),
}

/// An easing curve, by the two control points of `cubic-bezier()`.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct CubicBezier {
    /// The horizontal position of the first control point, from 0 to 1.
    pub x1: f32,
    /// The vertical position of the first control point.
    pub y1: f32,
    /// The horizontal position of the second control point, from 0 to 1.
    pub x2: f32,
    /// The vertical position of the second control point.
    pub y2: f32,
}

/// A damped spring, by the coefficients of its equation of motion.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Spring {
    /// The mass on the spring.
    pub mass: f32,
    /// The stiffness of the spring.
    pub stiffness: f32,
    /// The damping coefficient.
    pub damping: f32,
}

impl Spring {
    /// How far the spring has moved from its start to its end `seconds` after its release, where
    /// zero is the start and one is the end.
    fn progress(self, seconds: f64) -> f64 {
        let mass = f64::from(self.mass);
        let stiffness = f64::from(self.stiffness);
        let damping = f64::from(self.damping);
        let natural = (stiffness / mass).sqrt();
        let ratio = damping / (2.0 * (stiffness * mass).sqrt());

        let remaining = if ratio < 1.0 {
            let damped = natural * (1.0 - ratio * ratio).sqrt();
            (-ratio * natural * seconds).exp()
                * ((damped * seconds).cos() + ratio * natural / damped * (damped * seconds).sin())
        } else if ratio == 1.0 {
            (-natural * seconds).exp() * (1.0 + natural * seconds)
        } else {
            let root = (ratio * ratio - 1.0).sqrt();
            let slow = natural * (-ratio + root);
            let fast = natural * (-ratio - root);
            (fast * (slow * seconds).exp() - slow * (fast * seconds).exp()) / (fast - slow)
        };

        1.0 - remaining
    }
}

/// The number the user's text size setting multiplies text by.
///
/// One is the size the system uses by default. A stylesheet reads it as
/// `env(preferred-text-scale)` and applies it where it wants text to follow the setting, for
/// example `:root { font-size: calc(100% * env(preferred-text-scale)) }`.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub struct TextScale(f32);

impl TextScale {
    /// The scale of text at the size the system uses by default.
    pub const DEFAULT: Self = Self(1.0);

    /// The scale `factor`, when it is finite and larger than zero.
    pub fn new(factor: f32) -> Option<Self> {
        (factor.is_finite() && factor > 0.0).then_some(Self(factor))
    }

    /// The factor.
    pub fn get(self) -> f32 {
        self.0
    }
}

impl Default for TextScale {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One span of source text and the text that replaces it.
struct Replacement {
    /// The bytes replaced.
    span: Range<usize>,
    /// The text put at the start of the span. Spaces fill the remainder.
    text: &'static str,
}

/// `css` with each `env()` reference to a variable of this module written as a `var()` of its
/// custom property.
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

/// A rule that sets the custom property of each variable on the root element.
///
/// The keyboard insets are the distances of the keyboard's edges from the edges of the viewport,
/// and its width and height. They are zero when no keyboard shows, as the VirtualKeyboard API
/// specifies. The rule leaves the keyboard motion properties unset while the motion is unknown,
/// so a reference takes its fallback.
pub fn rule(environment: &Environment) -> String {
    let Environment {
        viewport,
        safe_area,
        keyboard,
        keyboard_motion,
        text_scale,
    } = *environment;
    let keyboard = keyboard.map_or([0.0; 6], |area| {
        [
            area.top().0,
            viewport.width.0 - area.right().0,
            viewport.height.0 - area.bottom().0,
            area.left().0,
            area.width().0,
            area.height().0,
        ]
    });
    let values = safe_area
        .into_array()
        .map(|inset| inset.0)
        .into_iter()
        .chain(keyboard);

    let mut rule = String::from(":root {");
    for (variable, value) in SAFE_AREA.iter().chain(&KEYBOARD).zip(values) {
        let _ = write!(rule, " {}: {value}px;", variable.property);
    }
    if let Some(KeyboardMotion { duration, easing }) = keyboard_motion {
        let [duration_property, easing_property] =
            KEYBOARD_MOTION.map(|variable| variable.property);
        let milliseconds = duration.as_secs_f64() * 1000.0;
        let _ = write!(
            rule,
            " {duration_property}: {milliseconds}ms; {easing_property}: "
        );
        write_easing(&mut rule, easing, duration);
        rule.push(';');
    }
    let _ = write!(rule, " {}: {};", TEXT_SCALE.property, text_scale.get());
    rule.push_str(" }");

    rule
}

/// Writes `easing` as a CSS easing function, for a movement that takes `duration`.
///
/// A spring is sampled at every twentieth of the duration. Its last point is the end, so the
/// curve ends where the movement does.
fn write_easing(css: &mut String, easing: KeyboardEasing, duration: Duration) {
    const SAMPLES: u32 = 20;

    match easing {
        KeyboardEasing::CubicBezier(CubicBezier { x1, y1, x2, y2 }) => {
            let _ = write!(css, "cubic-bezier({x1}, {y1}, {x2}, {y2})");
        }
        KeyboardEasing::Spring(spring) => {
            css.push_str("linear(0");
            for sample in 1..SAMPLES {
                let fraction = f64::from(sample) / f64::from(SAMPLES);
                let progress = spring.progress(fraction * duration.as_secs_f64());
                let percent = fraction * 100.0;
                let _ = write!(css, ", {progress:.4} {percent}%");
            }
            css.push_str(", 1)");
        }
    }
}

/// Records the replacements for every reference to a variable in what `parser` has left, in source
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

/// Records the replacements for the arguments of one `env()`, when they name a variable.
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
        let Some(variable) = SAFE_AREA
            .iter()
            .chain(&KEYBOARD)
            .chain(&KEYBOARD_MOTION)
            .chain([&TEXT_SCALE])
            .find(|variable| variable.name == &**name)
        else {
            return;
        };

        replacements.push(Replacement {
            span: function,
            text: "var(",
        });
        replacements.push(Replacement {
            span: start.byte_index()..block.position().byte_index(),
            text: variable.property,
        });
        return;
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::time::Duration;

    use zgui_geom::{CssPx, Edges, Point, Rect, Size};

    use super::CubicBezier;
    use super::Environment;
    use super::KeyboardEasing;
    use super::KeyboardMotion;
    use super::Spring;
    use super::TextScale;
    use super::{rule, substitute};

    /// The environment of a 390 by 844 viewport with nothing in it.
    fn plain() -> Environment {
        Environment {
            viewport: Size::new(CssPx(390.0), CssPx(844.0)),
            safe_area: Edges::uniform(CssPx(0.0)),
            keyboard: None,
            keyboard_motion: None,
            text_scale: TextScale::DEFAULT,
        }
    }

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
    fn each_keyboard_inset_becomes_its_custom_property() {
        assert_eq!(
            substitute(
                "a { top: env(keyboard-inset-top); right: env(keyboard-inset-right); \
                 bottom: env(keyboard-inset-bottom, 4px); left: env(keyboard-inset-left); \
                 width: env(keyboard-inset-width); height: env(keyboard-inset-height); }"
            ),
            "a { top: var(--zgui-kb-top     ); right: var(--zgui-kb-right     ); \
             bottom: var(--zgui-kb-bottom     , 4px); left: var(--zgui-kb-left     ); \
             width: var(--zgui-kb-width     ); height: var(--zgui-kb-height     ); }"
        );
    }

    #[test]
    fn the_keyboard_motion_and_the_text_scale_become_their_custom_properties() {
        assert_eq!(
            substitute(
                "a { transition: bottom env(keyboard-animation-duration, 0s) \
                 env(keyboard-animation-easing); font-size: calc(1rem * \
                 env(preferred-text-scale)); }"
            ),
            "a { transition: bottom var(--zgui-kb-duration         , 0s) \
             var(--zgui-kb-easing         ); font-size: calc(1rem * \
             var(--zgui-text-scale   )); }"
        );
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
                   url(env(safe-area-inset-top)); top: env(titlebar-area-y); \
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
        let environment = Environment {
            safe_area: Edges::new(CssPx(59.0), CssPx(0.0), CssPx(34.0), CssPx(0.5)),
            ..plain()
        };
        assert_eq!(
            rule(&environment),
            ":root { --zgui-safe-top: 59px; --zgui-safe-right: 0px; --zgui-safe-bottom: 34px; \
             --zgui-safe-left: 0.5px; --zgui-kb-top: 0px; --zgui-kb-right: 0px; \
             --zgui-kb-bottom: 0px; --zgui-kb-left: 0px; --zgui-kb-width: 0px; \
             --zgui-kb-height: 0px; --zgui-text-scale: 1; }"
        );
    }

    #[test]
    fn the_keyboard_insets_are_measured_from_each_edge_of_the_viewport() {
        let keyboard = Rect::new(
            Point::new(CssPx(10.0), CssPx(508.0)),
            Size::new(CssPx(370.0), CssPx(300.0)),
        );
        let environment = Environment {
            keyboard: Some(keyboard),
            ..plain()
        };
        assert_eq!(
            rule(&environment),
            ":root { --zgui-safe-top: 0px; --zgui-safe-right: 0px; --zgui-safe-bottom: 0px; \
             --zgui-safe-left: 0px; --zgui-kb-top: 508px; --zgui-kb-right: 10px; \
             --zgui-kb-bottom: 36px; --zgui-kb-left: 10px; --zgui-kb-width: 370px; \
             --zgui-kb-height: 300px; --zgui-text-scale: 1; }"
        );
    }

    #[test]
    fn a_known_keyboard_motion_and_a_larger_text_scale_are_set() {
        let environment = Environment {
            keyboard_motion: Some(KeyboardMotion {
                duration: Duration::from_millis(250),
                easing: KeyboardEasing::CubicBezier(CubicBezier {
                    x1: 0.25,
                    y1: 0.1,
                    x2: 0.25,
                    y2: 1.0,
                }),
            }),
            text_scale: TextScale::new(1.5).expect("a positive scale"),
            ..plain()
        };
        assert!(rule(&environment).ends_with(
            "--zgui-kb-height: 0px; --zgui-kb-duration: 250ms; \
             --zgui-kb-easing: cubic-bezier(0.25, 0.1, 0.25, 1); --zgui-text-scale: 1.5; }"
        ));
    }

    #[test]
    fn a_spring_is_sampled_as_a_linear_curve_that_ends_at_one() {
        let environment = Environment {
            keyboard_motion: Some(KeyboardMotion {
                duration: Duration::from_millis(500),
                easing: KeyboardEasing::Spring(Spring {
                    mass: 1.0,
                    stiffness: 100.0,
                    damping: 20.0,
                }),
            }),
            ..plain()
        };
        let css = rule(&environment);
        assert!(
            css.contains("--zgui-kb-easing: linear(0, 0.0265 5%, "),
            "{css}"
        );
        assert!(css.contains(", 0.9503 95%, 1);"), "{css}");
    }

    #[test]
    fn the_three_kinds_of_damping_meet_at_critical_damping() {
        let spring = |damping| Spring {
            mass: 3.0,
            stiffness: 1000.0,
            damping,
        };
        let critical = 2.0 * (1000.0_f32 * 3.0).sqrt();
        for seconds in [0.0, 0.05, 0.1, 0.25, 0.5] {
            let at = spring(critical).progress(seconds);
            let under = spring(critical * 0.9999).progress(seconds);
            let over = spring(critical * 1.0001).progress(seconds);
            assert!((at - under).abs() < 1e-3, "{seconds}: {at} {under}");
            assert!((at - over).abs() < 1e-3, "{seconds}: {at} {over}");
        }
        assert_eq!(spring(critical).progress(0.0), 0.0);
        assert!(spring(critical).progress(0.5) > 0.998);
    }

    #[test]
    fn a_keyboard_movement_follows_its_curve_and_ends_at_its_duration() {
        let ease_out = KeyboardMotion {
            duration: Duration::from_millis(250),
            easing: KeyboardEasing::CubicBezier(CubicBezier {
                x1: 0.0,
                y1: 0.0,
                x2: 0.58,
                y2: 1.0,
            }),
        };
        assert_eq!(ease_out.progress(Duration::ZERO), 0.0);
        let half = ease_out.progress(Duration::from_millis(125));
        assert!(half > 0.5 && half < 1.0, "{half}");
        assert_eq!(ease_out.progress(Duration::from_millis(250)), 1.0);

        let spring = KeyboardMotion {
            duration: Duration::from_millis(500),
            easing: KeyboardEasing::Spring(Spring {
                mass: 1.0,
                stiffness: 100.0,
                damping: 20.0,
            }),
        };
        let early = spring.progress(Duration::from_millis(25));
        assert!((early - 0.0265).abs() < 1e-3, "{early}");
        assert_eq!(spring.progress(Duration::from_secs(1)), 1.0);
    }

    #[test]
    fn a_text_scale_is_finite_and_positive() {
        assert_eq!(TextScale::new(0.0), None);
        assert_eq!(TextScale::new(-1.0), None);
        assert_eq!(TextScale::new(f32::NAN), None);
        assert_eq!(TextScale::new(2.0).map(TextScale::get), Some(2.0));
    }
}
