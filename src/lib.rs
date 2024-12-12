//! A module for converting asciimath to unicode
//!
//! To convert asciimath quickly, you can use the [`write_unicode`] or [`convert_unicode`] methods.
//! If you want more control, see the options exposed through [`InlineRenderer`] which can be
//! [rendered][InlineRenderer::render] into [`RenderedUnicode`].
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
//! cargo add asciimath-parser
//! ```
//!
//! ```
//! let res = asciimath_unicode::convert_unicode("1/2");
//! assert_eq!(res, "½");
//! ```
//!
//! ```
//! use asciimath_unicode::InlineRenderer;
//! let renderer = InlineRenderer {
//!     vulgar_fracs: false,
//!     ..Default::default()
//! };
//! let res: String = renderer.render("1/2").collect();
//! assert_eq!(res, "¹⁄₂");
//! ```
#![forbid(unsafe_code)]
#![warn(clippy::pedantic, missing_docs)]

mod iter;
mod render_chars;
mod tokens;

use asciimath_parser::tree::{
    Expression, Frac, Func, Group, Intermediate, Matrix, Script, ScriptFunc, Simple, SimpleBinary,
    SimpleFunc, SimpleScript, SimpleUnary,
};
use asciimath_parser::Tokenizer;
pub use emojis::SkinTone;
use iter::{Interleave, Modified};
use render_chars::{enum_iter, struct_iter, RenderChars};
use std::array;
use std::fmt;
use std::io;
use std::io::Write;
use std::iter::{Chain, Flatten, FusedIterator, Map};
use std::str::Chars;
use std::vec;
use tokens::{
    bold_map, cal_map, double_map, frak_map, italic_map, left_bracket_str, mono_map,
    right_bracket_str, sans_map, subscript_char, superscript_char, symbol_str, TOKEN_MAP,
};

type CharIter = array::IntoIter<char, 1>;

type GenericBinaryIter<'a> = Chain<
    Chain<Chain<Chain<Chars<'a>, CharIter>, Box<SimpleIter<'a>>>, CharIter>,
    Box<SimpleIter<'a>>,
>;

type CharMap<I> = Map<I, fn(char) -> char>;

type Delim<'a, I> = Chain<Chain<Chars<'a>, I>, Chars<'a>>;

type SimpleFuncIter<'a> = Chain<Chain<Chars<'a>, CharIter>, Box<SimpleIter<'a>>>;

type GroupIter<'a> = Delim<'a, Box<ExpressionIter<'a>>>;

type SimpleScriptIter<'a> = Chain<SimpleIter<'a>, ScriptIter<'a>>;

type FuncIter<'a> =
    Chain<Chain<Chain<Chars<'a>, ScriptIter<'a>>, CharIter>, Box<ScriptFuncIter<'a>>>;

type ExpressionIter<'a> = Flatten<vec::IntoIter<IntermediateIter<'a>>>;

type MatIter<'a> = Delim<'a, Box<Interleave<Delim<'a, Interleave<ExpressionIter<'a>>>>>>;

struct_iter! { SimpMappedIter : CharMap<SimpleIter<'a>> }

struct_iter! { FuncMappedIter : CharMap<ScriptFuncIter<'a>> }

struct_iter! { ExprMappedIter : CharMap<ExpressionIter<'a>> }

enum_iter! { SimpleUnaryIter :
    Simple => Chain<CharIter, Box<SimpleIter<'a>>>,
    Font => Box<CharMap<SimpleIter<'a>>>,
    StrippedFont => Box<CharMap<ExpressionIter<'a>>>,
    Wrapped => Delim<'a, Box<SimpleIter<'a>>>,
    StrippedWrapped => Delim<'a, Box<ExpressionIter<'a>>>,
    Single => Chain<Box<SimpleIter<'a>>, CharIter>,
    StrippedSingle => Chain<Box<ExpressionIter<'a>>, CharIter>,
    Moded => Box<Modified<SimpleIter<'a>>>,
    StrippedModed => Box<Modified<ExpressionIter<'a>>>,
    Generic => Chain<Chain<Chars<'a>, CharIter>, Box<SimpleIter<'a>>>,
}

enum_iter! { SimpleFracIter :
    Vulgar => CharIter,
    VulgOne => Chain<CharIter, SimpMappedIter<'a>>,
    StrippedVulgOne => Chain<CharIter, Box<ExprMappedIter<'a>>>,
    StrippedScript => Box<Chain<Chain<ExprMappedIter<'a>, CharIter>, ExprMappedIter<'a>>>,
    DenomStrippedScript => Chain<Chain<SimpMappedIter<'a>, CharIter>, Box<ExprMappedIter<'a>>>,
    NumerStrippedScript => Chain<Chain<Box<ExprMappedIter<'a>>, CharIter>, SimpMappedIter<'a>>,
    Script => Chain<Chain<SimpMappedIter<'a>, CharIter>, SimpMappedIter<'a>>,
    Simple => Chain<Chain<SimpleIter<'a>, CharIter>, SimpleIter<'a>>,
}

enum_iter! { SimpleBinaryIter :
    Simple => Chain<CharIter, Box<SimpleIter<'a>>>,
    ExprComb => Chain<Box<ExpressionIter<'a>>, CharIter>,
    Comb => Chain<Box<SimpleIter<'a>>, CharIter>,
    Char => CharIter,
    Frac => Box<SimpleFracIter<'a>>,
    Generic => GenericBinaryIter<'a>,
}

enum_iter! { SimpleIter :
    Chars => Chars<'a>,
    Func => SimpleFuncIter<'a>,
    Unary => SimpleUnaryIter<'a>,
    Binary => SimpleBinaryIter<'a>,
    Group => GroupIter<'a>,
    Matrix => MatIter<'a>,
}

enum_iter! { ScriptIter :
    Empty => Chars<'static>,
    Untouched => Chain<CharIter, SimpleIter<'a>>,
    Mapped => CharMap<SimpleIter<'a>>,
    SubsupUntouched => Chain<Chain<Chain<CharIter, SimpleIter<'a>>, CharIter>, SimpleIter<'a>>,
    SubsupMapped => Chain<SimpMappedIter<'a>, SimpMappedIter<'a>>,
}

enum_iter! { ScriptFuncIter :
    Simple => SimpleScriptIter<'a>,
    Func => FuncIter<'a>,
}

enum_iter! { FracIter :
    Simple => SimpleFracIter<'a>,
    NumerStrippedScript => Chain<Chain<Box<ExprMappedIter<'a>>, CharIter>, FuncMappedIter<'a>>,
    DenomStrippedScript => Chain<Chain<FuncMappedIter<'a>, CharIter>, Box<ExprMappedIter<'a>>>,
    Script => Chain<Chain<FuncMappedIter<'a>, CharIter>, FuncMappedIter<'a>>,
    VulgOne => Chain<CharIter, FuncMappedIter<'a>>,
    One => Chain<Chars<'static>, ScriptFuncIter<'a>>,
    Func => Chain<Chain<ScriptFuncIter<'a>, CharIter>, ScriptFuncIter<'a>>,
}

enum_iter! { IntermediateIter :
    ScriptFunc => ScriptFuncIter<'a>,
    Frac => FracIter<'a>,
}

macro_rules! num {
    ($num:pat) => {
        Simple::Number($num)
    };
}

macro_rules! iden {
    ($idn:pat) => {
        Simple::Ident($idn)
    };
}

macro_rules! symb {
    ($sym:pat) => {
        Simple::Symbol($sym)
    };
}

macro_rules! script_func {
    ($simp:pat) => {
        ScriptFunc::Simple(SimpleScript {
            simple: $simp,
            script: Script::None,
        })
    };
}

macro_rules! sgroup {
    ($expr:pat) => {
        Simple::Group(Group { expr: $expr, .. })
    };
}

macro_rules! xnum {
    ($expr:expr, $num:pat) => {
        matches!(
            **$expr,
            [Intermediate::ScriptFunc(script_func!(num!($num)))]
        )
    };
}

macro_rules! xiden {
    ($expr:expr, $num:pat) => {
        matches!(
            **$expr,
            [Intermediate::ScriptFunc(script_func!(iden!($num)))]
        )
    };
}

#[inline]
fn vulg<'a>(vulgar: char) -> RenderChars<SimpleFracIter<'a>> {
    RenderChars::from(vulgar).map(SimpleFracIter::Vulgar)
}

#[inline]
fn gsfrac<'a>(
    num: RenderChars<SimpleIter<'a>>,
    den: RenderChars<SimpleIter<'a>>,
) -> RenderChars<SimpleFracIter<'a>> {
    num.chain(RenderChars::from('/'))
        .chain(den)
        .map(SimpleFracIter::Simple)
}

#[inline]
fn gfrac<'a>(
    num: RenderChars<ScriptFuncIter<'a>>,
    den: RenderChars<ScriptFuncIter<'a>>,
) -> RenderChars<FracIter<'a>> {
    num.chain(RenderChars::from('/'))
        .chain(den)
        .map(FracIter::Func)
}

