//! A module for converting asciimath to unicode
//!
//! To convert asciimath quickly, you can use [`parse_unicode`] to get an [`Asciimath`] value that
//! implements [`fmt::Display`]. If you want more control, see the options exposed through [`Conf`]
//! which can [`parse`][Conf::parse] input into [`Asciimath`] as well.
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
//! let conf = Conf {
//!     vulgar_fracs: false,
//!     ..Default::default()
//! };
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
pub use emojis::SkinTone;
use inline::Mapper;
use std::fmt;

/// Configuration for unicode rendering of asciimath
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Conf {
    /// If true, this will strip unnecessary parenthesis in some contexts
    pub strip_brackets: bool,
    /// If true, this will try to render fractions as vulgar fractions
    pub vulgar_fracs: bool,
    /// If true, this will try to render fractions using super- and sub-scripts
    pub script_fracs: bool,
    /// Default skin tone for emojis
    pub skin_tone: SkinTone,
    /// If true, render as multi-line 2D block (stacked fractions, vertical scripts, matrix grids)
    pub block: bool,
}

impl Default for Conf {
    fn default() -> Self {
        Conf {
            strip_brackets: true,
            vulgar_fracs: true,
            script_fracs: true,
            skin_tone: SkinTone::Default,
            block: false,
        }
    }
}

impl Conf {
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
    /// Rendering configuration
    pub conf: Conf,
    /// The parsed expression
    pub expr: Expression<'a>,
}

impl fmt::Display for Asciimath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.conf.block {
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
