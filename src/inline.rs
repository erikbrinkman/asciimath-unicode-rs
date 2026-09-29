#![allow(missing_docs, clippy::missing_errors_doc)]

use asciimath_parser::tree::{
    Expression, Frac, Func, Group, Intermediate, Matrix, Script, ScriptFunc, Simple, SimpleBinary,
    SimpleFunc, SimpleScript, SimpleUnary,
};
use std::fmt;
use std::fmt::Write;
use unicode_normalization::char::compose;

use super::Conf;
use super::ast::{
    extract_vulgar_frac, func_hugs_argument, hugs_argument, needs_space, paren_contents,
    unwrap_parens,
};
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
                // a line through brackets would suggest they're part of the marked text
                if let Some(modifier) = self.conf.modifier
                    && !c.is_whitespace()
                    && !is_bracket_char(c)
                {
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

macro_rules! oper {
    ($op:pat) => {
        Simple::Operator($op)
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

fn only<T>(mut iter: impl Iterator<Item = T>) -> Option<T> {
    let first = iter.next();
    if iter.next().is_none() { first } else { None }
}

fn is_bracket_char(chr: char) -> bool {
    matches!(
        chr,
        '(' | ')' | '[' | ']' | '{' | '}' | '|' | '⟨' | '⟩' | '⌊' | '⌋' | '⌈' | '⌉'
    )
}

fn combining_letter(letter: &str) -> Option<char> {
    match letter {
        "a" => Some('\u{0363}'),
        "e" => Some('\u{0364}'),
        "i" => Some('\u{0365}'),
        "o" => Some('\u{0366}'),
        "u" => Some('\u{0367}'),
        "c" => Some('\u{0368}'),
        "d" => Some('\u{0369}'),
        "h" => Some('\u{036a}'),
        "m" => Some('\u{036b}'),
        "r" => Some('\u{036c}'),
        "t" => Some('\u{036d}'),
        "v" => Some('\u{036e}'),
        "x" => Some('\u{036f}'),
        _ => None,
    }
}

/// Whether `simple` is a prefix minus and its operand, which reads badly after `⅟`
fn is_negated(simple: &Simple<'_>) -> bool {
    matches!(
        paren_contents(simple).map(|expr| &**expr),
        Some([Intermediate::ScriptFunc(script_func!(oper!("-"))), _])
    )
}

/// How many vertical lines the matrix draws at a column boundary
pub(crate) fn column_rules(matrix: &Matrix<'_>, boundary: usize) -> usize {
    matrix
        .column_lines()
        .iter()
        .filter(|&&line| line == boundary)
        .count()
}

fn root_char(index: &Simple<'_>) -> Option<char> {
    match index {
        num!("2") => Some('√'),
        num!("3") => Some('∛'),
        num!("4") => Some('∜'),
        _ => None,
    }
}

impl Conf {
    pub(crate) fn stripped<'s, 'a>(self, simple: &'s Simple<'a>) -> Option<&'s Expression<'a>> {
        if self.strip_brackets {
            paren_contents(simple)
        } else {
            None
        }
    }

    fn unwrap_single<'s, 'a>(self, simple: &'s Simple<'a>) -> &'s Simple<'a> {
        if self.strip_brackets {
            unwrap_parens(simple)
        } else {
            simple
        }
    }

    fn inline_simplefunc(
        self,
        simple: &SimpleFunc<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(simple.func)?;
        if !hugs_argument(simple.arg()) {
            out.write_char(' ')?;
        }
        self.inline_simple(simple.arg(), out)
    }

    fn inline_root(
        self,
        root: &str,
        radicand: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(root)?;
        self.inline_simple(self.unwrap_single(radicand), out)
    }

    fn inline_nroot(
        self,
        index: &Simple<'_>,
        radicand: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        // the index is never shown as-is, so its brackets are always syntax
        if let Some(root) = root_char(unwrap_parens(index)) {
            let mut buf = [0; 4];
            self.inline_root(root.encode_utf8(&mut buf), radicand, out)
        } else if let Some(sup_conf) = out.conf.with_sup()
            && self
                .inline_simple_stripped(index, &mut sup_conf.wrap(&mut Sink))
                .is_ok()
        {
            self.inline_simple_stripped(index, &mut sup_conf.wrap(out.inner))?;
            self.inline_root("√", radicand, out)
        } else {
            self.inline_bgeneric("root", index, radicand, out)
        }
    }

    /// Render `simple` into a single char if it is one, already styled by `out`
    fn single_char(self, simple: &Simple<'_>, out: &Mapper<impl fmt::Write>) -> Option<char> {
        let mut single = SingleChar::default();
        self.inline_simple_stripped(simple, &mut out.onto(&mut single))
            .ok()?;
        single.0
    }

    /// The relation symbol for `=` with `over` stacked above it
    fn equals_char(self, over: &Simple<'_>, out: &Mapper<impl fmt::Write>) -> Option<char> {
        let mut buf = SmallBuf::default();
        self.inline_simple_stripped(over, &mut out.onto(&mut buf))
            .ok()?;
        match buf.as_str()? {
            "∘" => Some('\u{2257}'),
            "⋆" => Some('\u{225b}'),
            "△" => Some('\u{225c}'),
            "def" => Some('\u{225d}'),
            "m" => Some('\u{225e}'),
            "?" => Some('\u{225f}'),
            _ => None,
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

    fn inline_overset(
        self,
        over: &Simple<'_>,
        base: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        if let iden!(letter) = self.unwrap_single(over)
            && let Some(mark) = combining_letter(letter)
            && let Some(base_char) = self.single_char(base, out)
        {
            // base_char is already styled; write to inner so the font isn't re-applied
            out.inner.write_char(base_char)?;
            out.write_char(mark)
        } else if matches!(self.unwrap_single(base), symb!("="))
            && let Some(relation) = self.equals_char(over, out)
        {
            out.write_char(relation)
        } else {
            self.inline_simple_stripped(base, out)?;
            self.inline_sub_or_sup(over, out.conf.with_sup(), '^', out)
        }
    }

    pub(crate) fn inline_simplebinary(
        self,
        simple: &SimpleBinary<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match (simple.op, simple.first(), simple.second()) {
            ("root", index, radicand) => self.inline_nroot(index, radicand, out),
            ("frac", numer, denom) => self.inline_simplefrac(numer, denom, out).or_else(|_| {
                self.inline_simple(numer, out)?;
                out.write_char('/')?;
                self.inline_simple(denom, out)
            }),
            ("stackrel" | "overset", over, base) => self.inline_overset(over, base, out),
            ("underset", under, base) => {
                self.inline_simple_stripped(base, out)?;
                self.inline_sub_or_sup(under, out.conf.with_sub(), '_', out)
            }
            // styling and annotations that plain text can't carry
            ("color" | "id" | "class", _, arg) => self.inline_simple_stripped(arg, out),
            (op, first, second) => self.inline_bgeneric(op, first, second, out),
        }
    }

    fn inline_font(
        self,
        font: fn(char) -> char,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        self.inline_simple_stripped(arg, &mut out.with_font(font))
    }

    fn inline_sfunc(
        self,
        open: &str,
        arg: &Simple<'_>,
        close: &str,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(open)?;
        self.inline_simple_stripped(arg, out)?;
        out.write_str(close)
    }

    fn inline_modi(
        self,
        chr: char,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        self.inline_simple_stripped(arg, &mut out.with_modifier(chr))
    }

    /// A `line` goes on every char of a longer argument; a mark can only sit on one, so
    /// anything longer falls back to `op arg`
    fn inline_char_modi(
        self,
        op: &str,
        mark: char,
        line: Option<char>,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        // Try precomposition for single-char arguments (check AST, not rendered output)
        if let &Simple::Ident(text) | &Simple::Number(text) = self.unwrap_single(arg)
            && let Some(base) = only(text.chars())
            && let Some(precomposed) = compose(base, mark)
        {
            out.write_char(precomposed)
        } else if let Some(res) = self.single_char(arg, out) {
            // res is already styled; write to inner so the font isn't re-applied
            out.inner.write_char(res)?;
            out.write_char(mark)
        } else if let Some(line) = line {
            self.inline_modi(line, arg, out)
        } else {
            self.inline_ugeneric(op, arg, out)
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

    pub(crate) fn inline_simpleunary(
        self,
        simple: &SimpleUnary<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match (simple.op, simple.arg()) {
            ("sqrt", arg) => self.inline_root("√", arg, out),
            // fonts
            ("bb" | "mathbf" | "bold", arg) => self.inline_font(bold_map, arg, out),
            ("bbb" | "mathbb", arg) => self.inline_font(double_map, arg, out),
            ("cc" | "mathcal", arg) => self.inline_font(cal_map, arg, out),
            ("tt" | "mathtt", arg) => self.inline_font(mono_map, arg, out),
            ("fr" | "mathfrak", arg) => self.inline_font(frak_map, arg, out),
            ("sf" | "mathsf", arg) => self.inline_font(sans_map, arg, out),
            ("it" | "mathit" | "italic", arg) => self.inline_font(italic_map, arg, out),
            ("bbit", arg) => self.inline_font(|chr| bold_map(italic_map(chr)), arg, out),
            ("bbsf", arg) => self.inline_font(|chr| bold_map(sans_map(chr)), arg, out),
            ("sfit", arg) => self.inline_font(|chr| italic_map(sans_map(chr)), arg, out),
            ("bbsfit", arg) => {
                self.inline_font(|chr| bold_map(italic_map(sans_map(chr))), arg, out)
            }
            ("bbcc", arg) => self.inline_font(|chr| bold_map(cal_map(chr)), arg, out),
            ("bbfr", arg) => self.inline_font(|chr| bold_map(frak_map(chr)), arg, out),
            // functions
            ("abs" | "Abs", arg) => self.inline_sfunc("|", arg, "|", out),
            ("ceil", arg) => self.inline_sfunc("⌈", arg, "⌉", out),
            ("floor", arg) => self.inline_sfunc("⌊", arg, "⌋", out),
            ("norm", arg) => self.inline_sfunc("||", arg, "||", out),
            // any bracket around literal text only delimits it
            ("text" | "mbox", Simple::Group(group)) => self.inline_expression(&group.expr, out),
            ("text" | "mbox", arg) => self.inline_simple(arg, out),
            // braces have no plain-text form; their labels arrive as scripts
            ("ubrace" | "underbrace" | "obrace" | "overbrace", arg) => {
                self.inline_simple_stripped(arg, out)
            }
            // modifiers
            ("overline", arg) => self.inline_modi('\u{0305}', arg, out),
            ("underline" | "ul", arg) => self.inline_modi('\u{0332}', arg, out),
            ("cancel", arg) => self.inline_modi('\u{0336}', arg, out),
            // single character modifiers
            (op @ "hat", arg) => self.inline_char_modi(op, '\u{0302}', None, arg, out),
            (op @ "tilde", arg) => self.inline_char_modi(op, '\u{0303}', None, arg, out),
            // a bar over several chars is an overline
            (op @ "bar", arg) => self.inline_char_modi(op, '\u{0304}', Some('\u{0305}'), arg, out),
            (op @ "dot", arg) => self.inline_char_modi(op, '\u{0307}', None, arg, out),
            (op @ "ddot", arg) => self.inline_char_modi(op, '\u{0308}', None, arg, out),
            (op @ ("overarc" | "overparen"), arg) => {
                self.inline_char_modi(op, '\u{0311}', None, arg, out)
            }
            (op @ "vec", arg) => self.inline_char_modi(op, '\u{20D7}', None, arg, out),
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
                let rules = column_rules(matrix, j);
                if j > 0 && rules == 0 {
                    out.write_char(',')?;
                }
                for _ in 0..rules {
                    out.write_char('|')?;
                }
                self.inline_expression(expr, out)?;
            }
            for _ in 0..column_rules(matrix, row.len()) {
                out.write_char('|')?;
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
            &Simple::Ident(ident) | &Simple::Operator(ident) => out.write_str(ident),
            &Simple::Symbol(symbol) => out.write_str(symbol_str(symbol, self.skin_tone)),
            Simple::Func(func) => self.inline_simplefunc(func, out),
            Simple::Unary(unary) => self.inline_simpleunary(unary, out),
            Simple::Binary(binary) => self.inline_simplebinary(binary, out),
            Simple::Group(group) => self.inline_group(group, out),
            Simple::Matrix(matrix) => self.inline_matrix(matrix, out),
        }
    }

    /// Render a simple, stripping surrounding brackets when `strip_brackets` is on.
    fn inline_simple_stripped(
        self,
        simple: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        if let Some(expr) = self.stripped(simple) {
            self.inline_expression(expr, out)
        } else {
            self.inline_simple(simple, out)
        }
    }

    /// `script` with its brackets stripped and every char mapped through `conf`, if all map
    pub(crate) fn mapped_script(self, script: &Simple<'_>, conf: MapperConf) -> Option<String> {
        let mut text = String::new();
        self.inline_simple_stripped(script, &mut conf.wrap(&mut text))
            .ok()?;
        Some(text)
    }

    /// Write `script` through `conf` when every char maps, otherwise after a literal `marker`
    fn inline_sub_or_sup(
        self,
        script: &Simple<'_>,
        conf: Option<MapperConf>,
        marker: char,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        if let Some(sconf) = conf
            && self
                .inline_simple_stripped(script, &mut sconf.wrap(&mut Sink))
                .is_ok()
        {
            self.inline_simple_stripped(script, &mut sconf.wrap(out.inner))
        } else {
            out.write_char(marker)?;
            self.inline_simple(script, out)
        }
    }

    fn inline_script(self, script: &Script<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        match script {
            Script::None => Ok(()),
            Script::Sub(sub) => self.inline_sub_or_sup(sub, out.conf.with_sub(), '_', out),
            Script::Super(sup) => self.inline_sub_or_sup(sup, out.conf.with_sup(), '^', out),
            Script::Subsuper(sub, sup) => {
                if let Some(sub_conf) = out.conf.with_sub()
                    && self
                        .inline_simple_stripped(sub, &mut sub_conf.wrap(&mut Sink))
                        .is_ok()
                    && let Some(sup_conf) = out.conf.with_sup()
                    && self
                        .inline_simple_stripped(sup, &mut sup_conf.wrap(&mut Sink))
                        .is_ok()
                {
                    self.inline_simple_stripped(sub, &mut sub_conf.wrap(out.inner))?;
                    self.inline_simple_stripped(sup, &mut sup_conf.wrap(out.inner))
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
        if !func_hugs_argument(func) {
            out.write_char(' ')?;
        }
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

    fn inline_scriptfunc_stripped(
        self,
        func: &ScriptFunc<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match func {
            script_func!(simple) => self.inline_simple_stripped(simple, out),
            func => self.inline_scriptfunc(func, out),
        }
    }

    fn inline_sone(self, den: &Simple<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        if let Some(sconf) = out.conf.with_sub()
            && self
                .inline_simple_stripped(den, &mut sconf.wrap(&mut Sink))
                .is_ok()
        {
            out.write_char('⅟')?;
            self.inline_simple_stripped(den, &mut sconf.wrap(out.inner))
        } else {
            Err(fmt::Error)
        }
    }

    pub(crate) fn inline_simplefrac(
        self,
        numer: &Simple<'_>,
        denom: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        if self.vulgar_fracs
            && let Some(frac) = extract_vulgar_frac(numer, denom, self.strip_brackets)
        {
            out.write_char(frac)
        } else if self.vulgar_fracs
            && self.script_fracs
            && matches!(self.unwrap_single(numer), num!("1"))
            && !is_negated(denom)
        {
            self.inline_sone(denom, out)
        } else if self.script_fracs
            && let Some(sup_conf) = out.conf.with_sup()
            && self
                .inline_simple_stripped(numer, &mut sup_conf.wrap(&mut Sink))
                .is_ok()
            && let Some(sub_conf) = out.conf.with_sub()
            && self
                .inline_simple_stripped(denom, &mut sub_conf.wrap(&mut Sink))
                .is_ok()
        {
            self.inline_simple_stripped(numer, &mut sup_conf.wrap(out.inner))?;
            out.write_char('⁄')?;
            self.inline_simple_stripped(denom, &mut sub_conf.wrap(out.inner))
        } else {
            Err(fmt::Error)
        }
    }

    fn inline_fone(self, den: &ScriptFunc<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        if let Some(sconf) = out.conf.with_sub()
            && self
                .inline_scriptfunc(den, &mut sconf.wrap(&mut Sink))
                .is_ok()
        {
            out.write_char('⅟')?;
            self.inline_scriptfunc(den, &mut sconf.wrap(out.inner))
        } else {
            Err(fmt::Error)
        }
    }

    pub(crate) fn inline_frac(
        self,
        frac: &Frac<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match (&frac.numer, &frac.denom) {
            (script_func!(num), script_func!(den)) => {
                self.inline_simplefrac(num, den, out).or_else(|_| {
                    self.inline_simple(num, out)?;
                    out.write_char('/')?;
                    self.inline_simple(den, out)
                })
            }
            (script_func!(num), den)
                if self.script_fracs
                    && self.vulgar_fracs
                    && matches!(self.unwrap_single(num), num!("1"))
                    && !matches!(den, script_func!(den) if is_negated(den)) =>
            {
                self.inline_fone(den, out).or_else(|_| {
                    out.write_str("1/")?;
                    self.inline_scriptfunc(den, out)
                })
            }
            (num, den) => {
                if self.script_fracs
                    && let Some(sup_conf) = out.conf.with_sup()
                    && self
                        .inline_scriptfunc_stripped(num, &mut sup_conf.wrap(&mut Sink))
                        .is_ok()
                    && let Some(sub_conf) = out.conf.with_sub()
                    && self
                        .inline_scriptfunc_stripped(den, &mut sub_conf.wrap(&mut Sink))
                        .is_ok()
                {
                    self.inline_scriptfunc_stripped(num, &mut sup_conf.wrap(out.inner))?;
                    out.write_char('⁄')?;
                    self.inline_scriptfunc_stripped(den, &mut sub_conf.wrap(out.inner))
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
        let mut prev = None;
        for inter in expr.iter() {
            // an item that renders to nothing, like an empty group, must not leave a space behind
            let mut text = String::new();
            self.inline_intermediate(inter, &mut out.onto(&mut text))?;
            if text.is_empty() {
                continue;
            }
            if let Some(prev) = prev
                && needs_space(prev, inter)
            {
                out.write_char(' ')?;
            }
            out.inner.write_str(&text)?;
            prev = Some(inter);
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
        let expected = "∑ᵢ₌₁ⁿ i³=(ⁿ⁽ⁿ⁺¹⁾⁄₂)²";

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

        let res = opts.parse("7/(8)").to_string();
        assert_eq!(res, "⅞");

        let res = opts.parse("{:a:} / (s)").to_string();
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

        let res = opts.parse("y / (x)").to_string();
        assert_eq!(res, "ʸ⁄ₓ");

        let res = opts.parse("(y)/(x)").to_string();
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

        let res = super::super::parse_unicode("dot(x)").to_string();
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

        let res = super::super::parse_unicode("root (4) x").to_string();
        assert_eq!(res, "∜x");

        let res = super::super::parse_unicode("stackrel *** =").to_string();
        assert_eq!(res, "≛");

        let res = super::super::parse_unicode("overset a x").to_string();
        assert_eq!(res, "x\u{0363}");

        let res = super::super::parse_unicode("overset (e) (y)").to_string();
        assert_eq!(res, "y\u{0364}");

        // a multi-char base can't take a combining letter, so it is raised instead
        let res = super::super::parse_unicode("oversetasinx").to_string();
        assert_eq!(res, "sin xᵃ");
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
    fn script_strips_group_brackets() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        // a subscriptable group renders its inner expression without the brackets
        assert_eq!(render("x_(2i)"), "x₂ᵢ");
        assert_eq!(render("x^(2i)"), "x²ⁱ");
        assert_eq!(render("x_(i)^(2j)"), "xᵢ²ʲ");
        // a group that can't be subscripted keeps its brackets to preserve grouping
        assert_eq!(render("x_(A+B)"), "x_(A+B)");

        // --no-strip-brackets leaves the brackets in place
        let conf = Conf {
            strip_brackets: false,
            ..Default::default()
        };
        assert_eq!(conf.parse("x_(2i)").to_string(), "x₍₂ᵢ₎");
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
    fn matrix_column_lines() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("[(a,|,b),(c,|,d)]"), "[[a|b],[c|d]]");
        assert_eq!(render("[(|,a,b,|),(|,c,d,|)]"), "[[|a,b|],[|c,d|]]");
        assert_eq!(render("[(a,|,|,b),(c,|,|,d)]"), "[[a||b],[c||d]]");
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

    #[test]
    fn font_operators() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("bb(x)"), "𝐱");
        assert_eq!(render("mathbf(x)"), "𝐱");
        assert_eq!(render("bbb(R)"), "ℝ");
        assert_eq!(render("mathbb(R)"), "ℝ");
        assert_eq!(render("cc(L)"), "ℒ");
        assert_eq!(render("tt(a)"), "𝚊");
        assert_eq!(render("fr(a)"), "𝔞");
        // `g` is a function token; the fraktur arg must not carry a trailing space
        assert_eq!(render("fr(g)"), "𝔤");
        // fraktur letterlike capitals use dedicated black-letter symbols
        assert_eq!(render("fr(C)"), "ℭ");
        assert_eq!(render("fr(H)"), "ℌ");
        assert_eq!(render("fr(I)"), "ℑ");
        assert_eq!(render("fr(R)"), "ℜ");
        assert_eq!(render("fr(Z)"), "ℨ");
        assert_eq!(render("sf(h)"), "𝗁");
        assert_eq!(render("it(k)"), "𝑘");
        // double-struck n-ary summation has its own mapping
        assert_eq!(render("bbb(sum)"), "⅀");
    }

    #[test]
    fn bare_function_has_no_trailing_space() {
        // `f` and `g` are function tokens; with no argument they render as just the name
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("g"), "g");
        assert_eq!(render("f"), "f");
        assert_eq!(render("g^2"), "g²");
        // a separator is still inserted when an argument follows
        assert_eq!(render("g h"), "g h");
        assert_eq!(render("sin x"), "sin x");
        // a bare function reached through the `Simple::Func` path (as a unary
        // argument or a script base) also omits the trailing separator
        assert_eq!(render("sqrt sin"), "√sin");
        assert_eq!(render("x^sin"), "xˢⁱⁿ");
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn nested_font_not_reapplied() {
        // an inner font wins and must not be re-applied by an outer font when the
        // modifier/cover retry path emits the already-styled character.
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("cc(bb x)"), "\u{1d431}"); // bold x, inner font wins
        assert_eq!(render("hat(bb x)"), "\u{1d431}\u{0302}"); // bold x + combining hat
        // char-modifier retry: outer cal must not re-map the bold x
        assert_eq!(render("cc(hat(bb x))"), "\u{1d431}\u{0302}");
        // cover retry: same, with an overset combining mark
        assert_eq!(render("cc(overset(a)(bb x))"), "\u{1d431}\u{0363}");
    }

    #[test]
    fn explicit_frac_binary_falls_back() {
        // `frac(x)(y)` cannot script-render, but must not error/panic — it falls back to `/`
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("frac(x)(y)"), "(x)/(y)");
        // vulgar/script forms still apply when possible
        assert_eq!(render("frac(1)(2)"), "½");
    }

    #[test]
    fn delimiter_functions() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("abs(x)"), "|x|");
        assert_eq!(render("ceil(x)"), "⌈x⌉");
        assert_eq!(render("floor(x)"), "⌊x⌋");
        assert_eq!(render("norm(v)"), "||v||");
        // text/mbox renders its content with no delimiters
        assert_eq!(render("text(hi)"), "hi");
    }

    #[test]
    fn overline_underline_cancel() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("overline(x)"), "x\u{0305}");
        assert_eq!(render("underline(x)"), "x\u{0332}");
        assert_eq!(render("cancel(x)"), "x\u{0336}");
    }

    #[test]
    fn script_frac_grouped_numerator() {
        // (x+1)/2 with script fracs: grouped numerator superscripted over subscript
        let res = super::super::parse_unicode("(x+1)/2").to_string();
        assert_eq!(res, "ˣ⁺¹⁄₂");
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn overset_combining_marks() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        // each latin-letter argument to overset/stackrel maps to a combining mark
        assert_eq!(render("overset(a)(N)"), "N\u{0363}");
        assert_eq!(render("overset(e)(N)"), "N\u{0364}");
        assert_eq!(render("overset(i)(N)"), "N\u{0365}");
        assert_eq!(render("overset(o)(N)"), "N\u{0366}");
        assert_eq!(render("overset(u)(N)"), "N\u{0367}");
        assert_eq!(render("overset(c)(N)"), "N\u{0368}");
        assert_eq!(render("overset(d)(N)"), "N\u{0369}");
        assert_eq!(render("overset(h)(N)"), "N\u{036a}");
        assert_eq!(render("overset(m)(N)"), "N\u{036b}");
        assert_eq!(render("overset(r)(N)"), "N\u{036c}");
        assert_eq!(render("stackrel(t)(N)"), "N\u{036d}");
        assert_eq!(render("stackrel(v)(N)"), "N\u{036e}");
        assert_eq!(render("stackrel(x)(N)"), "N\u{036f}");
    }

    #[test]
    fn roots() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("root(2)(x)"), "√x");
        assert_eq!(render("root(3)(x)"), "∛x");
        assert_eq!(render("root(4)(x)"), "∜x");
        // other indices are raised in front of the radical
        assert_eq!(render("root(5)(x)"), "⁵√x");
        assert_eq!(render("root(n)(x+1)"), "ⁿ√(x+1)");
        // an index without a superscript form falls back to the command
        assert_eq!(render("root(Q)(x)"), "root (Q) (x)");
        // parentheses around a lone radicand are dropped
        assert_eq!(render("sqrt(x)"), "√x");
        assert_eq!(render("sqrt(2x)"), "√(2x)");
    }

    #[test]
    fn annotation_commands() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        // only the annotated expression is shown
        assert_eq!(render("color(red)(x)"), "x");
        assert_eq!(render("id(a)(x)"), "x");
        assert_eq!(render("class(b)(x+1)"), "x+1");
        assert_eq!(render("ubrace(x+y)"), "x+y");
        assert_eq!(render("underbrace(x+y)"), "x+y");
        assert_eq!(render("obrace(x+y)"), "x+y");
        assert_eq!(render("overbrace(x+y)"), "x+y");
    }

    #[test]
    fn over_and_under() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("underset(x)(lim)"), "limₓ");
        assert_eq!(render("underset(x->0)(lim) f(x)"), "lim_(x→0) f(x)");
        assert_eq!(render("stackrel(def)(=)"), "≝");
        assert_eq!(render("overset(?)(=)"), "≟");
        assert_eq!(render("overset(abc)(X)"), "Xᵃᵇᶜ");
        assert_eq!(render("stackrel(->)(=)"), "=^(→)");
    }

    #[test]
    fn primes_and_script_commas() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("x^'"), "x′");
        assert_eq!(render("f''(x)"), "f′′(x)");
        assert_eq!(render("x_(i,j)"), "xᵢ,ⱼ");
        assert_eq!(render("x^(i,j)"), "xⁱ,ʲ");
    }

    #[test]
    fn script_frac_grouped_paths() {
        // numerator superscriptable, denominator subscriptable: every grouped branch
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("(x+a)/(x+a)"), "ˣ⁺ᵃ⁄ₓ₊ₐ"); // (group, group)
        assert_eq!(render("x/(x+a)"), "ˣ⁄ₓ₊ₐ"); // (simple, group)
        assert_eq!(render("(x+a)/x"), "ˣ⁺ᵃ⁄ₓ"); // (group, simple)
        assert_eq!(render("1/(x+a)"), "⅟ₓ₊ₐ"); // one-over reciprocal, grouped
        assert_eq!(render("(1)/(x+a)"), "⅟ₓ₊ₐ"); // grouped `1` numerator
        assert_eq!(render("1/y"), "1/y"); // one-over reciprocal, non-subscriptable
    }

    #[test]
    fn script_frac_non_subscriptable_fallbacks() {
        // capitals/symbols have no sub/superscript form, so these fall back to plain `/`
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("(A+B)/(C+D)"), "(A+B)/(C+D)");
        assert_eq!(render("x/(A B)"), "x/(AB)");
        assert_eq!(render("(A B)/y"), "(AB)/y");
        assert_eq!(render("1/(A B)"), "1/(AB)");
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn char_modifier_group_fallbacks() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        // single symbol in a group: no precomposed form, modifier combines anyway
        assert_eq!(render("hat(+)"), "+\u{0302}");
        assert_eq!(render("dot(+)"), "+\u{0307}");
        // one accent can't span several chars, so the command stays literal
        assert_eq!(render("hat(x y)"), "hat (xy)");
        assert_eq!(render("vec(AB)"), "vec (AB)");
        assert_eq!(render("dot(ab)"), "dot (ab)");
        assert_eq!(render("ddot(ab)"), "ddot (ab)");
        assert_eq!(render("tilde(ab)"), "tilde (ab)");
        assert_eq!(render("hat|x|"), "hat |x|");
        // lines join up, so they go on every char but brackets
        assert_eq!(render("ul(ab)"), "a\u{0332}b\u{0332}");
        assert_eq!(render("bar(xy)"), "x\u{0305}y\u{0305}");
        assert_eq!(render("bar|xy|"), "|x\u{0305}y\u{0305}|");
        assert_eq!(render("overline(a+b)"), "a\u{0305}+\u{0305}b\u{0305}");
    }

    #[test]
    fn account_of_and_letter_vulgar_fracs() {
        // ast::vulgar_frac_char's non-digit entries
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("a/c"), "℀");
        assert_eq!(render("a/s"), "℁");
        assert_eq!(render("A/S"), "⅍");
        assert_eq!(render("c/o"), "℅");
        assert_eq!(render("c/u"), "℆");
        assert_eq!(render("0/3"), "↉");
    }

    #[test]
    fn superscript_uppercase_and_greek() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        // every uppercase letter with a superscript form
        assert_eq!(render("x^(DEGHIJKLMNOPRTUVW)"), "xᴰᴱᴳᴴᴵᴶᴷᴸᴹᴺᴼᴾᴿᵀᵁⱽᵂ");
        // greek letters with a superscript form
        assert_eq!(
            render("x^(alpha beta gamma delta epsilon theta iota phi varphi chi)"),
            "xᵅᵝᵞᵟᵋᶿᶥᵠᶲᵡ"
        );
    }

    #[test]
    fn subscript_greek() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("x_beta"), "xᵦ");
        assert_eq!(render("x_gamma"), "xᵧ");
        assert_eq!(render("x_rho"), "xᵨ");
        assert_eq!(render("x_phi"), "xᵩ");
        assert_eq!(render("x_chi"), "xᵪ");
    }

    #[test]
    fn frac_level_arms_with_scripts() {
        // when an operand carries a script the frac uses the `Frac`-level arms,
        // which fall back to plain `/` when the script blocks sub/superscripting
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("1/x^2"), "1/x²"); // one-over reciprocal
        assert_eq!(render("(1)/x^2"), "1/x²"); // grouped one-over
        assert_eq!(render("(x+a)/y^2"), "(x+a)/y²"); // grouped numerator
        assert_eq!(render("x^2/(x+a)"), "x²/(x+a)"); // grouped denominator
    }

    #[test]
    fn modifier_on_grouped_single_symbol() {
        // a group whose only element is a symbol is not a precomposable char
        let res = super::super::parse_unicode("hat((+))").to_string();
        assert_eq!(res, "hat ((+))");
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn modifier_on_bare_symbol() {
        // a bare (ungrouped) symbol argument takes the combining modifier directly
        let res = super::super::parse_unicode("vec*").to_string();
        assert_eq!(res, "\u{22c5}\u{20d7}");
    }

    #[test]
    fn word_spacing() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        // a function touches a bracketed argument and is spaced from a bare one
        assert_eq!(render("sin(x)"), "sin(x)");
        assert_eq!(render("f(x)"), "f(x)");
        assert_eq!(render("f'(x)"), "f′(x)");
        assert_eq!(render("sin^2(x)"), "sin²(x)");
        assert_eq!(render("sin x"), "sin x");
        assert_eq!(render("log_2 x"), "log₂ x");
        assert_eq!(render("min(a,b)"), "min(a,b)");
        // words stay separated from neighboring operands
        assert_eq!(render("a mod b"), "a mod b");
        assert_eq!(render("a and b"), "a and b");
        assert_eq!(render("x>0 or x<1"), "x>0 or x<1");
        assert_eq!(render("2 sin x"), "2 sin x");
        assert_eq!(render("sin x cos x"), "sin x cos x");
        assert_eq!(render("x max(a,b)"), "x max(a,b)");
        assert_eq!(render("x dx"), "x dx");
        // a big operator with scripts is separated from its operand; a bare one is not
        assert_eq!(render("int_0^1 f(x) dx"), "∫₀¹ f(x) dx");
        assert_eq!(render("lim_(x->0) f(x)"), "lim_(x→0) f(x)");
        assert_eq!(render("sum_i x_i"), "∑ᵢ xᵢ");
        assert_eq!(render("sum x_i"), "∑xᵢ");
        assert_eq!(render("int f dx"), "∫f dx");
        // an item that renders to nothing leaves no space behind
        assert_eq!(render("dx {: :} dy"), "dx dy");
        assert_eq!(render("dx \"\" dy"), "dx dy");
        // operators stay tight
        assert_eq!(render("x = -1"), "x=-1");
        assert_eq!(render("f(x)=x^2"), "f(x)=x²");
        assert_eq!(render("a b"), "ab");
    }

    #[test]
    fn literal_text() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("text(hello world)"), "hello world");
        assert_eq!(render("mbox[a + b]"), "a + b");
        assert_eq!(render("text{ x }"), " x ");
        assert_eq!(render("text(a) + 1"), "a+1");
        let conf = Conf {
            strip_brackets: false,
            ..Default::default()
        };
        assert_eq!(conf.parse("text(hello world)").to_string(), "hello world");
    }

    #[test]
    fn prefix_minus_in_scripts() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("x^-1"), "x⁻¹");
        assert_eq!(render("e^-x"), "e⁻ˣ");
        assert_eq!(render("x_-1^-2"), "x₋₁⁻²");
        assert_eq!(render("sin^-1 x"), "sin⁻¹ x");
        // a negative denominator is an ordinary fraction, not a reciprocal
        assert_eq!(render("1/-2"), "¹⁄₋₂");
        assert_eq!(render("1/-x"), "¹⁄₋ₓ");
    }

    #[test]
    fn added_symbols() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("hbar o- dag dagger ddag ddagger"), "ℏ⊖††‡‡");
        assert_eq!(render("a notequiv b !-= c"), "a≢b≢c");
        assert_eq!(render("a rightleftharpoons b"), "a⇌b");
        assert_eq!(
            render("a notsubset b notsubseteq c notsupset d notsupseteq e"),
            "a⊄b⊈c⊅d⊉e"
        );
        assert_eq!(render("a enspace b thinspace c"), "a\u{2002}b\u{2009}c");
        assert_eq!(render("arcsec x"), "arcsec x");
        assert_eq!(render("arccsc(x)"), "arccsc(x)");
        assert_eq!(render("arccot x"), "arccot x");
    }

    #[test]
    fn added_fonts() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("italic(x)"), "𝑥");
        assert_eq!(render("bold(x)"), "𝐱");
        assert_eq!(render("bbit(x)"), "𝒙");
        assert_eq!(render("bbsf(x)"), "𝘅");
        assert_eq!(render("sfit(x)"), "𝘹");
        assert_eq!(render("bbsfit(x)"), "𝙭");
        assert_eq!(render("bbcc(A)"), "𝓐");
        assert_eq!(render("bbfr(A)"), "𝕬");
    }

    #[test]
    fn only_grouping_brackets_are_stripped() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        // bars, angles, floors and ceilings carry meaning
        assert_eq!(render("|x|/2"), "|x|/2");
        assert_eq!(render("<<x>>/2"), "⟨x⟩/2");
        assert_eq!(render("x_|a|"), "x_|a|");
        assert_eq!(render("bb|x|"), "|𝐱|");
        assert_eq!(render("|__x__|/2"), "⌊x⌋/2");
        assert_eq!(render("|~x~|/2"), "⌈x⌉/2");
        assert_eq!(render("hat|x|"), "hat |x|");
        // parentheses, square and curly brackets only group
        assert_eq!(render("x_(a)"), "xₐ");
        assert_eq!(render("x_[a]"), "xₐ");
        assert_eq!(render("x_{:a:}"), "xₐ");
        assert_eq!(render("bb(x)"), "𝐱");
        assert_eq!(render("bb{x}"), "𝐱");
        assert_eq!(render("7/[8]"), "⅞");
        assert_eq!(render("{a} / (s)"), "℁");
        assert_eq!(render("dot{x}"), "ẋ");
        assert_eq!(render("root {4} x"), "∜x");
    }

    #[test]
    fn empty_trailing_superscript() {
        // `x^` leaves the script base `Simple::Missing`, which renders to nothing
        let res = super::super::parse_unicode("x^").to_string();
        assert_eq!(res, "x");
    }
}