/// An inline unicode renderer for asciimath
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRenderer {
    /// If true, this will strip unnecessary parenthesis in some contexts
    pub strip_brackets: bool,
    /// If true, this will try to render fractions as vulgar fractions
    pub vulgar_fracs: bool,
    /// If true, this will try to render fractions using super- and sub-scripts
    pub script_fracs: bool,
    /// Default skin tone for emojis
    pub skin_tone: SkinTone,
}

impl Default for InlineRenderer {
    fn default() -> Self {
        InlineRenderer {
            strip_brackets: true,
            vulgar_fracs: true,
            script_fracs: true,
            skin_tone: SkinTone::Default,
        }
    }
}

impl InlineRenderer {
    fn render_simplefunc<'a>(&self, simple: &SimpleFunc<'a>) -> RenderChars<SimpleFuncIter<'a>> {
        RenderChars::from(simple.func)
            .chain(RenderChars::from(' '))
            .chain(self.render_simple(simple.arg()).map(Box::new))
    }

    #[inline]
    fn render_root<'a>(
        &self,
        root_char: char,
        arg: &Simple<'a>,
    ) -> RenderChars<SimpleBinaryIter<'a>> {
        RenderChars::from(root_char)
            .chain(self.render_simple(arg).map(Box::new))
            .map(SimpleBinaryIter::Simple)
    }

    #[inline]
    fn cover<'a>(
        &self,
        op: &'a str,
        first: &Simple<'a>,
        arg: &Simple<'a>,
        chr: char,
    ) -> RenderChars<SimpleBinaryIter<'a>> {
        match arg {
            sgroup!(expr) if self.strip_brackets => {
                let rendered = self.render_expression(expr);
                if rendered.len == 1 {
                    rendered
                        .map(Box::new)
                        .chain(RenderChars::from(chr))
                        .map(SimpleBinaryIter::ExprComb)
                } else {
                    self.render_bgeneric(op, first, arg)
                }
            }
            arg => {
                let rendered = self.render_simple(arg);
                if rendered.len == 1 {
                    rendered
                        .map(Box::new)
                        .chain(RenderChars::from(chr))
                        .map(SimpleBinaryIter::Comb)
                } else {
                    self.render_bgeneric(op, first, arg)
                }
            }
        }
    }

    #[inline]
    fn render_equals<'a>(
        &self,
        iter: impl Iterator<Item = char> + Clone,
        op: &'a str,
        first: &Simple<'a>,
        second: &Simple<'a>,
    ) -> RenderChars<SimpleBinaryIter<'a>> {
        if iter.clone().eq("∘".chars()) {
            RenderChars::from('\u{2257}').map(SimpleBinaryIter::Char)
        } else if iter.clone().eq("⋆".chars()) {
            RenderChars::from('\u{225b}').map(SimpleBinaryIter::Char)
        } else if iter.clone().eq("△".chars()) {
            RenderChars::from('\u{225c}').map(SimpleBinaryIter::Char)
        } else if iter.clone().eq("def".chars()) {
            RenderChars::from('\u{225d}').map(SimpleBinaryIter::Char)
        } else if iter.clone().eq("m".chars()) {
            RenderChars::from('\u{225e}').map(SimpleBinaryIter::Char)
        } else if iter.eq("?".chars()) {
            RenderChars::from('\u{225f}').map(SimpleBinaryIter::Char)
        } else {
            self.render_bgeneric(op, first, second)
        }
    }

    #[inline]
    fn render_bgeneric<'a>(
        &self,
        op: &'a str,
        first: &Simple<'a>,
        second: &Simple<'a>,
    ) -> RenderChars<SimpleBinaryIter<'a>> {
        RenderChars::from(op)
            .chain(RenderChars::from(' '))
            .chain(self.render_simple(first).map(Box::new))
            .chain(RenderChars::from(' '))
            .chain(self.render_simple(second).map(Box::new))
            .map(SimpleBinaryIter::Generic)
    }

    fn render_simplebinary<'a>(
        &self,
        simple: &SimpleBinary<'a>,
    ) -> RenderChars<SimpleBinaryIter<'a>> {
        let sb = self.strip_brackets;
        match (simple.op, simple.first(), simple.second()) {
            // roots
            ("root", num!("2"), arg) => self.render_root('√', arg),
            ("root", num!("3"), arg) => self.render_root('∛', arg),
            ("root", num!("4"), arg) => self.render_root('∜', arg),
            ("root", sgroup!(expr), arg) if xnum!(expr, "2") => self.render_root('√', arg),
            ("root", sgroup!(expr), arg) if xnum!(expr, "3") => self.render_root('∛', arg),
            ("root", sgroup!(expr), arg) if xnum!(expr, "4") => self.render_root('∜', arg),
            // frac
            ("frac", numer, denom) => self
                .render_simplefrac(numer, denom)
                .map(|iter| SimpleBinaryIter::Frac(Box::new(iter))),
            // stackrel / overset combining
            (o @ ("stackrel" | "overset"), f @ iden!("a"), a) => self.cover(o, f, a, '\u{0363}'),
            (o @ ("stackrel" | "overset"), f @ iden!("e"), a) => self.cover(o, f, a, '\u{0364}'),
            (o @ ("stackrel" | "overset"), f @ iden!("i"), a) => self.cover(o, f, a, '\u{0365}'),
            (o @ ("stackrel" | "overset"), f @ iden!("o"), a) => self.cover(o, f, a, '\u{0366}'),
            (o @ ("stackrel" | "overset"), f @ iden!("u"), a) => self.cover(o, f, a, '\u{0367}'),
            (o @ ("stackrel" | "overset"), f @ iden!("c"), a) => self.cover(o, f, a, '\u{0368}'),
            (o @ ("stackrel" | "overset"), f @ iden!("d"), a) => self.cover(o, f, a, '\u{0369}'),
            (o @ ("stackrel" | "overset"), f @ iden!("h"), a) => self.cover(o, f, a, '\u{036a}'),
            (o @ ("stackrel" | "overset"), f @ iden!("m"), a) => self.cover(o, f, a, '\u{036b}'),
            (o @ ("stackrel" | "overset"), f @ iden!("r"), a) => self.cover(o, f, a, '\u{036c}'),
            (o @ ("stackrel" | "overset"), f @ iden!("t"), a) => self.cover(o, f, a, '\u{036d}'),
            (o @ ("stackrel" | "overset"), f @ iden!("v"), a) => self.cover(o, f, a, '\u{036e}'),
            (o @ ("stackrel" | "overset"), f @ iden!("x"), a) => self.cover(o, f, a, '\u{036f}'),
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "a") => {
                self.cover(simple.op, simple.first(), arg, '\u{0363}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "e") => {
                self.cover(simple.op, simple.first(), arg, '\u{0364}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "i") => {
                self.cover(simple.op, simple.first(), arg, '\u{0365}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "o") => {
                self.cover(simple.op, simple.first(), arg, '\u{0366}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "u") => {
                self.cover(simple.op, simple.first(), arg, '\u{0367}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "c") => {
                self.cover(simple.op, simple.first(), arg, '\u{0368}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "d") => {
                self.cover(simple.op, simple.first(), arg, '\u{0369}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "h") => {
                self.cover(simple.op, simple.first(), arg, '\u{036a}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "m") => {
                self.cover(simple.op, simple.first(), arg, '\u{036b}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "r") => {
                self.cover(simple.op, simple.first(), arg, '\u{036c}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "t") => {
                self.cover(simple.op, simple.first(), arg, '\u{036d}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "v") => {
                self.cover(simple.op, simple.first(), arg, '\u{036e}')
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "x") => {
                self.cover(simple.op, simple.first(), arg, '\u{036f}')
            }
            // stackrel / overset equals
            ("stackrel" | "overset", arg, symb!("=")) => match arg {
                sgroup!(expr) if self.strip_brackets => {
                    let rendered = self.render_expression(expr);
                    self.render_equals(rendered.iter, simple.op, arg, simple.second())
                }
                arg => {
                    let rendered = self.render_simple(arg);
                    self.render_equals(rendered.iter, simple.op, arg, simple.second())
                }
            },
            // generic
            (op, first, second) => self.render_bgeneric(op, first, second),
        }
    }

    #[inline]
    fn render_font<'a>(
        &self,
        font: fn(char) -> char,
        arg: &Simple<'a>,
    ) -> RenderChars<SimpleUnaryIter<'a>> {
        match arg {
            sgroup!(expr) if self.strip_brackets => self
                .render_expression(expr)
                .map(|iter| SimpleUnaryIter::StrippedFont(Box::new(iter.map(font)))),
            arg => self
                .render_simple(arg)
                .map(|iter| SimpleUnaryIter::Font(Box::new(iter.map(font)))),
        }
    }

    #[inline]
    fn render_sfunc<'a>(
        &self,
        open: &'a str,
        arg: &Simple<'a>,
        close: &'a str,
    ) -> RenderChars<SimpleUnaryIter<'a>> {
        match arg {
            sgroup!(expr) if self.strip_brackets => RenderChars::from(open)
                .chain(self.render_expression(expr).map(Box::new))
                .chain(RenderChars::from(close))
                .map(SimpleUnaryIter::StrippedWrapped),
            arg => RenderChars::from(open)
                .chain(self.render_simple(arg).map(Box::new))
                .chain(RenderChars::from(close))
                .map(SimpleUnaryIter::Wrapped),
        }
    }

    #[inline]
    fn render_mod<'a>(&self, chr: char, arg: &Simple<'a>) -> RenderChars<SimpleUnaryIter<'a>> {
        match arg {
            sgroup!(expr) if self.strip_brackets => self
                .render_expression(expr)
                .map(|iter| SimpleUnaryIter::StrippedModed(Box::new(Modified::new(iter, chr)))),
            arg => self
                .render_simple(arg)
                .map(|iter| SimpleUnaryIter::Moded(Box::new(Modified::new(iter, chr)))),
        }
    }

    #[inline]
    fn render_char_mod<'a>(
        &self,
        op: &'a str,
        chr: char,
        arg: &Simple<'a>,
    ) -> RenderChars<SimpleUnaryIter<'a>> {
        match arg {
            sgroup!(expr) if self.strip_brackets => {
                let rendered = self.render_expression(expr);
                if rendered.len == 1 {
                    rendered
                        .map(Box::new)
                        .chain(RenderChars::from(chr))
                        .map(SimpleUnaryIter::StrippedSingle)
                } else {
                    self.render_ugeneric(op, arg)
                }
            }
            arg => {
                let rendered = self.render_simple(arg);
                if rendered.len == 1 {
                    rendered
                        .map(Box::new)
                        .chain(RenderChars::from(chr))
                        .map(SimpleUnaryIter::Single)
                } else {
                    self.render_ugeneric(op, arg)
                }
            }
        }
    }

    #[inline]
    fn render_ugeneric<'a>(
        &self,
        op: &'a str,
        arg: &Simple<'a>,
    ) -> RenderChars<SimpleUnaryIter<'a>> {
        RenderChars::from(op)
            .chain(RenderChars::from(' '))
            .chain(self.render_simple(arg).map(Box::new))
            .map(SimpleUnaryIter::Generic)
    }

    #[allow(clippy::too_many_lines)]
    fn render_simpleunary<'a>(&self, simple: &SimpleUnary<'a>) -> RenderChars<SimpleUnaryIter<'a>> {
        match (simple.op, simple.arg()) {
            // sqrt
            ("sqrt", arg) => RenderChars::from('√')
                .chain(self.render_simple(arg).map(Box::new))
                .map(SimpleUnaryIter::Simple),
            // fonts
            ("bb" | "mathbf", arg) => self.render_font(bold_map, arg),
            ("bbb" | "mathbb", arg) => self.render_font(double_map, arg),
            ("cc" | "mathcal", arg) => self.render_font(cal_map, arg),
            ("tt" | "mathtt", arg) => self.render_font(mono_map, arg),
            ("fr" | "mathfrak", arg) => self.render_font(frak_map, arg),
            ("sf" | "mathsf", arg) => self.render_font(sans_map, arg),
            ("it" | "mathit", arg) => self.render_font(italic_map, arg),
            // functions
            ("abs" | "Abs", arg) => self.render_sfunc("|", arg, "|"),
            ("ceil", arg) => self.render_sfunc("⌈", arg, "⌉"),
            ("floor", arg) => self.render_sfunc("⌊", arg, "⌋"),
            ("norm", arg) => self.render_sfunc("||", arg, "||"),
            ("text", arg) => self.render_sfunc("", arg, ""),
            // modifiers
            ("overline", arg) => self.render_mod('\u{0305}', arg),
            ("underline" | "ul", arg) => self.render_mod('\u{0332}', arg),
            // single character modifiers
            (o @ "hat", arg) => self.render_char_mod(o, '\u{0302}', arg),
            (o @ "tilde", arg) => self.render_char_mod(o, '\u{0303}', arg),
            (o @ "bar", arg) => self.render_char_mod(o, '\u{0304}', arg),
            (o @ "dot", arg) => self.render_char_mod(o, '\u{0307}', arg),
            (o @ "ddot", arg) => self.render_char_mod(o, '\u{0308}', arg),
            (o @ ("overarc" | "overparen"), arg) => self.render_char_mod(o, '\u{0311}', arg),
            // generic
            (op, arg) => self.render_ugeneric(op, arg),
        }
    }

    fn render_matrix<'a>(&self, matrix: &Matrix<'a>) -> RenderChars<MatIter<'a>> {
        let num_cols = matrix.num_cols();
        let num_rows = matrix.num_rows();
        let left_rend = RenderChars::from(left_bracket_str(matrix.left_bracket));
        let right_rend = RenderChars::from(right_bracket_str(matrix.right_bracket));

        let mut rendered = Vec::with_capacity(matrix.num_rows());
        let mut len =
            (num_rows + 1) * (left_rend.len + right_rend.len) + (num_rows - 1) * (num_cols - 1);
        for row in matrix.rows() {
            let mut rends = Vec::with_capacity(row.len());
            for expr in row {
                let rend = self.render_expression(expr);
                len += rend.len;
                rends.push(rend.iter);
            }
            rendered.push(
                left_rend
                    .iter
                    .clone()
                    .chain(Interleave::new(rends, ','))
                    .chain(right_rend.iter.clone()),
            );
        }
        RenderChars {
            iter: left_rend
                .iter
                .chain(Box::new(Interleave::new(rendered, ',')))
                .chain(right_rend.iter),
            len,
            sub: false,
            sup: false,
        }
    }

    fn render_group<'a>(&self, group: &Group<'a>) -> RenderChars<GroupIter<'a>> {
        RenderChars::from(left_bracket_str(group.left_bracket))
            .chain(self.render_expression(&group.expr).map(Box::new))
            .chain(RenderChars::from(right_bracket_str(group.right_bracket)))
    }

    fn render_simple<'a>(&self, simple: &Simple<'a>) -> RenderChars<SimpleIter<'a>> {
        match simple {
            Simple::Missing => RenderChars::from("").map(SimpleIter::Chars),
            &Simple::Number(num) => RenderChars::from(num).map(SimpleIter::Chars),
            &Simple::Text(text) => RenderChars::from(text).map(SimpleIter::Chars),
            &Simple::Ident(ident) => RenderChars::from(ident).map(SimpleIter::Chars),
            &Simple::Symbol(symbol) => {
                RenderChars::from(symbol_str(symbol, self.skin_tone)).map(SimpleIter::Chars)
            }
            Simple::Func(func) => self.render_simplefunc(func).map(SimpleIter::Func),
            Simple::Unary(unary) => self.render_simpleunary(unary).map(SimpleIter::Unary),
            Simple::Binary(binary) => self.render_simplebinary(binary).map(SimpleIter::Binary),
            Simple::Group(group) => self.render_group(group).map(SimpleIter::Group),
            Simple::Matrix(matrix) => self.render_matrix(matrix).map(SimpleIter::Matrix),
        }
    }

    fn render_script<'a>(&self, script: &Script<'a>) -> RenderChars<ScriptIter<'a>> {
        match script {
            Script::None => RenderChars::from("").map(ScriptIter::Empty),
            Script::Sub(sub) => {
                let rendered = self.render_simple(sub);
                if rendered.sub {
                    rendered
                        .map(|iter| ScriptIter::Mapped(iter.map(|c| subscript_char(c).unwrap())))
                } else {
                    RenderChars::from('_')
                        .chain(rendered)
                        .map(ScriptIter::Untouched)
                }
            }
            Script::Super(sup) => {
                let rendered = self.render_simple(sup);
                if rendered.sup {
                    rendered
                        .map(|iter| ScriptIter::Mapped(iter.map(|c| superscript_char(c).unwrap())))
                } else {
                    RenderChars::from('^')
                        .chain(rendered)
                        .map(ScriptIter::Untouched)
                }
            }
            Script::Subsuper(sub, supersc) => {
                let rend_sub = self.render_simple(sub);
                let rend_super = self.render_simple(supersc);
                if rend_sub.sub && rend_super.sup {
                    rend_sub
                        .map(|iter| SimpMappedIter(iter.map(|c| subscript_char(c).unwrap())))
                        .chain(
                            rend_super.map(|iter| {
                                SimpMappedIter(iter.map(|c| superscript_char(c).unwrap()))
                            }),
                        )
                        .map(ScriptIter::SubsupMapped)
                } else {
                    RenderChars::from('_')
                        .chain(rend_sub)
                        .chain(RenderChars::from('^'))
                        .chain(rend_super)
                        .map(ScriptIter::SubsupUntouched)
                }
            }
        }
    }

    fn render_simplescript<'a>(
        &self,
        simple: &SimpleScript<'a>,
    ) -> RenderChars<SimpleScriptIter<'a>> {
        self.render_simple(&simple.simple)
            .chain(self.render_script(&simple.script))
    }

    fn render_func<'a>(&self, func: &Func<'a>) -> RenderChars<FuncIter<'a>> {
        RenderChars::from(func.func)
            .chain(self.render_script(&func.script))
            .chain(RenderChars::from(' '))
            .chain(self.render_scriptfunc(func.arg()).map(Box::new))
    }

    fn render_scriptfunc<'a>(&self, func: &ScriptFunc<'a>) -> RenderChars<ScriptFuncIter<'a>> {
        match func {
            ScriptFunc::Simple(simple) => {
                self.render_simplescript(simple).map(ScriptFuncIter::Simple)
            }
            ScriptFunc::Func(func) => self.render_func(func).map(ScriptFuncIter::Func),
        }
    }

    #[inline]
    fn render_sone<'a>(
        &self,
        num: &Simple<'a>,
        den: &Simple<'a>,
    ) -> RenderChars<SimpleFracIter<'a>> {
        match den {
            sgroup!(expr) if self.strip_brackets => {
                let rend_den = self.render_expression(expr);
                if rend_den.sub {
                    RenderChars::from('⅟')
                        .chain(rend_den.map(|iter| {
                            Box::new(ExprMappedIter(iter.map(|c| subscript_char(c).unwrap())))
                        }))
                        .map(SimpleFracIter::StrippedVulgOne)
                } else {
                    gsfrac(self.render_simple(num), self.render_simple(den))
                }
            }
            den => {
                let rend_den = self.render_simple(den);
                if rend_den.sub {
                    RenderChars::from('⅟')
                        .chain(
                            rend_den.map(|iter| {
                                SimpMappedIter(iter.map(|c| subscript_char(c).unwrap()))
                            }),
                        )
                        .map(SimpleFracIter::VulgOne)
                } else {
                    gsfrac(self.render_simple(num), rend_den)
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn render_simplefrac<'a>(
        &self,
        numer: &Simple<'a>,
        denom: &Simple<'a>,
    ) -> RenderChars<SimpleFracIter<'a>> {
        let vsf = self.vulgar_fracs && self.script_fracs;
        let vs = self.vulgar_fracs && self.strip_brackets;
        match (numer, denom) {
            // fracs
            (num!("0"), num!("3")) if self.vulgar_fracs => vulg('↉'),
            (num!("1"), num!("10")) if self.vulgar_fracs => vulg('⅒'),
            (num!("1"), num!("9")) if self.vulgar_fracs => vulg('⅑'),
            (num!("1"), num!("8")) if self.vulgar_fracs => vulg('⅛'),
            (num!("1"), num!("7")) if self.vulgar_fracs => vulg('⅐'),
            (num!("1"), num!("6")) if self.vulgar_fracs => vulg('⅙'),
            (num!("1"), num!("5")) if self.vulgar_fracs => vulg('⅕'),
            (num!("1"), num!("4")) if self.vulgar_fracs => vulg('¼'),
            (num!("1"), num!("3")) if self.vulgar_fracs => vulg('⅓'),
            (num!("1"), num!("2")) if self.vulgar_fracs => vulg('½'),
            (num!("2"), num!("5")) if self.vulgar_fracs => vulg('⅖'),
            (num!("2"), num!("3")) if self.vulgar_fracs => vulg('⅔'),
            (num!("3"), num!("8")) if self.vulgar_fracs => vulg('⅜'),
            (num!("3"), num!("5")) if self.vulgar_fracs => vulg('⅗'),
            (num!("3"), num!("4")) if self.vulgar_fracs => vulg('¾'),
            (num!("4"), num!("5")) if self.vulgar_fracs => vulg('⅘'),
            (num!("5"), num!("8")) if self.vulgar_fracs => vulg('⅝'),
            (num!("5"), num!("6")) if self.vulgar_fracs => vulg('⅚'),
            (num!("7"), num!("8")) if self.vulgar_fracs => vulg('⅞'),
            (sgroup!(num), num!("3")) if xnum!(num, "0") && vs => vulg('↉'),
            (sgroup!(num), num!("10")) if xnum!(num, "1") && vs => vulg('⅒'),
            (sgroup!(num), num!("9")) if xnum!(num, "1") && vs => vulg('⅑'),
            (sgroup!(num), num!("8")) if xnum!(num, "1") && vs => vulg('⅛'),
            (sgroup!(num), num!("7")) if xnum!(num, "1") && vs => vulg('⅐'),
            (sgroup!(num), num!("6")) if xnum!(num, "1") && vs => vulg('⅙'),
            (sgroup!(num), num!("5")) if xnum!(num, "1") && vs => vulg('⅕'),
            (sgroup!(num), num!("4")) if xnum!(num, "1") && vs => vulg('¼'),
            (sgroup!(num), num!("3")) if xnum!(num, "1") && vs => vulg('⅓'),
            (sgroup!(num), num!("2")) if xnum!(num, "1") && vs => vulg('½'),
            (sgroup!(num), num!("5")) if xnum!(num, "2") && vs => vulg('⅖'),
            (sgroup!(num), num!("3")) if xnum!(num, "2") && vs => vulg('⅔'),
            (sgroup!(num), num!("8")) if xnum!(num, "3") && vs => vulg('⅜'),
            (sgroup!(num), num!("5")) if xnum!(num, "3") && vs => vulg('⅗'),
            (sgroup!(num), num!("4")) if xnum!(num, "3") && vs => vulg('¾'),
            (sgroup!(num), num!("5")) if xnum!(num, "4") && vs => vulg('⅘'),
            (sgroup!(num), num!("8")) if xnum!(num, "5") && vs => vulg('⅝'),
            (sgroup!(num), num!("6")) if xnum!(num, "5") && vs => vulg('⅚'),
            (sgroup!(num), num!("8")) if xnum!(num, "7") && vs => vulg('⅞'),
            (num!("0"), sgroup!(den)) if xnum!(den, "3") && vs => vulg('↉'),
            (num!("1"), sgroup!(den)) if xnum!(den, "10") && vs => vulg('⅒'),
            (num!("1"), sgroup!(den)) if xnum!(den, "9") && vs => vulg('⅑'),
            (num!("1"), sgroup!(den)) if xnum!(den, "8") && vs => vulg('⅛'),
            (num!("1"), sgroup!(den)) if xnum!(den, "7") && vs => vulg('⅐'),
            (num!("1"), sgroup!(den)) if xnum!(den, "6") && vs => vulg('⅙'),
            (num!("1"), sgroup!(den)) if xnum!(den, "5") && vs => vulg('⅕'),
            (num!("1"), sgroup!(den)) if xnum!(den, "4") && vs => vulg('¼'),
            (num!("1"), sgroup!(den)) if xnum!(den, "3") && vs => vulg('⅓'),
            (num!("1"), sgroup!(den)) if xnum!(den, "2") && vs => vulg('½'),
            (num!("2"), sgroup!(den)) if xnum!(den, "5") && vs => vulg('⅖'),
            (num!("2"), sgroup!(den)) if xnum!(den, "3") && vs => vulg('⅔'),
            (num!("3"), sgroup!(den)) if xnum!(den, "8") && vs => vulg('⅜'),
            (num!("3"), sgroup!(den)) if xnum!(den, "5") && vs => vulg('⅗'),
            (num!("3"), sgroup!(den)) if xnum!(den, "4") && vs => vulg('¾'),
            (num!("4"), sgroup!(den)) if xnum!(den, "5") && vs => vulg('⅘'),
            (num!("5"), sgroup!(den)) if xnum!(den, "8") && vs => vulg('⅝'),
            (num!("5"), sgroup!(den)) if xnum!(den, "6") && vs => vulg('⅚'),
            (num!("7"), sgroup!(den)) if xnum!(den, "8") && vs => vulg('⅞'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "0") && xnum!(den, "3") && vs => vulg('↉'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "1") && xnum!(den, "10") && vs => vulg('⅒'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "1") && xnum!(den, "9") && vs => vulg('⅑'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "1") && xnum!(den, "8") && vs => vulg('⅛'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "1") && xnum!(den, "7") && vs => vulg('⅐'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "1") && xnum!(den, "6") && vs => vulg('⅙'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "1") && xnum!(den, "5") && vs => vulg('⅕'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "1") && xnum!(den, "4") && vs => vulg('¼'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "1") && xnum!(den, "3") && vs => vulg('⅓'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "1") && xnum!(den, "2") && vs => vulg('½'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "2") && xnum!(den, "5") && vs => vulg('⅖'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "2") && xnum!(den, "3") && vs => vulg('⅔'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "3") && xnum!(den, "8") && vs => vulg('⅜'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "3") && xnum!(den, "5") && vs => vulg('⅗'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "3") && xnum!(den, "4") && vs => vulg('¾'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "4") && xnum!(den, "5") && vs => vulg('⅘'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "5") && xnum!(den, "8") && vs => vulg('⅝'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "5") && xnum!(den, "6") && vs => vulg('⅚'),
            (sgroup!(num), sgroup!(den)) if xnum!(num, "7") && xnum!(den, "8") && vs => vulg('⅞'),
            // frac like
            (iden!("a"), iden!("c")) if self.vulgar_fracs => vulg('℀'),
            (iden!("a"), iden!("s")) if self.vulgar_fracs => vulg('℁'),
            (iden!("A"), iden!("S")) if self.vulgar_fracs => vulg('⅍'),
            (iden!("c"), iden!("o")) if self.vulgar_fracs => vulg('℅'),
            (iden!("c"), iden!("u")) if self.vulgar_fracs => vulg('℆'),
            (sgroup!(num), iden!("c")) if xiden!(num, "a") && vs => vulg('℀'),
            (sgroup!(num), iden!("s")) if xiden!(num, "a") && vs => vulg('℁'),
            (sgroup!(num), iden!("S")) if xiden!(num, "A") && vs => vulg('⅍'),
            (sgroup!(num), iden!("o")) if xiden!(num, "c") && vs => vulg('℅'),
            (sgroup!(num), iden!("u")) if xiden!(num, "c") && vs => vulg('℆'),
            (iden!("a"), sgroup!(den)) if xiden!(den, "c") && vs => vulg('℀'),
            (iden!("a"), sgroup!(den)) if xiden!(den, "s") && vs => vulg('℁'),
            (iden!("A"), sgroup!(den)) if xiden!(den, "S") && vs => vulg('⅍'),
            (iden!("c"), sgroup!(den)) if xiden!(den, "o") && vs => vulg('℅'),
            (iden!("c"), sgroup!(den)) if xiden!(den, "u") && vs => vulg('℆'),
            (sgroup!(num), sgroup!(den)) if xiden!(num, "a") && xiden!(den, "c") && vs => vulg('℀'),
            (sgroup!(num), sgroup!(den)) if xiden!(num, "a") && xiden!(den, "s") && vs => vulg('℁'),
            (sgroup!(num), sgroup!(den)) if xiden!(num, "A") && xiden!(den, "S") && vs => vulg('⅍'),
            (sgroup!(num), sgroup!(den)) if xiden!(num, "c") && xiden!(den, "o") && vs => vulg('℅'),
            (sgroup!(num), sgroup!(den)) if xiden!(num, "c") && xiden!(den, "u") && vs => vulg('℆'),
            // one fracs
            (num!("1"), den) if vsf => self.render_sone(numer, den),
            (sgroup!(num), den) if vsf && self.strip_brackets && xnum!(num, "1") => {
                self.render_sone(numer, den)
            }
            // normal
            (sgroup!(num), sgroup!(den)) if self.strip_brackets && self.script_fracs => {
                let rend_num = self.render_expression(num);
                let rend_den = self.render_expression(den);
                if rend_num.sup && rend_den.sub {
                    rend_num
                        .map(|iter| ExprMappedIter(iter.map(|c| superscript_char(c).unwrap())))
                        .chain(RenderChars::from('⁄'))
                        .chain(
                            rend_den.map(|iter| {
                                ExprMappedIter(iter.map(|c| subscript_char(c).unwrap()))
                            }),
                        )
                        .map(|iter| SimpleFracIter::StrippedScript(Box::new(iter)))
                } else {
                    gsfrac(self.render_simple(numer), self.render_simple(denom))
                }
            }
            (num, sgroup!(den)) if self.strip_brackets && self.script_fracs => {
                let rend_num = self.render_simple(num);
                let rend_den = self.render_expression(den);
                if rend_num.sup && rend_den.sub {
                    rend_num
                        .map(|iter| SimpMappedIter(iter.map(|c| superscript_char(c).unwrap())))
                        .chain(RenderChars::from('⁄'))
                        .chain(rend_den.map(|iter| {
                            Box::new(ExprMappedIter(iter.map(|c| subscript_char(c).unwrap())))
                        }))
                        .map(SimpleFracIter::DenomStrippedScript)
                } else {
                    gsfrac(rend_num, self.render_simple(denom))
                }
            }
            (sgroup!(num), den) if self.strip_brackets && self.script_fracs => {
                let rend_num = self.render_expression(num);
                let rend_den = self.render_simple(den);
                if rend_num.sup && rend_den.sub {
                    rend_num
                        .map(|iter| {
                            Box::new(ExprMappedIter(iter.map(|c| superscript_char(c).unwrap())))
                        })
                        .chain(RenderChars::from('⁄'))
                        .chain(
                            rend_den.map(|iter| {
                                SimpMappedIter(iter.map(|c| subscript_char(c).unwrap()))
                            }),
                        )
                        .map(SimpleFracIter::NumerStrippedScript)
                } else {
                    gsfrac(self.render_simple(numer), rend_den)
                }
            }
            (num, den) => {
                let rend_num = self.render_simple(num);
                let rend_den = self.render_simple(den);
                if self.script_fracs && rend_num.sup && rend_den.sub {
                    rend_num
                        .map(|iter| SimpMappedIter(iter.map(|c| superscript_char(c).unwrap())))
                        .chain(RenderChars::from('⁄'))
                        .chain(
                            rend_den.map(|iter| {
                                SimpMappedIter(iter.map(|c| subscript_char(c).unwrap()))
                            }),
                        )
                        .map(SimpleFracIter::Script)
                } else {
                    gsfrac(rend_num, rend_den)
                }
            }
        }
    }

    #[inline]
    fn render_fone<'a>(&self, den: &ScriptFunc<'a>) -> RenderChars<FracIter<'a>> {
        let rend_den = self.render_scriptfunc(den);
        if rend_den.sub {
            RenderChars::from('⅟')
                .chain(
                    rend_den.map(|iter| FuncMappedIter(iter.map(|c| subscript_char(c).unwrap()))),
                )
                .map(FracIter::VulgOne)
        } else {
            RenderChars::from("1/").chain(rend_den).map(FracIter::One)
        }
    }

    #[allow(clippy::too_many_lines)]
    fn render_frac<'a>(&self, frac: &Frac<'a>) -> RenderChars<FracIter<'a>> {
        let sv = self.script_fracs && self.vulgar_fracs;
        match (&frac.numer, &frac.denom) {
            // simple frac
            (script_func!(num), script_func!(den)) => {
                self.render_simplefrac(num, den).map(FracIter::Simple)
            }
            // one vulgar
            (script_func!(num!("1")), den) if sv => self.render_fone(den),
            (script_func!(sgroup!(num)), den) if sv && self.strip_brackets && xnum!(num, "1") => {
                self.render_fone(den)
            }
            // normal fractions
            (script_func!(sgroup!(num)), den) if self.strip_brackets && self.script_fracs => {
                let rend_num = self.render_expression(num);
                let rend_den = self.render_scriptfunc(den);
                if rend_num.sup && rend_den.sub {
                    rend_num
                        .map(|iter| {
                            Box::new(ExprMappedIter(iter.map(|c| superscript_char(c).unwrap())))
                        })
                        .chain(RenderChars::from('⁄'))
                        .chain(
                            rend_den.map(|iter| {
                                FuncMappedIter(iter.map(|c| subscript_char(c).unwrap()))
                            }),
                        )
                        .map(FracIter::NumerStrippedScript)
                } else {
                    gfrac(self.render_scriptfunc(&frac.numer), rend_den)
                }
            }
            (num, script_func!(sgroup!(den))) if self.strip_brackets && self.script_fracs => {
                let rend_num = self.render_scriptfunc(num);
                let rend_den = self.render_expression(den);
                if rend_num.sup && rend_den.sub {
                    rend_num
                        .map(|iter| FuncMappedIter(iter.map(|c| superscript_char(c).unwrap())))
                        .chain(RenderChars::from('⁄'))
                        .chain(rend_den.map(|iter| {
                            Box::new(ExprMappedIter(iter.map(|c| subscript_char(c).unwrap())))
                        }))
                        .map(FracIter::DenomStrippedScript)
                } else {
                    gfrac(rend_num, self.render_scriptfunc(&frac.denom))
                }
            }
            (num, den) => {
                let rend_num = self.render_scriptfunc(num);
                let rend_den = self.render_scriptfunc(den);
                if self.script_fracs && rend_num.sup && rend_den.sub {
                    rend_num
                        .map(|iter| FuncMappedIter(iter.map(|c| superscript_char(c).unwrap())))
                        .chain(RenderChars::from('⁄'))
                        .chain(
                            rend_den.map(|iter| {
                                FuncMappedIter(iter.map(|c| subscript_char(c).unwrap()))
                            }),
                        )
                        .map(FracIter::Script)
                } else {
                    gfrac(rend_num, rend_den)
                }
            }
        }
    }

    fn render_intermediate<'a>(
        &self,
        inter: &Intermediate<'a>,
    ) -> RenderChars<IntermediateIter<'a>> {
        match inter {
            Intermediate::ScriptFunc(sf) => {
                self.render_scriptfunc(sf).map(IntermediateIter::ScriptFunc)
            }
            Intermediate::Frac(frac) => self.render_frac(frac).map(IntermediateIter::Frac),
        }
    }

    fn render_expression<'a>(&self, expr: &Expression<'a>) -> RenderChars<ExpressionIter<'a>> {
        let inters: RenderChars<_> = expr
            .iter()
            .map(|inter| self.render_intermediate(inter))
            .collect();
        inters
    }

    /// Render an input string with the given options
    #[must_use]
    pub fn render<'a>(&self, inp: &'a str) -> RenderedUnicode<'a> {
        let parsed = parse_unicode(inp);
        let rendered = self.render_expression(&parsed);
        RenderedUnicode(rendered.iter)
    }
}

