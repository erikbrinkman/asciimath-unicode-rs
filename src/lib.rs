//! A module for converting asciimath to unicode
//!
//! To convert asciimath quickly, you can use [`parse_unicode`] to get an [`Asciimath`] value that
//! implements [`Display`]. If you want more control, see the options exposed through [`Conf`]
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

mod tokens;

use asciimath_parser::Tokenizer;
use asciimath_parser::tree::{
    Expression, Frac, Func, Group, Intermediate, Matrix, Script, ScriptFunc, Simple, SimpleBinary,
    SimpleFunc, SimpleScript, SimpleUnary,
};
pub use emojis::SkinTone;
use std::fmt;
use std::fmt::Write;
use tokens::{
    TOKEN_MAP, bold_map, cal_map, double_map, frak_map, italic_map, left_bracket_str, mono_map,
    right_bracket_str, sans_map, subscript_char, superscript_char, symbol_str,
};
use unicode_normalization::char::compose;

#[derive(Debug)]
struct Sink;

impl fmt::Write for Sink {
    fn write_str(&mut self, _: &str) -> fmt::Result {
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct MapperConf {
    font: Option<fn(char) -> char>,
    sub_sup: Option<fn(char) -> Option<char>>,
    modifier: Option<char>,
}

impl MapperConf {
    /// This method allows checking if we can apply `sub_sup` without borrowing the inner writer
    fn with_sub_sup(&self, sub_sup: fn(char) -> Option<char>) -> Option<MapperConf> {
        if self.sub_sup.is_none() {
            Some(MapperConf {
                font: self.font,
                sub_sup: Some(sub_sup),
                modifier: self.modifier,
            })
        } else {
            None
        }
    }

    fn with_sub(&self) -> Option<MapperConf> {
        self.with_sub_sup(subscript_char)
    }

    fn with_sup(&self) -> Option<MapperConf> {
        self.with_sub_sup(superscript_char)
    }

    fn wrap<S: Write>(self, other: &mut S) -> Mapper<'_, S> {
        Mapper {
            inner: other,
            conf: self,
        }
    }
}

#[derive(Debug)]
struct Mapper<'a, W: ?Sized> {
    inner: &'a mut W,
    conf: MapperConf,
}

impl<'a, W: fmt::Write + ?Sized> Mapper<'a, W> {
    fn new(inner: &'a mut W) -> Self {
        Mapper {
            inner,
            conf: MapperConf::default(),
        }
    }

    fn with_font(&mut self, f: fn(char) -> char) -> Mapper<'_, W> {
        Mapper {
            inner: &mut *self.inner,
            conf: MapperConf {
                font: Some(f),
                sub_sup: self.conf.sub_sup,
                modifier: self.conf.modifier,
            },
        }
    }

    fn with_modifier(&mut self, c: char) -> Mapper<'_, W> {
        Mapper {
            inner: &mut *self.inner,
            conf: MapperConf {
                font: self.conf.font,
                sub_sup: self.conf.sub_sup,
                modifier: Some(c),
            },
        }
    }

    fn onto<'b, S: Write>(&self, other: &'b mut S) -> Mapper<'b, S> {
        Mapper {
            inner: other,
            conf: self.conf,
        }
    }
}

impl<W: fmt::Write + ?Sized> fmt::Write for Mapper<'_, W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.conf.font.is_none() && self.conf.sub_sup.is_none() && self.conf.modifier.is_none() {
            self.inner.write_str(s)
        } else {
            for mut c in s.chars() {
                if let Some(script) = self.conf.sub_sup {
                    c = script(c).ok_or(fmt::Error)?;
                }
                if let Some(font) = self.conf.font {
                    c = font(c);
                }
                self.inner.write_char(c)?;
                if let Some(modifier) = self.conf.modifier {
                    self.inner.write_char(modifier)?;
                }
            }
            Ok(())
        }
    }
}

#[derive(Debug, Default)]
struct SingleChar(Option<char>);

impl fmt::Write for SingleChar {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for char in s.chars() {
            if self.0.is_none() {
                self.0 = Some(char);
            } else {
                return Err(fmt::Error);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct SmallBuf {
    buf: [u8; 4],
    len: usize,
}

impl SmallBuf {
    fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.buf[..self.len]).ok()
    }
}

impl fmt::Write for SmallBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        if self.len + bytes.len() > 4 {
            Err(fmt::Error)
        } else {
            self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
            self.len += bytes.len();
            Ok(())
        }
    }
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

fn only<T>(mut iter: impl Iterator<Item = T>) -> Option<T> {
    let first = iter.next();
    if iter.next().is_none() { first } else { None }
}

