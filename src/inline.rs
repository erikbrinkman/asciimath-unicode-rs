#![allow(missing_docs, clippy::missing_errors_doc)]

use asciimath_parser::tree::{
    Expression, Frac, Func, Group, Intermediate, Matrix, Script, ScriptFunc, Simple, SimpleBinary,
    SimpleFunc, SimpleScript, SimpleUnary,
};
use std::fmt;
use std::fmt::Write;
use unicode_normalization::char::compose;

use super::Conf;
use super::ast::{extract_single_char, extract_vulgar_frac};
use super::tokens::{
    bold_map, cal_map, double_map, frak_map, italic_map, left_bracket_str, mono_map,
    right_bracket_str, sans_map, subscript_char, superscript_char, symbol_str,
};

#[derive(Debug)]
pub struct Sink;

impl fmt::Write for Sink {
    fn write_str(&mut self, _: &str) -> fmt::Result {
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MapperConf {
    pub font: Option<fn(char) -> char>,
    pub sub_sup: Option<fn(char) -> Option<char>>,
    pub modifier: Option<char>,
}

impl MapperConf {
    /// This method allows checking if we can apply `sub_sup` without borrowing the inner writer
    pub fn with_sub_sup(&self, sub_sup: fn(char) -> Option<char>) -> Option<MapperConf> {
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

    pub fn with_sub(&self) -> Option<MapperConf> {
        self.with_sub_sup(subscript_char)
    }

    pub fn with_sup(&self) -> Option<MapperConf> {
        self.with_sub_sup(superscript_char)
    }

    pub fn wrap<S: Write>(self, other: &mut S) -> Mapper<'_, S> {
        Mapper {
            inner: other,
            conf: self,
        }
    }
}

#[derive(Debug)]
pub struct Mapper<'a, W: ?Sized> {
    pub inner: &'a mut W,
    pub conf: MapperConf,
}

impl<'a, W: fmt::Write + ?Sized> Mapper<'a, W> {
    pub fn new(inner: &'a mut W) -> Self {
        Mapper {
            inner,
            conf: MapperConf::default(),
        }
    }

    pub fn with_font(&mut self, f: fn(char) -> char) -> Mapper<'_, W> {
        Mapper {
            inner: &mut *self.inner,
            conf: MapperConf {
                font: Some(f),
                sub_sup: self.conf.sub_sup,
                modifier: self.conf.modifier,
            },
        }
    }

    pub fn with_modifier(&mut self, c: char) -> Mapper<'_, W> {
        Mapper {
            inner: &mut *self.inner,
            conf: MapperConf {
                font: self.conf.font,
                sub_sup: self.conf.sub_sup,
                modifier: Some(c),
            },
        }
    }

    pub fn onto<'b, S: Write>(&self, other: &'b mut S) -> Mapper<'b, S> {
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

impl Conf {
    fn inline_simplefunc(
        self,
        simple: &SimpleFunc<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(simple.func)?;
        out.write_char(' ')?;
        self.inline_simple(simple.arg(), out)
    }

    fn inline_root(
        self,
        root_char: char,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_char(root_char)?;
        self.inline_simple(arg, out)
    }