/// Rendered unicode
///
/// This can be formatted to get a string, [consumed into a `Write`][RenderedUnicode::into_write],
/// or iterated as `char`s.
#[derive(Debug, Clone)]
pub struct RenderedUnicode<'a>(ExpressionIter<'a>);

impl Iterator for RenderedUnicode<'_> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl FusedIterator for RenderedUnicode<'_> {}

impl RenderedUnicode<'_> {
    /// Write out, consuming self in the process
    ///
    /// This avoids the clone necessary when formatting.
    ///
    /// # Errors
    ///
    /// If there are any io errors writing.
    pub fn into_write<O: Write>(self, out: &mut O) -> io::Result<()> {
        for chr in self {
            write!(out, "{chr}")?;
        }
        Ok(())
    }
}

impl fmt::Display for RenderedUnicode<'_> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        for chr in self.clone() {
            write!(out, "{chr}")?;
        }
        Ok(())
    }
}

/// Parse asciimath using the conventions of this renderer
#[must_use]
pub fn parse_unicode(inp: &str) -> Expression {
    asciimath_parser::parse_tokens(Tokenizer::with_tokens(inp, &*TOKEN_MAP, true))
}

/// Convert an asciimath string into unicode and write it to the writer
///
/// # Errors
///
/// If one is thrown by the writer
pub fn write_unicode<O: Write>(inp: &str, out: &mut O) -> io::Result<()> {
    InlineRenderer::default().render(inp).into_write(out)
}

