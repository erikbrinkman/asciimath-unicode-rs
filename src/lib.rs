#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(clippy::pedantic, missing_docs)]

mod ast;
mod block;
mod inline;
mod tokens;

use asciimath_parser::tree::Expression;
pub use asciimath_parser::{
    ASCIIMATH_TOKENS, SymbolClass, Token, TokenSpelling, asciimath_spellings,
};
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
            use std::collections::HashSet;

            #[test]
            fn every_tone_renders_differently() {
                let tones = [super::SkinTone::$default, $(super::SkinTone::$tone),+];
                let rendered: HashSet<String> = tones
                    .into_iter()
                    .map(|tone| {
                        super::Conf::default()
                            .with_skin_tone(tone)
                            .parse(":couple_with_heart:")
                            .to_string()
                    })
                    .collect();
                assert_eq!(rendered.len(), tones.len());
            }
        }
    };
}

include!("skin_tones.rs");
with_skin_tones!(skin_tones);

/// Configuration for unicode rendering of asciimath
///
/// Start from [`Conf::default()`][Default::default] and change what you need, by field or with the
/// `with_*` methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)]
#[non_exhaustive]
pub struct Conf {
    /// Drop ( ), [ ] and { } around fractions, scripts and command arguments
    pub strip_brackets: bool,
    /// Render fractions as vulgar fractions, like ½, where one exists
    pub vulgar_fracs: bool,
    /// Default skin tone for emojis
    pub skin_tone: SkinTone,
    /// How to lay out the math
    pub layout: Layout,
    /// What stands in for the parts that aren't there yet, or `None` to show nothing
    pub placeholders: Option<Placeholders>,
    /// Write the whitespace typed between parts of the math back out
    pub keep_spaces: bool,
    /// Put a space on either side of an operator that joins two parts of the math, except inside
    /// a raised or lowered script
    pub spaced_operators: bool,
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
            spaced_operators: false,
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
    #[must_use]
    pub fn with_placeholders(self, placeholders: Option<Placeholders>) -> Self {
        Conf {
            placeholders,
            ..self
        }
    }

    /// Set whether the whitespace typed between parts of the math is written back out
    #[must_use]
    pub fn with_keep_spaces(self, keep_spaces: bool) -> Self {
        Conf {
            keep_spaces,
            ..self
        }
    }

    /// Set whether an operator that joins two parts of the math gets a space on either side
    #[must_use]
    pub fn with_spaced_operators(self, spaced_operators: bool) -> Self {
        Conf {
            spaced_operators,
            ..self
        }
    }

    /// Whether fractions may be written as super- and subscripts; only the one-line script
    /// layout does
    fn script_fracs(self) -> bool {
        self.layout == Layout::InlineScript
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.conf.layout == Layout::Block {
            let block = self
                .conf
                .block_expression(&self.expr, inline::Font::default());
            write!(formatter, "{block}")
        } else {
            self.conf
                .inline_expression(&self.expr, &mut Mapper::new(formatter))
        }
    }
}

/// Parse asciimath into an [`Asciimath`] value that implements [`fmt::Display`]
#[must_use]
pub fn parse_unicode(inp: &str) -> Asciimath<'_> {
    Conf::default().parse(inp)
}
