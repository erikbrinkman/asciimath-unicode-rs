//! A module for converting asciimath to unicode
//!
//! To convert asciimath quickly, you can use [`parse_unicode`] to get an [`Asciimath`] value that
//! implements [`fmt::Display`]. If you want more control, see the options exposed through [`Conf`]
//! which can [`parse`][Conf::parse] input into [`Asciimath`] as well.
//!
//! All of the input is read as math, so prose run through this comes out mangled: `it is` renders
//! as `𝑖s`. Pick the math out of prose first.
//!
//! # Usage
//!
//! ## Binary
//!
//! This crate provides a simple cli for converting asciimath to unicode:
//!
//! ```bash
//! cargo install asciimath-unicode --features binary
//! ```
//!
//! ```bash
//! asciimath-unicode -h
//! ```
//!
//! ## Library
//!
//! ```bash
//! cargo add asciimath-unicode
//! ```
//!
//! ```
//! let res = asciimath_unicode::parse_unicode("1/2").to_string();
//! assert_eq!(res, "½");
//! ```
//!
//! ```
//! use asciimath_unicode::Conf;
//! let conf = Conf::default().with_vulgar_fracs(false);
//! let res = conf.parse("1/2").to_string();
//! assert_eq!(res, "¹⁄₂");
//! ```
#![forbid(unsafe_code)]
#![warn(clippy::pedantic, missing_docs)]

mod ast;
mod block;
mod inline;
mod tokens;

use asciimath_parser::tree::Expression;
use inline::Mapper;
use std::fmt;

macro_rules! skin_tones {
    ($default:ident, $($tone:ident),+ $(,)?) => {
        /// Skin tone for emojis that take one
        ///
        /// The paired tones are for emojis with two people, like a couple, and name the two tones
        /// in order.
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum SkinTone {
            #[doc = concat!("The `", stringify!($default), "` skin tone")]
            #[default]
            $default,
            $(
                #[doc = concat!("The `", stringify!($tone), "` skin tone")]
                $tone,
            )+
        }

        impl From<SkinTone> for emojis::SkinTone {
            fn from(tone: SkinTone) -> Self {
                match tone {
                    SkinTone::$default => emojis::SkinTone::$default,
                    $(SkinTone::$tone => emojis::SkinTone::$tone,)+
                }
            }
        }

        #[cfg(test)]
        mod skin_tone_tests {
            #[test]
            fn every_tone_has_an_emoji_tone() {
                for (ours, theirs) in [
                    (super::SkinTone::$default, emojis::SkinTone::$default),
                    $((super::SkinTone::$tone, emojis::SkinTone::$tone),)+
                ] {
                    assert_eq!(emojis::SkinTone::from(ours), theirs);
                }
            }
        }
    };
}

skin_tones! {
    Default,
    Light,
    MediumLight,
    Medium,
    MediumDark,
    Dark,
    LightAndMediumLight,
    LightAndMedium,
    LightAndMediumDark,
    LightAndDark,
    MediumLightAndLight,
    MediumLightAndMedium,
    MediumLightAndMediumDark,
    MediumLightAndDark,
    MediumAndLight,
    MediumAndMediumLight,
    MediumAndMediumDark,
    MediumAndDark,
    MediumDarkAndLight,
    MediumDarkAndMediumLight,
    MediumDarkAndMedium,
    MediumDarkAndDark,
    DarkAndLight,
    DarkAndMediumLight,
    DarkAndMedium,
    DarkAndMediumDark,
}

/// Configuration for unicode rendering of asciimath
///
/// Start from [`Conf::default()`][Default::default] and change what you need, by field or with the
/// `with_*` methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
#[non_exhaustive]
pub struct Conf {
    /// Drop ( ), [ ] and { } around fractions, scripts and command arguments
    pub strip_brackets: bool,
    /// If true, this will try to render fractions as vulgar fractions
    pub vulgar_fracs: bool,
    /// Default skin tone for emojis
    pub skin_tone: SkinTone,
    /// How to lay out the math
    pub layout: Layout,
    /// What stands in for the parts that aren't there yet, or `None` to show nothing
    pub placeholders: Option<Placeholders>,
    /// Write the whitespace typed between parts of the math back out
    pub keep_spaces: bool,
}