/// Convert an asciimath string into a unicode string
#[must_use]
pub fn convert_unicode(inp: &str) -> String {
    InlineRenderer::default().render(inp).collect()
}

#[cfg(test)]
mod tests {
    use super::{InlineRenderer, SkinTone};

    #[test]
    fn example() {
        let ex = "sum_(i=1)^n i^3=((n(n+1))/2)^2";
        let expected = "∑₍ᵢ₌₁₎ⁿi³=(ⁿ⁽ⁿ⁺¹⁾⁄₂)²";

        let res = super::convert_unicode(ex);
        assert_eq!(res, expected);

        let mut res = Vec::new();
        super::write_unicode(ex, &mut res).unwrap();
        assert_eq!(res, expected.as_bytes());

        let rend = InlineRenderer::default().render(ex);
        assert_eq!(format!("{rend}"), expected);

        let mut res = Vec::new();
        rend.into_write(&mut res).unwrap();
        assert_eq!(res, expected.as_bytes());
    }

    #[test]
    fn vulgar_fracs() {
        let opts = InlineRenderer {
            vulgar_fracs: true,
            ..Default::default()
        };
        let res: String = opts.render("1/2").collect();
        assert_eq!(res, "½");

        let res: String = opts.render("a / s").collect();
        assert_eq!(res, "℁");
    }