    fn inline_cover(
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
                if self
                    .inline_expression(expr, &mut out.onto(&mut single))
                    .is_ok()
                    && let Some(res) = single.0
                {
                    out.write_char(res)?;
                    out.write_char(chr)
                } else {
                    self.inline_bgeneric(op, first, arg, out)
                }
            }
            arg => {
                let mut single = SingleChar::default();
                if self.inline_simple(arg, &mut out.onto(&mut single)).is_ok()
                    && let Some(res) = single.0
                {
                    out.write_char(res)?;
                    out.write_char(chr)
                } else {
                    self.inline_bgeneric(op, first, arg, out)
                }
            }
        }
    }

    fn inline_equals(
        self,
        op: &str,
        first: &Simple<'_>,
        second: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let mut buf = SmallBuf::default();
        let scan = match first {
            sgroup!(expr) if self.strip_brackets => {
                self.inline_expression(expr, &mut out.onto(&mut buf))
            }
            f => self.inline_simple(f, &mut out.onto(&mut buf)),
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
            self.inline_bgeneric(op, first, second, out)
        }
    }

    fn inline_bgeneric(
        self,
        op: &str,
        first: &Simple<'_>,
        second: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(op)?;
        out.write_char(' ')?;
        self.inline_simple(first, out)?;
        out.write_char(' ')?;
        self.inline_simple(second, out)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn inline_simplebinary(
        self,
        simple: &SimpleBinary<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let sb = self.strip_brackets;
        match (simple.op, simple.first(), simple.second()) {
            // roots
            ("root", num!("2"), arg) => self.inline_root('√', arg, out),
            ("root", num!("3"), arg) => self.inline_root('∛', arg, out),
            ("root", num!("4"), arg) => self.inline_root('∜', arg, out),
            ("root", sgroup!(expr), arg) if xnum!(expr, "2") => self.inline_root('√', arg, out),
            ("root", sgroup!(expr), arg) if xnum!(expr, "3") => self.inline_root('∛', arg, out),
            ("root", sgroup!(expr), arg) if xnum!(expr, "4") => self.inline_root('∜', arg, out),
            // frac
            ("frac", numer, denom) => self.inline_simplefrac(numer, denom, out),
            // stackrel / overset combining
            (o @ ("stackrel" | "overset"), f @ iden!("a"), a) => {
                self.inline_cover(o, f, a, '\u{0363}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("e"), a) => {
                self.inline_cover(o, f, a, '\u{0364}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("i"), a) => {
                self.inline_cover(o, f, a, '\u{0365}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("o"), a) => {
                self.inline_cover(o, f, a, '\u{0366}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("u"), a) => {
                self.inline_cover(o, f, a, '\u{0367}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("c"), a) => {
                self.inline_cover(o, f, a, '\u{0368}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("d"), a) => {
                self.inline_cover(o, f, a, '\u{0369}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("h"), a) => {
                self.inline_cover(o, f, a, '\u{036a}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("m"), a) => {
                self.inline_cover(o, f, a, '\u{036b}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("r"), a) => {
                self.inline_cover(o, f, a, '\u{036c}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("t"), a) => {
                self.inline_cover(o, f, a, '\u{036d}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("v"), a) => {
                self.inline_cover(o, f, a, '\u{036e}', out)
            }
            (o @ ("stackrel" | "overset"), f @ iden!("x"), a) => {
                self.inline_cover(o, f, a, '\u{036f}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "a") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{0363}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "e") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{0364}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "i") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{0365}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "o") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{0366}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "u") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{0367}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "c") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{0368}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "d") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{0369}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "h") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{036a}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "m") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{036b}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "r") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{036c}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "t") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{036d}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "v") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{036e}', out)
            }
            ("stackrel" | "overset", sgroup!(exp), arg) if sb && xiden!(exp, "x") => {
                self.inline_cover(simple.op, simple.first(), arg, '\u{036f}', out)
            }
            // stackrel / overset equals
            ("stackrel" | "overset", arg, symb!("=")) => {
                self.inline_equals(simple.op, arg, simple.second(), out)
            }
            // generic
            (op, first, second) => self.inline_bgeneric(op, first, second, out),
        }
    }

    fn inline_font(
        self,
        font: fn(char) -> char,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let mut w = out.with_font(font);
        match arg {
            sgroup!(expr) if self.strip_brackets => self.inline_expression(expr, &mut w),
            arg => self.inline_simple(arg, &mut w),
        }
    }

    fn inline_sfunc(
        self,
        open: &str,
        arg: &Simple<'_>,
        close: &str,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(open)?;
        match arg {
            sgroup!(expr) if self.strip_brackets => self.inline_expression(expr, out)?,
            arg => self.inline_simple(arg, out)?,
        }
        out.write_str(close)
    }

    fn inline_modi(
        self,
        chr: char,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let mut w = out.with_modifier(chr);
        match arg {
            sgroup!(expr) if self.strip_brackets => self.inline_expression(expr, &mut w),
            arg => self.inline_simple(arg, &mut w),
        }
    }

    fn inline_char_modi(
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
                    if self
                        .inline_expression(expr, &mut out.onto(&mut single))
                        .is_ok()
                        && let Some(res) = single.0
                    {
                        out.write_char(res)?;
                        out.write_char(chr)
                    } else {
                        self.inline_ugeneric(op, arg, out)
                    }
                }
                arg => {
                    let mut single = SingleChar::default();
                    if self.inline_simple(arg, &mut out.onto(&mut single)).is_ok()
                        && let Some(res) = single.0
                    {
                        out.write_char(res)?;
                        out.write_char(chr)
                    } else {
                        self.inline_ugeneric(op, arg, out)
                    }
                }
            }
        }
    }

    fn inline_ugeneric(
        self,
        op: &str,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(op)?;
        out.write_char(' ')?;
        self.inline_simple(arg, out)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn inline_simpleunary(
        self,
        simple: &SimpleUnary<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match (simple.op, simple.arg()) {
            // sqrt
            ("sqrt", arg) => {
                out.write_char('√')?;
                self.inline_simple(arg, out)
            }
            // fonts
            ("bb" | "mathbf", arg) => self.inline_font(bold_map, arg, out),
            ("bbb" | "mathbb", arg) => self.inline_font(double_map, arg, out),
            ("cc" | "mathcal", arg) => self.inline_font(cal_map, arg, out),
            ("tt" | "mathtt", arg) => self.inline_font(mono_map, arg, out),
            ("fr" | "mathfrak", arg) => self.inline_font(frak_map, arg, out),
            ("sf" | "mathsf", arg) => self.inline_font(sans_map, arg, out),
            ("it" | "mathit", arg) => self.inline_font(italic_map, arg, out),
            // functions
            ("abs" | "Abs", arg) => self.inline_sfunc("|", arg, "|", out),
            ("ceil", arg) => self.inline_sfunc("⌈", arg, "⌉", out),
            ("floor", arg) => self.inline_sfunc("⌊", arg, "⌋", out),
            ("norm", arg) => self.inline_sfunc("||", arg, "||", out),
            ("text" | "mbox", arg) => self.inline_sfunc("", arg, "", out),
            // modifiers
            ("overline", arg) => self.inline_modi('\u{0305}', arg, out),
            ("underline" | "ul", arg) => self.inline_modi('\u{0332}', arg, out),
            ("cancel", arg) => self.inline_modi('\u{0336}', arg, out),
            // single character modifiers
            (o @ "hat", arg) => self.inline_char_modi(o, '\u{0302}', arg, out),
            (o @ "tilde", arg) => self.inline_char_modi(o, '\u{0303}', arg, out),
            (o @ "bar", arg) => self.inline_char_modi(o, '\u{0304}', arg, out),
            (o @ "dot", arg) => self.inline_char_modi(o, '\u{0307}', arg, out),
            (o @ "ddot", arg) => self.inline_char_modi(o, '\u{0308}', arg, out),
            (o @ ("overarc" | "overparen"), arg) => self.inline_char_modi(o, '\u{0311}', arg, out),
            (o @ "vec", arg) => self.inline_char_modi(o, '\u{20D7}', arg, out),
            // generic
            (op, arg) => self.inline_ugeneric(op, arg, out),
        }
    }

    fn inline_matrix(self, matrix: &Matrix<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
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
                self.inline_expression(expr, out)?;
            }
            out.write_str(right)?;
        }
        out.write_str(right)
    }

    fn inline_group(self, group: &Group<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        out.write_str(left_bracket_str(group.left_bracket))?;
        self.inline_expression(&group.expr, out)?;
        out.write_str(right_bracket_str(group.right_bracket))
    }

    pub(crate) fn inline_simple(
        self,
        simple: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match simple {
            Simple::Missing => Ok(()),
            &Simple::Number(num) => out.write_str(num),
            &Simple::Text(text) => out.write_str(text),
            &Simple::Ident(ident) => out.write_str(ident),
            &Simple::Symbol(symbol) => out.write_str(symbol_str(symbol, self.skin_tone)),
            Simple::Func(func) => self.inline_simplefunc(func, out),
            Simple::Unary(unary) => self.inline_simpleunary(unary, out),
            Simple::Binary(binary) => self.inline_simplebinary(binary, out),
            Simple::Group(group) => self.inline_group(group, out),
            Simple::Matrix(matrix) => self.inline_matrix(matrix, out),
        }
    }

    fn inline_script(self, script: &Script<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        let mut sink = Sink;
        match script {
            Script::None => Ok(()),
            Script::Sub(sub) => {
                if let Some(sconf) = out.conf.with_sub()
                    && self.inline_simple(sub, &mut sconf.wrap(&mut sink)).is_ok()
                {
                    self.inline_simple(sub, &mut sconf.wrap(out.inner))
                } else {
                    out.write_char('_')?;
                    self.inline_simple(sub, out)
                }
            }
            Script::Super(sup) => {
                if let Some(sconf) = out.conf.with_sup()
                    && self.inline_simple(sup, &mut sconf.wrap(&mut sink)).is_ok()
                {
                    self.inline_simple(sup, &mut sconf.wrap(out.inner))
                } else {
                    out.write_char('^')?;
                    self.inline_simple(sup, out)
                }
            }
            Script::Subsuper(sub, sup) => {
                if let Some(sub_conf) = out.conf.with_sub()
                    && self
                        .inline_simple(sub, &mut sub_conf.wrap(&mut sink))
                        .is_ok()
                    && let Some(sup_conf) = out.conf.with_sup()
                    && self
                        .inline_simple(sup, &mut sup_conf.wrap(&mut sink))
                        .is_ok()
                {
                    self.inline_simple(sub, &mut sub_conf.wrap(out.inner))?;
                    self.inline_simple(sup, &mut sup_conf.wrap(out.inner))
                } else {
                    out.write_char('_')?;
                    self.inline_simple(sub, out)?;
                    out.write_char('^')?;
                    self.inline_simple(sup, out)
                }
            }
        }
    }

    fn inline_simplescript(
        self,
        simple: &SimpleScript<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        self.inline_simple(&simple.simple, out)?;
        self.inline_script(&simple.script, out)
    }

    fn inline_func(self, func: &Func<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        out.write_str(func.func)?;
        self.inline_script(&func.script, out)?;
        out.write_char(' ')?;
        self.inline_scriptfunc(func.arg(), out)
    }

    fn inline_scriptfunc(
        self,
        func: &ScriptFunc<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match func {
            ScriptFunc::Simple(simple) => self.inline_simplescript(simple, out),
            ScriptFunc::Func(func) => self.inline_func(func, out),
        }
    }

    fn inline_sone(
        self,
        _num: &Simple<'_>,
        den: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let mut sink = Sink;
        match den {
            sgroup!(expr) if self.strip_brackets => {
                if let Some(sconf) = out.conf.with_sub()
                    && self
                        .inline_expression(expr, &mut sconf.wrap(&mut sink))
                        .is_ok()
                {
                    out.write_char('⅟')?;
                    self.inline_expression(expr, &mut sconf.wrap(out.inner))
                } else {
                    Err(fmt::Error)
                }
            }
            den => {
                if let Some(sconf) = out.conf.with_sub()
                    && self.inline_simple(den, &mut sconf.wrap(&mut sink)).is_ok()
                {
                    out.write_char('⅟')?;
                    let mut w = sconf.wrap(out.inner);
                    self.inline_simple(den, &mut w)
                } else {
                    Err(fmt::Error)
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn inline_simplefrac(
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
            (num!("1"), den) if vsf => self.inline_sone(numer, den, out),
            (sgroup!(num), den) if vsf && self.strip_brackets && xnum!(num, "1") => {
                self.inline_sone(numer, den, out)
            }
            // normal
            (sgroup!(num), sgroup!(den)) if self.strip_brackets && self.script_fracs => {
                if let Some(sup_conf) = out.conf.with_sup()
                    && self
                        .inline_expression(num, &mut sup_conf.wrap(&mut sink))
                        .is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self
                        .inline_expression(den, &mut sub_conf.wrap(&mut sink))
                        .is_ok()
                {
                    self.inline_expression(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.inline_expression(den, &mut sub_conf.wrap(out.inner))
                } else {
                    Err(fmt::Error)
                }
            }
            (num, sgroup!(den)) if self.strip_brackets && self.script_fracs => {
                if let Some(sup_conf) = out.conf.with_sup()
                    && self
                        .inline_simple(num, &mut sup_conf.wrap(&mut sink))
                        .is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self
                        .inline_expression(den, &mut sub_conf.wrap(&mut sink))
                        .is_ok()
                {
                    self.inline_simple(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.inline_expression(den, &mut sub_conf.wrap(out.inner))
                } else {
                    Err(fmt::Error)
                }
            }
            (sgroup!(num), den) if self.strip_brackets && self.script_fracs => {
                if let Some(sup_conf) = out.conf.with_sup()
                    && self
                        .inline_expression(num, &mut sup_conf.wrap(&mut sink))
                        .is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self
                        .inline_simple(den, &mut sub_conf.wrap(&mut sink))
                        .is_ok()
                {
                    self.inline_expression(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.inline_simple(den, &mut sub_conf.wrap(out.inner))
                } else {
                    Err(fmt::Error)
                }
            }
            (num, den) => {
                if self.script_fracs
                    && let Some(sup_conf) = out.conf.with_sup()
                    && self
                        .inline_simple(num, &mut sup_conf.wrap(&mut sink))
                        .is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self
                        .inline_simple(den, &mut sub_conf.wrap(&mut sink))
                        .is_ok()
                {
                    self.inline_simple(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.inline_simple(den, &mut sub_conf.wrap(out.inner))
                } else {
                    Err(fmt::Error)
                }
            }
        }
    }

    fn inline_fone(self, den: &ScriptFunc<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        let mut sink = Sink;
        if let Some(sconf) = out.conf.with_sub()
            && self
                .inline_scriptfunc(den, &mut sconf.wrap(&mut sink))
                .is_ok()
        {
            out.write_char('⅟')?;
            self.inline_scriptfunc(den, &mut sconf.wrap(out.inner))
        } else {
            Err(fmt::Error)
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn inline_frac(
        self,
        frac: &Frac<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let mut sink = Sink;
        let sv = self.script_fracs && self.vulgar_fracs;
        match (&frac.numer, &frac.denom) {
            // simple frac
            (script_func!(num), script_func!(den)) => {
                self.inline_simplefrac(num, den, out).or_else(|_| {
                    self.inline_simple(num, out)?;
                    out.write_char('/')?;
                    self.inline_simple(den, out)
                })
            }
            // one vulgar
            (script_func!(num!("1")), den) if sv => self.inline_fone(den, out).or_else(|_| {
                out.write_str("1/")?;
                self.inline_scriptfunc(den, out)
            }),
            (script_func!(sgroup!(num)), den) if sv && self.strip_brackets && xnum!(num, "1") => {
                self.inline_fone(den, out).or_else(|_| {
                    out.write_str("1/")?;
                    self.inline_scriptfunc(den, out)
                })
            }
            // normal fractions
            (script_func!(sgroup!(num)), den) if self.strip_brackets && self.script_fracs => {
                if let Some(sup_conf) = out.conf.with_sup()
                    && self
                        .inline_expression(num, &mut sup_conf.wrap(&mut sink))
                        .is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self
                        .inline_scriptfunc(den, &mut sub_conf.wrap(&mut sink))
                        .is_ok()
                {
                    self.inline_expression(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.inline_scriptfunc(den, &mut sub_conf.wrap(out.inner))
                } else {
                    Err(fmt::Error)
                }
            }
            (num, script_func!(sgroup!(den))) if self.strip_brackets && self.script_fracs => {
                if let Some(sup_conf) = out.conf.with_sup()
                    && self
                        .inline_scriptfunc(num, &mut sup_conf.wrap(&mut sink))
                        .is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self
                        .inline_expression(den, &mut sub_conf.wrap(&mut sink))
                        .is_ok()
                {
                    self.inline_scriptfunc(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.inline_expression(den, &mut sub_conf.wrap(out.inner))
                } else {
                    Err(fmt::Error)
                }
            }
            (num, den) => {
                if self.script_fracs
                    && let Some(sup_conf) = out.conf.with_sup()
                    && self
                        .inline_scriptfunc(num, &mut sup_conf.wrap(&mut sink))
                        .is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self
                        .inline_scriptfunc(den, &mut sub_conf.wrap(&mut sink))
                        .is_ok()
                {
                    self.inline_scriptfunc(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.inline_scriptfunc(den, &mut sub_conf.wrap(out.inner))
                } else {
                    Err(fmt::Error)
                }
            }
        }
    }

    fn inline_intermediate(
        self,
        inter: &Intermediate<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match inter {
            Intermediate::ScriptFunc(sf) => self.inline_scriptfunc(sf, out),
            Intermediate::Frac(frac) => self.inline_frac(frac, out).or_else(|_| {
                self.inline_scriptfunc(&frac.numer, out)?;
                out.write_char('/')?;
                self.inline_scriptfunc(&frac.denom, out)
            }),
        }
    }

    pub(crate) fn inline_expression(
        self,
        expr: &Expression<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        for inter in expr.iter() {
            self.inline_intermediate(inter, out)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Conf, SkinTone};

    #[test]
    fn example() {
        let ex = "sum_(i=1)^n i^3=((n(n+1))/2)^2";
        let expected = "∑₍ᵢ₌₁₎ⁿi³=(ⁿ⁽ⁿ⁺¹⁾⁄₂)²";

        let res = super::super::parse_unicode(ex).to_string();
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
        let res = super::super::parse_unicode("1/x").to_string();
        assert_eq!(res, "⅟ₓ");

        let res = super::super::parse_unicode("1 / sinx").to_string();
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
        let res = super::super::parse_unicode("sqrt x").to_string();
        assert_eq!(res, "√x");

        let res = super::super::parse_unicode("vec x").to_string();
        assert_eq!(res, "x\u{20D7}");

        let res = super::super::parse_unicode("bbb E").to_string();
        assert_eq!(res, "𝔼");

        let res = super::super::parse_unicode("bbb (E)").to_string();
        assert_eq!(res, "𝔼");

        let res = super::super::parse_unicode("dot x").to_string();
        assert_eq!(res, "ẋ");

        let res = super::super::parse_unicode("dot{x}").to_string();
        assert_eq!(res, "ẋ");

        let res = super::super::parse_unicode("norm x").to_string();
        assert_eq!(res, "||x||");

        let res = super::super::parse_unicode("sqrt overline x").to_string();
        assert_eq!(res, "√x̅");

        let res = super::super::parse_unicode("sqrt overline(x)").to_string();
        assert_eq!(res, "√x̅");
    }

    #[test]
    fn binary() {
        let res = super::super::parse_unicode("root 3 x").to_string();
        assert_eq!(res, "∛x");

        let res = super::super::parse_unicode("root {4} x").to_string();
        assert_eq!(res, "∜x");

        let res = super::super::parse_unicode("stackrel *** =").to_string();
        assert_eq!(res, "≛");

        let res = super::super::parse_unicode("overset a x").to_string();
        assert_eq!(res, "x\u{0363}");

        let res = super::super::parse_unicode("overset (e) {y}").to_string();
        assert_eq!(res, "y\u{0364}");

        let res = super::super::parse_unicode("oversetasinx").to_string();
        assert_eq!(res, "overset a sin x");
    }

    #[test]
    fn functions() {
        let res = super::super::parse_unicode("sin x/x").to_string();
        assert_eq!(res, "ˢⁱⁿ ˣ⁄ₓ");
    }

    #[test]
    fn script() {
        let res = super::super::parse_unicode("x^sin x").to_string();
        assert_eq!(res, "xˢⁱⁿ ˣ");

        let res = super::super::parse_unicode("x^vec(x)").to_string();
        assert_eq!(res, "x^x\u{20D7}");

        let res = super::super::parse_unicode("x_x^y").to_string();
        assert_eq!(res, "xₓʸ");

        let res = super::super::parse_unicode("x_y^sin x").to_string();
        assert_eq!(res, "x_y^sin x");

        let res = super::super::parse_unicode("x^sin rho").to_string();
        assert_eq!(res, "x^sin ρ");

        let res = super::super::parse_unicode("x_x").to_string();
        assert_eq!(res, "xₓ");

        let res = super::super::parse_unicode("x_y").to_string();
        assert_eq!(res, "x_y");
    }

    #[test]
    fn text() {
        let res = super::super::parse_unicode("\"text\"").to_string();
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
        assert_eq!(super::super::parse_unicode("").to_string(), "");
    }

    #[test]
    fn gt_symbol() {
        let res = super::super::parse_unicode("x > y").to_string();
        assert_eq!(res, "x>y");
    }

    #[test]
    fn land_lor() {
        let res = super::super::parse_unicode("x land y").to_string();
        assert_eq!(res, "x∧y");

        let res = super::super::parse_unicode("x lor y").to_string();
        assert_eq!(res, "x∨y");
    }

    #[test]
    fn approx() {
        let res = super::super::parse_unicode("x approx y").to_string();
        assert_eq!(res, "x≈y");
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn unary_modifiers() {
        let res = super::super::parse_unicode("hat x").to_string();
        assert_eq!(res, "x̂");

        let res = super::super::parse_unicode("tilde x").to_string();
        assert_eq!(res, "x̃");

        let res = super::super::parse_unicode("bar x").to_string();
        assert_eq!(res, "x̄");

        let res = super::super::parse_unicode("ddot x").to_string();
        assert_eq!(res, "ẍ");

        let res = super::super::parse_unicode("overarc x").to_string();
        assert_eq!(res, "x̑");

        let res = super::super::parse_unicode("overparen x").to_string();
        assert_eq!(res, "x̑");

        let res = super::super::parse_unicode("ul x").to_string();
        assert_eq!(res, "x̲");

        let res = super::super::parse_unicode("underline x").to_string();
        assert_eq!(res, "x̲");

        let res = super::super::parse_unicode("cancel x").to_string();
        assert_eq!(res, "x\u{0336}");

        let res = super::super::parse_unicode("vec x").to_string();
        assert_eq!(res, "x\u{20D7}");

        // non-precomposed: q has no precomposed dot or ddot form
        let res = super::super::parse_unicode("dot q").to_string();
        assert_eq!(res, "q\u{0307}");

        let res = super::super::parse_unicode("ddot q").to_string();
        assert_eq!(res, "q\u{0308}");

        let res = super::super::parse_unicode("hat q").to_string();
        assert_eq!(res, "q\u{0302}");

        let res = super::super::parse_unicode("tilde q").to_string();
        assert_eq!(res, "q\u{0303}");

        let res = super::super::parse_unicode("bar q").to_string();
        assert_eq!(res, "q\u{0304}");

        let res = super::super::parse_unicode("vec q").to_string();
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
        let res = super::super::parse_unicode("((((x))))").to_string();
        assert_eq!(res, "((((x))))");

        let res = super::super::parse_unicode("sqrt sqrt sqrt x").to_string();
        assert_eq!(res, "√√√x");
    }

    #[test]
    fn non_ascii_ident() {
        // non-ASCII characters pass through as identifiers
        let res = super::super::parse_unicode("λ").to_string();
        assert_eq!(res, "λ");
    }
}
