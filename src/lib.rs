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

impl Default for Conf {
    fn default() -> Self {
        Conf {
            strip_brackets: true,
            vulgar_fracs: true,
            skin_tone: SkinTone::Default,
            layout: Layout::InlineScript,
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

    /// Whether one-line fractions may be written as super- and subscripts
    fn script_fracs(self) -> bool {
        self.layout != Layout::InlinePlain
    }

    /// Parse an asciimath string into an [`Asciimath`] value that implements [`fmt::Display`]
    #[must_use]
    pub fn parse(self, inp: &str) -> Asciimath<'_> {
        Asciimath {
            conf: self,
            expr: tokens::parse(inp),
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