    #[test]
    fn stripped_vulgar_fracs() {
        let opts = InlineRenderer {
            vulgar_fracs: true,
            strip_brackets: true,
            ..Default::default()
        };
        let res: String = opts.render("(1)/2").collect();
        assert_eq!(res, "½");

        let res: String = opts.render("7/[8]").collect();
        assert_eq!(res, "⅞");

        let res: String = opts.render("{a} / (s)").collect();
        assert_eq!(res, "℁");
    }

    #[test]
    fn script_fracs() {
        let opts = InlineRenderer {
            script_fracs: true,
            strip_brackets: false,
            ..Default::default()
        };
        let res: String = opts.render("y / x").collect();
        assert_eq!(res, "ʸ⁄ₓ");

        let res: String = opts.render("(y) / x").collect();
        assert_eq!(res, "⁽ʸ⁾⁄ₓ");
    }

    #[test]
    fn stripped_script_fracs() {
        let opts = InlineRenderer {
            script_fracs: true,
            ..Default::default()
        };

        let res: String = opts.render("(y) / x").collect();
        assert_eq!(res, "ʸ⁄ₓ");

        let res: String = opts.render("y / [x]").collect();
        assert_eq!(res, "ʸ⁄ₓ");

        let res: String = opts.render("(y)/[x]").collect();
        assert_eq!(res, "ʸ⁄ₓ");
    }