/// How to lay out the math
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    /// One line, fractions as super- and subscripts
    #[default]
    InlineScript,
    /// One line, fractions with a slash
    InlinePlain,
    /// Several lines, with stacked fractions and grids
    Block,
}

/// What stands in for the parts of half-typed math that aren't there yet
///
/// Start from [`Placeholders::default()`][Default::default] and change what you need, by field or
/// with the `with_*` methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Placeholders {
    /// Stands in for a missing argument, fraction part, bracket pair or cell
    pub char: char,
    /// Stands in for a missing subscript
    pub sub: char,
    /// Stands in for a missing superscript
    pub sup: char,
}

impl Default for Placeholders {
    fn default() -> Self {
        Placeholders {
            char: '□',
            sub: '▫',
            sup: '⸋',
        }
    }
}

impl Placeholders {
    /// Set what stands in for a missing argument, fraction part, bracket pair or cell
    #[must_use]
    pub fn with_char(self, chr: char) -> Self {
        Placeholders { char: chr, ..self }
    }

    /// Set what stands in for a missing subscript
    #[must_use]
    pub fn with_sub(self, sub: char) -> Self {
        Placeholders { sub, ..self }
    }

    /// Set what stands in for a missing superscript
    #[must_use]
    pub fn with_sup(self, sup: char) -> Self {
        Placeholders { sup, ..self }
    }
}

impl Default for Conf {
    fn default() -> Self {
        Conf {
            strip_brackets: true,
            vulgar_fracs: true,
            skin_tone: SkinTone::Default,
            layout: Layout::InlineScript,
            placeholders: None,
            keep_spaces: false,
        }
    }
}

impl Conf {
    /// Set whether brackets that only group are dropped
    #[must_use]
    pub fn with_strip_brackets(self, strip_brackets: bool) -> Self {
        Conf {
            strip_brackets,
            ..self
        }
    }

    /// Set whether fractions are rendered as vulgar fractions
    #[must_use]
    pub fn with_vulgar_fracs(self, vulgar_fracs: bool) -> Self {
        Conf {
            vulgar_fracs,
            ..self
        }
    }

    /// Set the skin tone for emojis
    #[must_use]
    pub fn with_skin_tone(self, skin_tone: SkinTone) -> Self {
        Conf { skin_tone, ..self }
    }

    /// Set how the math is laid out
    #[must_use]
    pub fn with_layout(self, layout: Layout) -> Self {
        Conf { layout, ..self }
    }

    /// Set what stands in for the parts that aren't there yet, or `None` to show nothing
    ///
    /// This is for showing math while it is still being typed, so that `x^` or `sqrt` doesn't
    /// look the same as `x` or nothing. Brackets or a matrix cell with nothing in them count as
    /// a part being typed too, while a function name like `sin` or `f` reads fine alone and gets
    /// no placeholder.
    ///
    /// Whether the mark replaces a bracket pair with nothing in it or sits between the brackets
    /// follows [`strip_brackets`][Conf::strip_brackets]: `abs()` gives `|□|` by default and
    /// `|(□)|` with the brackets kept.
    ///
    /// A script that can't be raised or lowered is written after a literal `^` or `_`, and a
    /// script that isn't there yet then takes `char` rather than `sup` or `sub`: `x_y^` gives
    /// `x_y^□` because `y` has no subscript form, while `x_x^` gives `xₓ⸋`.
    ///
    /// ```
    /// use asciimath_unicode::{Conf, Placeholders};
    /// let conf = Conf::default().with_placeholders(Some(Placeholders::default()));
    /// assert_eq!(conf.parse("sqrt").to_string(), "√□");
    /// assert_eq!(conf.parse("x^").to_string(), "x⸋");
    /// assert_eq!(conf.parse("abs()").to_string(), "|□|");
    /// assert_eq!(conf.with_strip_brackets(false).parse("abs()").to_string(), "|(□)|");
    /// assert_eq!(conf.parse("x_y^").to_string(), "x_y^□");
    ///
    /// let marks = Placeholders::default().with_char('?').with_sup('!');
    /// let conf = conf.with_placeholders(Some(marks));
    /// assert_eq!(conf.parse("sqrt").to_string(), "√?");
    /// assert_eq!(conf.parse("x^").to_string(), "x!");
    /// ```
    #[must_use]
    pub fn with_placeholders(self, placeholders: Option<Placeholders>) -> Self {
        Conf {
            placeholders,
            ..self
        }
    }