fn vulgar_frac_char(num: &str, den: &str) -> Option<char> {
    match (num, den) {
        ("0", "3") => Some('↉'),
        ("1", "10") => Some('⅒'),
        ("1", "9") => Some('⅑'),
        ("1", "8") => Some('⅛'),
        ("1", "7") => Some('⅐'),
        ("1", "6") => Some('⅙'),
        ("1", "5") => Some('⅕'),
        ("1", "4") => Some('¼'),
        ("1", "3") => Some('⅓'),
        ("1", "2") => Some('½'),
        ("2", "5") => Some('⅖'),
        ("2", "3") => Some('⅔'),
        ("3", "8") => Some('⅜'),
        ("3", "5") => Some('⅗'),
        ("3", "4") => Some('¾'),
        ("4", "5") => Some('⅘'),
        ("5", "8") => Some('⅝'),
        ("5", "6") => Some('⅚'),
        ("7", "8") => Some('⅞'),
        ("a", "c") => Some('℀'),
        ("a", "s") => Some('℁'),
        ("A", "S") => Some('⅍'),
        ("c", "o") => Some('℅'),
        ("c", "u") => Some('℆'),
        _ => None,
    }
}

fn extract_single_char(expr: &Expression<'_>) -> Option<char> {
    if let [
        Intermediate::ScriptFunc(ScriptFunc::Simple(SimpleScript {
            simple,
            script: Script::None,
        })),
    ] = &**expr
    {
        match simple {
            &Simple::Ident(s) | &Simple::Number(s) => only(s.chars()),
            _ => None,
        }
    } else {
        None
    }
}