    #[test]
    fn one_fracs() {
        let res = super::convert_unicode("1/x");
        assert_eq!(res, "⅟ₓ");

        let res = super::convert_unicode("1 / sinx");
        assert_eq!(res, "⅟ₛᵢₙ ₓ");

        let opts = InlineRenderer {
            script_fracs: false,
            vulgar_fracs: false,
            strip_brackets: false,
            ..Default::default()
        };
        let res: String = opts.render("1 / sinx").collect();
        assert_eq!(res, "1/sin x");
    }

    #[test]
    fn normal_fracs() {
        let opts = InlineRenderer {
            script_fracs: false,
            vulgar_fracs: false,
            strip_brackets: false,
            ..Default::default()
        };

        let res: String = opts.render("sinx / cosy").collect();
        assert_eq!(res, "sin x/cos y");
    }

    #[test]
    fn unary() {
        let res = super::convert_unicode("sqrt x");
        assert_eq!(res, "√x");

        let res = super::convert_unicode("vec x");
        assert_eq!(res, "vec x");

        let res = super::convert_unicode("bbb E");
        assert_eq!(res, "𝔼");

        let res = super::convert_unicode("bbb (E)");
        assert_eq!(res, "𝔼");

        let res = super::convert_unicode("dot x");
        assert_eq!(res, "ẋ");

        let res = super::convert_unicode("dot{x}");
        assert_eq!(res, "ẋ");

        let res = super::convert_unicode("norm x");
        assert_eq!(res, "||x||");

        let res = super::convert_unicode("sqrt overline x");
        assert_eq!(res, "√x̅");

        let res = super::convert_unicode("sqrt overline(x)");
        assert_eq!(res, "√x̅");
    }