    /// Set whether the whitespace typed between parts of the math is written back out
    ///
    /// Only whitespace between two neighboring parts is kept. Whitespace within one part, like
    /// around the `/` of a fraction or before a function's argument, is still dropped. Where none
    /// was typed, the usual spacing still applies, so a space is added wherever one is needed for
    /// legibility. A `+` or `-` that applies to the part after it rather than joining two parts is
    /// written against that part, so whitespace typed after such a sign is dropped as well.
    ///
    /// How a kept run is written depends on where it lands. A one-line layout writes it as typed,
    /// tabs and newlines included. [`Block`][Layout::Block] writes it as plain spaces as wide as
    /// the run printed, since a tab or newline would throw off the lines it stacks, and drops what
    /// was typed inside a grid. Raised or lowered, every space is written one step narrower, so a
    /// script with a space in it doesn't read as finished math.
    ///
    /// ```
    /// use asciimath_unicode::{Conf, Layout};
    /// let conf = Conf::default().with_keep_spaces(true);
    /// assert_eq!(conf.parse("a + b").to_string(), "a + b");
    /// assert_eq!(conf.parse("a+b").to_string(), "a+b");
    /// assert_eq!(conf.parse("1 / 2  x ^ 2").to_string(), "½  x²");
    /// // the second `-` is a sign on `x`, so what was typed after it goes
    /// assert_eq!(conf.parse("1 - - x").to_string(), "1 - -x");
    /// assert_eq!(conf.parse("a\tb").to_string(), "a\tb");
    /// // nothing was typed, so `sin` still gets the space it needs
    /// assert_eq!(conf.parse("sinx").to_string(), "sin x");
    /// // a thin space, where a plain one would look like the end of the script
    /// assert_eq!(conf.parse("x^(a b)").to_string(), "xᵃ\u{2009}ᵇ");
    ///
    /// let block = conf.with_layout(Layout::Block);
    /// assert_eq!(block.parse("a\tb").to_string(), "a b");
    /// assert_eq!(block.parse("[[a  b, c], [d, e]]").to_string(), "⎡ab  c⎤\n⎣ d  e⎦");
    /// assert_eq!(conf.parse("[[a  b, c], [d, e]]").to_string(), "[[a  b,c],[d,e]]");
    /// ```
    #[must_use]
    pub fn with_keep_spaces(self, keep_spaces: bool) -> Self {
        Conf {
            keep_spaces,
            ..self
        }
    }

    /// Whether one-line fractions may be written as super- and subscripts
    fn script_fracs(self) -> bool {
        self.layout != Layout::InlinePlain
    }

    /// This conf for the cells of a grid, whose typed whitespace the multi-line layout drops
    fn grid_cell(self) -> Conf {
        Conf {
            keep_spaces: self.keep_spaces && self.layout != Layout::Block,
            ..self
        }
    }

    /// Parse an asciimath string into an [`Asciimath`] value that implements [`fmt::Display`]
    #[must_use]
    pub fn parse(self, inp: &str) -> Asciimath<'_> {
        Asciimath {
            conf: self,
            expr: tokens::parse(inp, self.keep_spaces),
        }
    }
}

/// Parsed asciimath expression ready for rendering
///
/// Implements [`fmt::Display`] so it can be used with `format!`, `write!`, or `.to_string()`.
#[derive(Debug, Clone)]
pub struct Asciimath<'a> {
    conf: Conf,
    expr: Expression<'a>,
}

impl fmt::Display for Asciimath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.conf.layout == Layout::Block {
            let block = self.conf.block_expression(&self.expr);
            write!(f, "{block}")
        } else {
            self.conf.inline_expression(&self.expr, &mut Mapper::new(f))
        }
    }
}

/// Parse asciimath into an [`Asciimath`] value that implements [`fmt::Display`]
#[must_use]
pub fn parse_unicode(inp: &str) -> Asciimath<'_> {
    Conf::default().parse(inp)
}