fn extract_simple_str<'a>(simple: &Simple<'a>, strip: bool) -> Option<&'a str> {
    match simple {
        &Simple::Number(n) => Some(n),
        &Simple::Ident(i) => Some(i),
        Simple::Group(g) if strip => {
            if let [
                Intermediate::ScriptFunc(ScriptFunc::Simple(SimpleScript {
                    simple,
                    script: Script::None,
                })),
            ] = &*g.expr
            {
                extract_simple_str(simple, false)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn extract_vulgar_frac<'a>(numer: &Simple<'a>, denom: &Simple<'a>, strip: bool) -> Option<char> {
    let num = extract_simple_str(numer, strip)?;
    let den = extract_simple_str(denom, strip)?;
    vulgar_frac_char(num, den)
}

/// Configuration for inline unicode rendering of asciimath
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conf {
    /// If true, this will strip unnecessary parenthesis in some contexts
    pub strip_brackets: bool,
    /// If true, this will try to render fractions as vulgar fractions
    pub vulgar_fracs: bool,
    /// If true, this will try to render fractions using super- and sub-scripts
    pub script_fracs: bool,
    /// Default skin tone for emojis
    pub skin_tone: SkinTone,
}

impl Default for Conf {
    fn default() -> Self {
        Conf {
            strip_brackets: true,
            vulgar_fracs: true,
            script_fracs: true,
            skin_tone: SkinTone::Default,
        }
    }
}

impl Conf {
    fn simplefunc(self, simple: &SimpleFunc<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        out.write_str(simple.func)?;
        out.write_char(' ')?;
        self.simple(simple.arg(), out)
    }

    fn root(
        self,
        root_char: char,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_char(root_char)?;
        self.simple(arg, out)
    }

    fn cover(
        self,
        op: &str,
        first: &Simple<'_>,
        arg: &Simple<'_>,
        chr: char,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match arg {
            sgroup!(expr) if self.strip_brackets => {
                let mut single = SingleChar::default();
                if self.expression(expr, &mut out.onto(&mut single)).is_ok()
                    && let Some(res) = single.0
                {
                    out.write_char(res)?;
                    out.write_char(chr)
                } else {
                    self.bgeneric(op, first, arg, out)
                }
            }
            arg => {
                let mut single = SingleChar::default();
                if self.simple(arg, &mut out.onto(&mut single)).is_ok()
                    && let Some(res) = single.0
                {
                    out.write_char(res)?;
                    out.write_char(chr)
                } else {
                    self.bgeneric(op, first, arg, out)
                }
            }
        }
    }

    fn equals(
        self,
        op: &str,
        first: &Simple<'_>,
        second: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let mut buf = SmallBuf::default();
        let scan = match first {
            sgroup!(expr) if self.strip_brackets => self.expression(expr, &mut out.onto(&mut buf)),
            f => self.simple(f, &mut out.onto(&mut buf)),
        };
        if scan.is_ok()
            && let Some(s) = buf.as_str()
            && let Some(c) = match s {
                "∘" => Some('\u{2257}'),
                "⋆" => Some('\u{225b}'),
                "△" => Some('\u{225c}'),
                "def" => Some('\u{225d}'),
                "m" => Some('\u{225e}'),
                "?" => Some('\u{225f}'),
                _ => None,
            }
        {
            out.write_char(c)
        } else {
            self.bgeneric(op, first, second, out)
        }
    }

    fn bgeneric(
        self,
        op: &str,
        first: &Simple<'_>,
        second: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(op)?;
        out.write_char(' ')?;
        self.simple(first, out)?;
        out.write_char(' ')?;
        self.simple(second, out)
    }

    #[allow(clippy::too_many_lines)]
    fn simplebinary(
        self,
        simple: &SimpleBinary<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let sb = self.strip_brackets;
        match (simple.op, simple.first(), simple.second()) {
            // roots
            ("root", num!("2"), arg) => self.root('√', arg, out),
            ("root", num!("3"), arg) => self.root('∛', arg, out),
            ("root", num!("4"), arg) => self.root('∜', arg, out),
            ("root", sgroup!(expr), arg) if xnum!(expr, "2") => self.root('√', arg, out),
            ("root", sgroup!(expr), arg) if xnum!(expr, "3") => self.root('∛', arg, out),
            ("root", sgroup!(expr), arg) if xnum!(expr, "4") => self.root('∜', arg, out),
            // frac
            ("frac", numer, denom) => self.simplefrac(numer, denom, out),
            // stackrel / overset combining
            (o @ ("stackrel" | "overset"), f @ iden!("a"), a) => {
                self.cover(o, f, a, '\u{0363}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("e"), a) => {
                self.cover(o, f, a, '\u{0364}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("i"), a) => {
                self.cover(o, f, a, '\u{0365}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("o"), a) => {
                self.cover(o, f, a, '\u{0366}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("u"), a) => {
                self.cover(o, f, a, '\u{0367}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("c"), a) => {
                self.cover(o, f, a, '\u{0368}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("d"), a) => {
                self.cover(o, f, a, '\u{0369}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("h"), a) => {
                self.cover(o, f, a, '\u{036a}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("m"), a) => {
                self.cover(o, f, a, '\u{036b}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("r"), a) => {
                self.cover(o, f, a, '\u{036c}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("t"), a) => {
                self.cover(o, f, a, '\u{036d}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("v"), a) => {
                self.cover(o, f, a, '\u{036e}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("x"), a) => {
                self.cover(o, f, a, '\u{036f}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "a") => {
                self.cover(simple.op, simple.first(), arg, '\u{0363}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "e") => {
                self.cover(simple.op, simple.first(), arg, '\u{0364}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "i") => {
                self.cover(simple.op, simple.first(), arg, '\u{0365}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "o") => {
                self.cover(simple.op, simple.first(), arg, '\u{0366}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "u") => {
                self.cover(simple.op, simple.first(), arg, '\u{0367}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "c") => {
                self.cover(simple.op, simple.first(), arg, '\u{0368}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "d") => {
                self.cover(simple.op, simple.first(), arg, '\u{0369}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "h") => {
                self.cover(simple.op, simple.first(), arg, '\u{036a}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "m") => {
                self.cover(simple.op, simple.first(), arg, '\u{036b}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "r") => {
                self.cover(simple.op, simple.first(), arg, '\u{036c}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "t") => {
                self.cover(simple.op, simple.first(), arg, '\u{036d}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "v") => {
                self.cover(simple.op, simple.first(), arg, '\u{036e}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "x") => {
                self.cover(simple.op, simple.first(), arg, '\u{036f}', out)
            }
            // stackrel / overset equals
            ("stackrel" | "overset", arg, symb!("=")) => {
                self.equals(simple.op, arg, simple.second(), out)
            }
            // generic
            (op, first, second) => self.bgeneric(op, first, second, out),
        }
    }

    fn font(
        self,
        font: fn(char) -> char,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let mut w = out.with_font(font);
        match arg {
            sgroup!(expr) if self.strip_brackets => self.expression(expr, &mut w),
            arg => self.simple(arg, &mut w),
        }
    }

    fn sfunc(
        self,
        open: &str,
        arg: &Simple<'_>,
        close: &str,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(open)?;
        match arg {
            sgroup!(expr) if self.strip_brackets => self.expression(expr, out)?,
            arg => self.simple(arg, out)?,
        }
        out.write_str(close)
    }

    fn modi(self, chr: char, arg: &Simple<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        let mut w = out.with_modifier(chr);
        match arg {
            sgroup!(expr) if self.strip_brackets => self.expression(expr, &mut w),
            arg => self.simple(arg, &mut w),
        }
    }

    fn char_modi(
        self,
        op: &str,
        chr: char,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        // Try precomposition for single-char arguments (check AST, not rendered output)
        if let Some(base) = match arg {
            &Simple::Ident(s) | &Simple::Number(s) => only(s.chars()),
            sgroup!(expr) if self.strip_brackets => extract_single_char(expr),
            _ => None,
        } && let Some(precomposed) = compose(base, chr)
        {
            out.write_char(precomposed)
        } else {
            match arg {
                sgroup!(expr) if self.strip_brackets => {
                    let mut single = SingleChar::default();
                    if self.expression(expr, &mut out.onto(&mut single)).is_ok()
                        && let Some(res) = single.0
                    {
                        out.write_char(res)?;
                        out.write_char(chr)
                    } else {
                        self.ugeneric(op, arg, out)
                    }
                }
                arg => {
                    let mut single = SingleChar::default();
                    if self.simple(arg, &mut out.onto(&mut single)).is_ok()
                        && let Some(res) = single.0
                    {
                        out.write_char(res)?;
                        out.write_char(chr)
                    } else {
                        self.ugeneric(op, arg, out)
                    }
                }
            }
        }
    }

    fn ugeneric(
        self,
        op: &str,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(op)?;
        out.write_char(' ')?;
        self.simple(arg, out)
    }

    #[allow(clippy::too_many_lines)]
    fn simpleunary(
        self,
        simple: &SimpleUnary<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match (simple.op, simple.arg()) {
            // sqrt
            ("sqrt", arg) => {
                out.write_char('√')?;
                self.simple(arg, out)
            }
            // fonts
            ("bb" | "mathbf", arg) => self.font(bold_map, arg, out),
            ("bbb" | "mathbb", arg) => self.font(double_map, arg, out),
            ("cc" | "mathcal", arg) => self.font(cal_map, arg, out),
            ("tt" | "mathtt", arg) => self.font(mono_map, arg, out),
            ("fr" | "mathfrak", arg) => self.font(frak_map, arg, out),
            ("sf" | "mathsf", arg) => self.font(sans_map, arg, out),
            ("it" | "mathit", arg) => self.font(italic_map, arg, out),
            // functions
            ("abs" | "Abs", arg) => self.sfunc("|", arg, "|", out),
            ("ceil", arg) => self.sfunc("⌈", arg, "⌉", out),
            ("floor", arg) => self.sfunc("⌊", arg, "⌋", out),
            ("norm", arg) => self.sfunc("||", arg, "||", out),
            ("text" | "mbox", arg) => self.sfunc("", arg, "", out),
            // modifiers
            ("overline", arg) => self.modi('\u{0305}', arg, out),
            ("underline" | "ul", arg) => self.modi('\u{0332}', arg, out),
            ("cancel", arg) => self.modi('\u{0336}', arg, out),
            // single character modifiers
            (o @ "hat", arg) => self.char_modi(o, '\u{0302}', arg, out),
            (o @ "tilde", arg) => self.char_modi(o, '\u{0303}', arg, out),
            (o @ "bar", arg) => self.char_modi(o, '\u{0304}', arg, out),
            (o @ "dot", arg) => self.char_modi(o, '\u{0307}', arg, out),
            (o @ "ddot", arg) => self.char_modi(o, '\u{0308}', arg, out),
            (o @ ("overarc" | "overparen"), arg) => self.char_modi(o, '\u{0311}', arg, out),
            (o @ "vec", arg) => self.char_modi(o, '\u{20D7}', arg, out),
            // generic
            (op, arg) => self.ugeneric(op, arg, out),
        }
    }

    fn matrix(self, matrix: &Matrix<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        let left = left_bracket_str(matrix.left_bracket);
        let right = right_bracket_str(matrix.right_bracket);
        out.write_str(left)?;
        for (i, row) in matrix.rows().enumerate() {
            if i > 0 {
                out.write_char(',')?;
            }
            out.write_str(left)?;
            for (j, expr) in row.iter().enumerate() {
                if j > 0 {
                    out.write_char(',')?;
                }
                self.expression(expr, out)?;
            }
            out.write_str(right)?;
        }
        out.write_str(right)
    }

    fn group(self, group: &Group<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        out.write_str(left_bracket_str(group.left_bracket))?;
        self.expression(&group.expr, out)?;
        out.write_str(right_bracket_str(group.right_bracket))
    }

    fn simple(self, simple: &Simple<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        match simple {
            Simple::Missing => Ok(()),
            &Simple::Number(num) => out.write_str(num),
            &Simple::Text(text) => out.write_str(text),
            &Simple::Ident(ident) => out.write_str(ident),
            &Simple::Symbol(symbol) => out.write_str(symbol_str(symbol, self.skin_tone)),
            Simple::Func(func) => self.simplefunc(func, out),
            Simple::Unary(unary) => self.simpleunary(unary, out),
            Simple::Binary(binary) => self.simplebinary(binary, out),
            Simple::Group(group) => self.group(group, out),
            Simple::Matrix(matrix) => self.matrix(matrix, out),
        }
    }

    fn script(self, script: &Script<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        let mut sink = Sink;
        match script {
            Script::None => Ok(()),
            Script::Sub(sub) => {
                if let Some(sconf) = out.conf.with_sub()
                    && self.simple(sub, &mut sconf.wrap(&mut sink)).is_ok()
                {
                    self.simple(sub, &mut sconf.wrap(out.inner))
                } else {
                    out.write_char('_')?;
                    self.simple(sub, out)
                }
            }
            Script::Super(sup) => {
                if let Some(sconf) = out.conf.with_sup()
                    && self.simple(sup, &mut sconf.wrap(&mut sink)).is_ok()
                {
                    self.simple(sup, &mut sconf.wrap(out.inner))
                } else {
                    out.write_char('^')?;
                    self.simple(sup, out)
                }
            }
            Script::Subsuper(sub, sup) => {
                if let Some(sub_conf) = out.conf.with_sub()
                    && self.simple(sub, &mut sub_conf.wrap(&mut sink)).is_ok()
                    && let Some(sup_conf) = out.conf.with_sup()
                    && self.simple(sup, &mut sup_conf.wrap(&mut sink)).is_ok()
                {
                    self.simple(sub, &mut sub_conf.wrap(out.inner))?;
                    self.simple(sup, &mut sup_conf.wrap(out.inner))
                } else {
                    out.write_char('_')?;
                    self.simple(sub, out)?;
                    out.write_char('^')?;
                    self.simple(sup, out)
                }
            }
        }
    }

    fn simplescript(
        self,
        simple: &SimpleScript<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        self.simple(&simple.simple, out)?;
        self.script(&simple.script, out)
    }

    fn func(self, func: &Func<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        out.write_str(func.func)?;
        self.script(&func.script, out)?;
        out.write_char(' ')?;
        self.scriptfunc(func.arg(), out)
    }

    fn scriptfunc(self, func: &ScriptFunc<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        match func {
            ScriptFunc::Simple(simple) => self.simplescript(simple, out),
            ScriptFunc::Func(func) => self.func(func, out),
        }
    }

    fn sone(
        self,
        num: &Simple<'_>,
        den: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let mut sink = Sink;
        match den {
            sgroup!(expr) if self.strip_brackets => {
                if let Some(sconf) = out.conf.with_sub()
                    && self.expression(expr, &mut sconf.wrap(&mut sink)).is_ok()
                {
                    out.write_char('⅟')?;
                    self.expression(expr, &mut sconf.wrap(out.inner))
                } else {
                    self.simple(num, out)?;
                    out.write_char('/')?;
                    self.simple(den, out)
                }
            }
            den => {
                if let Some(sconf) = out.conf.with_sub()
                    && self.simple(den, &mut sconf.wrap(&mut sink)).is_ok()
                {
                    out.write_char('⅟')?;
                    let mut w = sconf.wrap(out.inner);
                    self.simple(den, &mut w)
                } else {
                    self.simple(num, out)?;
                    out.write_char('/')?;
                    self.simple(den, out)
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn simplefrac(
        self,
        numer: &Simple<'_>,
        denom: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        // simple vulgar frac
        if self.vulgar_fracs
            && let Some(frac) = extract_vulgar_frac(numer, denom, self.strip_brackets)
        {
            return out.write_char(frac);
        }
        let mut sink = Sink;

        let vsf = self.vulgar_fracs && self.script_fracs;
        match (numer, denom) {
            // one fracs
            (num!("1"), den) if vsf => self.sone(numer, den, out),
            (sgroup!(num), den) if vsf && self.strip_brackets && xnum!(num, "1") => {
                self.sone(numer, den, out)
            }
            // normal
            (sgroup!(num), sgroup!(den)) if self.strip_brackets && self.script_fracs => {
                if let Some(sup_conf) = out.conf.with_sup()
                    && self.expression(num, &mut sup_conf.wrap(&mut sink)).is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self.expression(den, &mut sub_conf.wrap(&mut sink)).is_ok()
                {
                    self.expression(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.expression(den, &mut sub_conf.wrap(out.inner))
                } else {
                    self.simple(numer, out)?;
                    out.write_char('/')?;
                    self.simple(denom, out)
                }
            }
            (num, sgroup!(den)) if self.strip_brackets && self.script_fracs => {
                if let Some(sup_conf) = out.conf.with_sup()
                    && self.simple(num, &mut sup_conf.wrap(&mut sink)).is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self.expression(den, &mut sub_conf.wrap(&mut sink)).is_ok()
                {
                    self.simple(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.expression(den, &mut sub_conf.wrap(out.inner))
                } else {
                    self.simple(num, out)?;
                    out.write_char('/')?;
                    self.simple(denom, out)
                }
            }
            (sgroup!(num), den) if self.strip_brackets && self.script_fracs => {
                if let Some(sup_conf) = out.conf.with_sup()
                    && self.expression(num, &mut sup_conf.wrap(&mut sink)).is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self.simple(den, &mut sub_conf.wrap(&mut sink)).is_ok()
                {
                    self.expression(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.simple(den, &mut sub_conf.wrap(out.inner))
                } else {
                    self.simple(numer, out)?;
                    out.write_char('/')?;
                    self.simple(den, out)
                }
            }
            (num, den) => {
                if self.script_fracs
                    && let Some(sup_conf) = out.conf.with_sup()
                    && self.simple(num, &mut sup_conf.wrap(&mut sink)).is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self.simple(den, &mut sub_conf.wrap(&mut sink)).is_ok()
                {
                    self.simple(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.simple(den, &mut sub_conf.wrap(out.inner))
                } else {
                    self.simple(num, out)?;
                    out.write_char('/')?;
                    self.simple(den, out)
                }
            }
        }
    }

    fn fone(self, den: &ScriptFunc<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        let mut sink = Sink;
        if let Some(sconf) = out.conf.with_sub()
            && self.scriptfunc(den, &mut sconf.wrap(&mut sink)).is_ok()
        {
            out.write_char('⅟')?;
            self.scriptfunc(den, &mut sconf.wrap(out.inner))
        } else {
            out.write_str("1/")?;
            self.scriptfunc(den, out)
        }
    }

    #[allow(clippy::too_many_lines)]
    fn frac(self, frac: &Frac<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        let mut sink = Sink;
        let sv = self.script_fracs && self.vulgar_fracs;
        match (&frac.numer, &frac.denom) {
            // simple frac
            (script_func!(num), script_func!(den)) => self.simplefrac(num, den, out),
            // one vulgar
            (script_func!(num!("1")), den) if sv => self.fone(den, out),
            (script_func!(sgroup!(num)), den) if sv && self.strip_brackets && xnum!(num, "1") => {
                self.fone(den, out)
            }
            // normal fractions
            (script_func!(sgroup!(num)), den) if self.strip_brackets && self.script_fracs => {
                if let Some(sup_conf) = out.conf.with_sup()
                    && self.expression(num, &mut sup_conf.wrap(&mut sink)).is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self.scriptfunc(den, &mut sub_conf.wrap(&mut sink)).is_ok()
                {
                    self.expression(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.scriptfunc(den, &mut sub_conf.wrap(out.inner))
                } else {
                    self.scriptfunc(&frac.numer, out)?;
                    out.write_char('/')?;
                    self.scriptfunc(den, out)
                }
            }
            (num, script_func!(sgroup!(den))) if self.strip_brackets && self.script_fracs => {
                if let Some(sup_conf) = out.conf.with_sup()
                    && self.scriptfunc(num, &mut sup_conf.wrap(&mut sink)).is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self.expression(den, &mut sub_conf.wrap(&mut sink)).is_ok()
                {
                    self.scriptfunc(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.expression(den, &mut sub_conf.wrap(out.inner))
                } else {
                    self.scriptfunc(num, out)?;
                    out.write_char('/')?;
                    self.scriptfunc(&frac.denom, out)
                }
            }
            (num, den) => {
                if self.script_fracs
                    && let Some(sup_conf) = out.conf.with_sup()
                    && self.scriptfunc(num, &mut sup_conf.wrap(&mut sink)).is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self.scriptfunc(den, &mut sub_conf.wrap(&mut sink)).is_ok()
                {
                    self.scriptfunc(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.scriptfunc(den, &mut sub_conf.wrap(out.inner))
                } else {
                    self.scriptfunc(num, out)?;
                    out.write_char('/')?;
                    self.scriptfunc(den, out)
                }
            }
        }
    }

    fn intermediate(
        self,
        inter: &Intermediate<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match inter {
            Intermediate::ScriptFunc(sf) => self.scriptfunc(sf, out),
            Intermediate::Frac(frac) => self.frac(frac, out),
        }
    }

    fn expression(self, expr: &Expression<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        for inter in expr.iter() {
            self.intermediate(inter, out)?;
        }
        Ok(())
    }

    /// Parse an asciimath string into an [`Asciimath`] value that implements [`Display`]
    #[must_use]
    pub fn parse(self, inp: &str) -> Asciimath<'_> {
        let expr = asciimath_parser::parse_tokens(Tokenizer::with_tokens(inp, &*TOKEN_MAP, true));
        Asciimath { conf: self, expr }
    }
}

/// Parsed asciimath expression ready for rendering
///
/// Implements [`Display`] so it can be used with `format!`, `write!`, or `.to_string()`.
#[derive(Debug, Clone)]
pub struct Asciimath<'a> {
    conf: Conf,
    expr: Expression<'a>,
}

impl<'a> Asciimath<'a> {
    /// Extract the inner asciimath expression
    #[must_use]
    pub fn into_inner(self) -> Expression<'a> {
        self.expr
    }

    /// Update the rendering configuration of this `Asciimath`
    #[must_use]
    pub fn with_conf(self, new_conf: Conf) -> Self {
        Self {
            conf: new_conf,
            expr: self.expr,
        }
    }
}

impl fmt::Display for Asciimath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.conf.expression(&self.expr, &mut Mapper::new(f))
    }
}

/// Parse asciimath into an [`Asciimath`] value that implements [`Display`]
#[must_use]
pub fn parse_unicode(inp: &str) -> Asciimath<'_> {
    Conf::default().parse(inp)
}

#[cfg(test)]
mod tests {
    use super::{Conf, SkinTone};

    #[test]
    fn example() {
        let ex = "sum_(i=1)^n i^3=((n(n+1))/2)^2";
        let expected = "∑₍ᵢ₌₁₎ⁿi³=(ⁿ⁽ⁿ⁺¹⁾⁄₂)²";

        let res = super::parse_unicode(ex).to_string();
        assert_eq!(res, expected);

        let rend = Conf::default().parse(ex);
        assert_eq!(format!("{rend}"), expected);
    }

    #[test]
    fn vulgar_fracs() {
        let opts = Conf {
            vulgar_fracs: true,
            ..Default::default()
        };
        let res = opts.parse("1/2").to_string();
        assert_eq!(res, "½");

        let res = opts.parse("a / s").to_string();
        assert_eq!(res, "℁");
    }

    #[test]
    fn stripped_vulgar_fracs() {
        let opts = Conf {
            vulgar_fracs: true,
            strip_brackets: true,
            ..Default::default()
        };
        let res = opts.parse("(1)/2").to_string();
        assert_eq!(res, "½");

        let res = opts.parse("7/[8]").to_string();
        assert_eq!(res, "⅞");

        let res = opts.parse("{a} / (s)").to_string();
        assert_eq!(res, "℁");
    }

    #[test]
    fn script_fracs() {
        let opts = Conf {
            script_fracs: true,
            strip_brackets: false,
            ..Default::default()
        };
        let res = opts.parse("y / x").to_string();
        assert_eq!(res, "ʸ⁄ₓ");

        let res = opts.parse("(y) / x").to_string();
        assert_eq!(res, "⁽ʸ⁾⁄ₓ");
    }

    #[test]
    fn stripped_script_fracs() {
        let opts = Conf {
            script_fracs: true,
            ..Default::default()
        };

        let res = opts.parse("(y) / x").to_string();
        assert_eq!(res, "ʸ⁄ₓ");

        let res = opts.parse("y / [x]").to_string();
        assert_eq!(res, "ʸ⁄ₓ");

        let res = opts.parse("(y)/[x]").to_string();
        assert_eq!(res, "ʸ⁄ₓ");
    }

    #[test]
    fn one_fracs() {
        let res = super::parse_unicode("1/x").to_string();
        assert_eq!(res, "⅟ₓ");

        let res = super::parse_unicode("1 / sinx").to_string();
        assert_eq!(res, "⅟ₛᵢₙ ₓ");

        let opts = Conf {
            script_fracs: false,
            vulgar_fracs: false,
            strip_brackets: false,
            ..Default::default()
        };
        let res = opts.parse("1 / sinx").to_string();
        assert_eq!(res, "1/sin x");
    }

    #[test]
    fn normal_fracs() {
        let opts = Conf {
            script_fracs: false,
            vulgar_fracs: false,
            strip_brackets: false,
            ..Default::default()
        };

        let res = opts.parse("sinx / cosy").to_string();
        assert_eq!(res, "sin x/cos y");
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn unary() {
        let res = super::parse_unicode("sqrt x").to_string();
        assert_eq!(res, "√x");

        let res = super::parse_unicode("vec x").to_string();
        assert_eq!(res, "x\u{20D7}");

        let res = super::parse_unicode("bbb E").to_string();
        assert_eq!(res, "𝔼");

        let res = super::parse_unicode("bbb (E)").to_string();
        assert_eq!(res, "𝔼");

        let res = super::parse_unicode("dot x").to_string();
        assert_eq!(res, "ẋ");

        let res = super::parse_unicode("dot{x}").to_string();
        assert_eq!(res, "ẋ");

        let res = super::parse_unicode("norm x").to_string();
        assert_eq!(res, "||x||");

        let res = super::parse_unicode("sqrt overline x").to_string();
        assert_eq!(res, "√x̅");

        let res = super::parse_unicode("sqrt overline(x)").to_string();
        assert_eq!(res, "√x̅");
    }

    #[test]
    fn binary() {
        let res = super::parse_unicode("root 3 x").to_string();
        assert_eq!(res, "∛x");

        let res = super::parse_unicode("root {4} x").to_string();
        assert_eq!(res, "∜x");

        let res = super::parse_unicode("stackrel *** =").to_string();
        assert_eq!(res, "≛");

        let res = super::parse_unicode("overset a x").to_string();
        assert_eq!(res, "x\u{0363}");

        let res = super::parse_unicode("overset (e) {y}").to_string();
        assert_eq!(res, "y\u{0364}");

        let res = super::parse_unicode("oversetasinx").to_string();
        assert_eq!(res, "overset a sin x");
    }

    #[test]
    fn functions() {
        let res = super::parse_unicode("sin x/x").to_string();
        assert_eq!(res, "ˢⁱⁿ ˣ⁄ₓ");
    }

    #[test]
    fn script() {
        let res = super::parse_unicode("x^sin x").to_string();
        assert_eq!(res, "xˢⁱⁿ ˣ");

        let res = super::parse_unicode("x^vec(x)").to_string();
        assert_eq!(res, "x^x\u{20D7}");

        let res = super::parse_unicode("x_x^y").to_string();
        assert_eq!(res, "xₓʸ");

        let res = super::parse_unicode("x_y^sin x").to_string();
        assert_eq!(res, "x_y^sin x");

        let res = super::parse_unicode("x^sin rho").to_string();
        assert_eq!(res, "x^sin ρ");

        let res = super::parse_unicode("x_x").to_string();
        assert_eq!(res, "xₓ");

        let res = super::parse_unicode("x_y").to_string();
        assert_eq!(res, "x_y");
    }

    #[test]
    fn text() {
        let res = super::parse_unicode("\"text\"").to_string();
        assert_eq!(res, "text");
    }

    #[test]
    fn matrix() {
        let opts = Conf::default();

        let res = opts.parse("[ [x, y], [a, b] ]").to_string();
        assert_eq!(res, "[[x,y],[a,b]]");
    }

    #[test]
    fn skin_tone() {
        let opts = Conf {
            skin_tone: SkinTone::Default,
            ..Default::default()
        };
        let res = opts.parse(":hand:").to_string();
        assert_eq!(res, "✋");

        let opts = Conf {
            skin_tone: SkinTone::Dark,
            ..Default::default()
        };
        let res = opts.parse(":hand:").to_string();
        assert_eq!(res, "✋🏿");
    }

    #[test]
    fn empty_input() {
        assert_eq!(super::parse_unicode("").to_string(), "");
    }

    #[test]
    fn gt_symbol() {
        let res = super::parse_unicode("x > y").to_string();
        assert_eq!(res, "x>y");
    }

    #[test]
    fn land_lor() {
        let res = super::parse_unicode("x land y").to_string();
        assert_eq!(res, "x∧y");

        let res = super::parse_unicode("x lor y").to_string();
        assert_eq!(res, "x∨y");
    }

    #[test]
    fn approx() {
        let res = super::parse_unicode("x approx y").to_string();
        assert_eq!(res, "x≈y");
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn unary_modifiers() {
        let res = super::parse_unicode("hat x").to_string();
        assert_eq!(res, "x̂");

        let res = super::parse_unicode("tilde x").to_string();
        assert_eq!(res, "x̃");

        let res = super::parse_unicode("bar x").to_string();
        assert_eq!(res, "x̄");

        let res = super::parse_unicode("ddot x").to_string();
        assert_eq!(res, "ẍ");

        let res = super::parse_unicode("overarc x").to_string();
        assert_eq!(res, "x̑");

        let res = super::parse_unicode("overparen x").to_string();
        assert_eq!(res, "x̑");

        let res = super::parse_unicode("ul x").to_string();
        assert_eq!(res, "x̲");

        let res = super::parse_unicode("underline x").to_string();
        assert_eq!(res, "x̲");

        let res = super::parse_unicode("cancel x").to_string();
        assert_eq!(res, "x\u{0336}");

        let res = super::parse_unicode("vec x").to_string();
        assert_eq!(res, "x\u{20D7}");

        // non-precomposed: q has no precomposed dot or ddot form
        let res = super::parse_unicode("dot q").to_string();
        assert_eq!(res, "q\u{0307}");

        let res = super::parse_unicode("ddot q").to_string();
        assert_eq!(res, "q\u{0308}");

        let res = super::parse_unicode("hat q").to_string();
        assert_eq!(res, "q\u{0302}");

        let res = super::parse_unicode("tilde q").to_string();
        assert_eq!(res, "q\u{0303}");

        let res = super::parse_unicode("bar q").to_string();
        assert_eq!(res, "q\u{0304}");

        let res = super::parse_unicode("vec q").to_string();
        assert_eq!(res, "q\u{20D7}");
    }

    #[test]
    fn vulgar_fraction_patterns() {
        let opts = Conf {
            vulgar_fracs: true,
            ..Default::default()
        };
        assert_eq!(opts.parse("1/4").to_string(), "¼");
        assert_eq!(opts.parse("1/3").to_string(), "⅓");
        assert_eq!(opts.parse("2/3").to_string(), "⅔");
        assert_eq!(opts.parse("1/5").to_string(), "⅕");
        assert_eq!(opts.parse("2/5").to_string(), "⅖");
        assert_eq!(opts.parse("3/5").to_string(), "⅗");
        assert_eq!(opts.parse("4/5").to_string(), "⅘");
        assert_eq!(opts.parse("1/6").to_string(), "⅙");
        assert_eq!(opts.parse("5/6").to_string(), "⅚");
        assert_eq!(opts.parse("1/7").to_string(), "⅐");
        assert_eq!(opts.parse("1/8").to_string(), "⅛");
        assert_eq!(opts.parse("3/8").to_string(), "⅜");
        assert_eq!(opts.parse("5/8").to_string(), "⅝");
        assert_eq!(opts.parse("7/8").to_string(), "⅞");
        assert_eq!(opts.parse("1/9").to_string(), "⅑");
        assert_eq!(opts.parse("1/10").to_string(), "⅒");
        assert_eq!(opts.parse("3/4").to_string(), "¾");
    }

    #[test]
    fn config_no_strip_no_vulgar_no_script() {
        let opts = Conf {
            strip_brackets: false,
            vulgar_fracs: false,
            script_fracs: false,
            ..Default::default()
        };
        let res = opts.parse("(x)/y").to_string();
        assert_eq!(res, "(x)/y");
    }

    #[test]
    fn config_strip_no_vulgar_no_script() {
        let opts = Conf {
            strip_brackets: true,
            vulgar_fracs: false,
            script_fracs: false,
            ..Default::default()
        };
        let res = opts.parse("(x)/y").to_string();
        assert_eq!(res, "(x)/y");
    }

    #[test]
    fn config_no_strip_vulgar_no_script() {
        let opts = Conf {
            strip_brackets: false,
            vulgar_fracs: true,
            script_fracs: false,
            ..Default::default()
        };
        let res = opts.parse("1/2").to_string();
        assert_eq!(res, "½");
    }

    #[test]
    fn config_no_strip_no_vulgar_script() {
        let opts = Conf {
            strip_brackets: false,
            vulgar_fracs: false,
            script_fracs: true,
            ..Default::default()
        };
        // y is not subscriptable, falls through to plain frac
        let res = opts.parse("x/y").to_string();
        assert_eq!(res, "x/y");
        // x is both super/subscriptable
        let res = opts.parse("x/x").to_string();
        assert_eq!(res, "ˣ⁄ₓ");
    }

    #[test]
    fn deeply_nested() {
        let res = super::parse_unicode("((((x))))").to_string();
        assert_eq!(res, "((((x))))");

        let res = super::parse_unicode("sqrt sqrt sqrt x").to_string();
        assert_eq!(res, "√√√x");
    }

    #[test]
    fn non_ascii_ident() {
        // non-ASCII characters pass through as identifiers
        let res = super::parse_unicode("λ").to_string();
        assert_eq!(res, "λ");
    }
}