    #[test]
    fn binary() {
        let res = super::convert_unicode("root 3 x");
        assert_eq!(res, "∛x");

        let res = super::convert_unicode("root {4} x");
        assert_eq!(res, "∜x");

        let res = super::convert_unicode("stackrel *** =");
        assert_eq!(res, "≛");

        let res = super::convert_unicode("overset a x");
        assert_eq!(res, "x\u{0363}");

        let res = super::convert_unicode("overset (e) {y}");
        assert_eq!(res, "y\u{0364}");

        let res = super::convert_unicode("oversetasinx");
        assert_eq!(res, "overset a sin x");
    }

    #[test]
    fn functions() {
        let res = super::convert_unicode("sin x/x");
        assert_eq!(res, "ˢⁱⁿ ˣ⁄ₓ");
    }

    #[test]
    fn script() {
        let res = super::convert_unicode("x^sin x");
        assert_eq!(res, "xˢⁱⁿ ˣ");

        let res = super::convert_unicode("x^vec(x)");
        assert_eq!(res, "xᵛᵉᶜ ⁽ˣ⁾");

        let res = super::convert_unicode("x_x^y");
        assert_eq!(res, "xₓʸ");

        let res = super::convert_unicode("x_y^sin x");
        assert_eq!(res, "x_y^sin x");

        let res = super::convert_unicode("x^sin rho");
        assert_eq!(res, "x^sin ρ");

        let res = super::convert_unicode("x_x");
        assert_eq!(res, "xₓ");

        let res = super::convert_unicode("x_y");
        assert_eq!(res, "x_y");
    }

    #[test]
    fn text() {
        let res = super::convert_unicode("\"text\"");
        assert_eq!(res, "text");
    }

    #[test]
    fn matrix() {
        let opts = InlineRenderer::default();

        let res: String = opts.render("[ [x, y], [a, b] ]").collect();
        assert_eq!(res, "[[x,y],[a,b]]");
    }

    #[test]
    fn skin_tone() {
        let opts = InlineRenderer {
            skin_tone: SkinTone::Default,
            ..Default::default()
        };
        let res: String = opts.render(":hand:").collect();
        assert_eq!(res, "✋");

        let opts = InlineRenderer {
            skin_tone: SkinTone::Dark,
            ..Default::default()
        };
        let res: String = opts.render(":hand:").collect();
        assert_eq!(res, "✋🏿");
    }
}
