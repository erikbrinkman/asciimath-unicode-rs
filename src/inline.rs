#![allow(missing_docs, clippy::missing_errors_doc)]

use asciimath_parser::tree::{
    Expression, Func, Group, Intermediate, Matrix, Script, ScriptFunc, Simple, SimpleBinary,
    SimpleFunc, SimpleScript, SimpleUnary, Symbol,
};
use std::fmt;
use std::fmt::Write;
use unicode_normalization::char::{compose, is_combining_mark};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::ast::{
    delimiters, extract_vulgar_frac, frac_edges, func_hugs_argument, hugs_argument,
    inter_is_spaced_op, is_empty_grouping, name_without_argument, needs_space, paren_contents,
    scriptfunc_edges, unwrap_parens,
};
use super::tokens::{
    bold_map, cal_map, double_map, frak_map, is_bracket_char, italic_map, left_bracket_str,
    mono_map, right_bracket_str, sans_map, subscript_char, superscript_char, symbol_str,
};
use super::{Conf, Layout};

/// The combining mark drawing a line over every char
const OVERLINE: char = '\u{0305}';
/// The combining mark drawing a line under every char
const UNDERLINE: char = '\u{0332}';
/// The combining mark drawing a line through the middle of every char
const CANCEL: char = '\u{0336}';
/// The combining marks that draw a line through every char
const LINE_MARKS: [char; 3] = [OVERLINE, UNDERLINE, CANCEL];

/// A style a font command gives each char
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// bold
    Bold,
    /// italic
    Italic,
    /// calligraphic script
    Cal,
    /// fraktur
    Frak,
    /// double-struck
    Double,
    /// sans-serif
    Sans,
    /// monospace
    Mono,
}

impl Style {
    /// `chr` in this style, keeping the styles it already has where a char combines them
    fn apply(self, chr: char) -> char {
        match self {
            Style::Bold => bold_map(chr),
            Style::Italic => italic_map(chr),
            Style::Cal => cal_map(chr),
            Style::Frak => frak_map(chr),
            Style::Double => double_map(chr),
            Style::Sans => sans_map(chr),
            Style::Mono => mono_map(chr),
        }
    }
}

/// The styles of the fonts a char is in, outermost first and each at most once
#[derive(Debug, Default, Clone, Copy)]
pub struct Font([Option<Style>; 7]);

impl Font {
    /// This font inside the font of `styles`, which are given outermost first; a style applied
    /// again changes nothing, so one already here is left out
    pub fn within(self, styles: &[Style]) -> Self {
        let Font(mut slots) = self;
        for &style in styles {
            if !slots.contains(&Some(style))
                && let Some(slot) = slots.iter_mut().find(|slot| slot.is_none())
            {
                *slot = Some(style);
            }
        }
        Font(slots)
    }

    fn is_plain(self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    /// `chr` in this font, the innermost style applied first
    pub fn apply(self, chr: char) -> char {
        self.0
            .into_iter()
            .rev()
            .flatten()
            .fold(chr, |styled, style| style.apply(styled))
    }

    /// Whether this font leaves every char of `text` as it is
    pub fn leaves(self, text: &str) -> bool {
        text.chars().all(|chr| self.apply(chr) == chr)
    }
}

/// The lines drawn through every char, each at most once, in the order they were applied
#[derive(Debug, Default, Clone, Copy)]
pub struct Lines([Option<char>; LINE_MARKS.len()]);

impl Lines {
    /// These lines and `mark`
    fn with(self, mark: char) -> Self {
        debug_assert!(LINE_MARKS.contains(&mark), "{mark:?} is not a line mark");
        let Lines(mut marks) = self;
        if !marks.contains(&Some(mark))
            && let Some(slot) = marks.iter_mut().find(|slot| slot.is_none())
        {
            *slot = Some(mark);
        }
        Lines(marks)
    }

    fn is_empty(self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    /// The marks to write after a char, the innermost one nearest it
    fn marks(self) -> impl Iterator<Item = char> {
        self.0.into_iter().rev().flatten()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MapperConf {
    pub font: Font,
    pub sub_sup: Option<fn(char) -> Option<char>>,
    pub lines: Lines,
}

impl MapperConf {
    /// This method allows checking if we can apply `sub_sup` without borrowing the inner writer
    pub fn with_sub_sup(&self, sub_sup: fn(char) -> Option<char>) -> Option<MapperConf> {
        if self.sub_sup.is_none() {
            Some(MapperConf {
                font: self.font,
                sub_sup: Some(sub_sup),
                lines: self.lines,
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

    /// `chr` styled and raised or lowered, or an error if it has no raised or lowered form
    fn map_char(self, chr: char) -> Result<char, fmt::Error> {
        let styled = self.font.apply(chr);
        match self.sub_sup {
            Some(script) => script(styled).ok_or(fmt::Error),
            None => Ok(styled),
        }
    }

    /// This configuration without its lines
    fn unlined(self) -> MapperConf {
        MapperConf {
            lines: Lines::default(),
            ..self
        }
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

    /// This mapper styling each char in the font of `styles`, given outermost first, before the
    /// font it already has
    pub fn with_font(&mut self, styles: &[Style]) -> Mapper<'_, W> {
        Mapper {
            inner: &mut *self.inner,
            conf: MapperConf {
                font: self.conf.font.within(styles),
                sub_sup: self.conf.sub_sup,
                lines: self.conf.lines,
            },
        }
    }

    pub fn with_modifier(&mut self, c: char) -> Mapper<'_, W> {
        Mapper {
            inner: &mut *self.inner,
            conf: MapperConf {
                font: self.conf.font,
                sub_sup: self.conf.sub_sup,
                lines: self.conf.lines.with(c),
            },
        }
    }

    /// `chr` the way this mapper would write it, if it maps at all
    pub fn mapped_char(&self, chr: char) -> Option<String> {
        let mut text = String::new();
        self.onto(&mut text).write_char(chr).ok()?;
        Some(text)
    }

    /// `text` the way this mapper would write it, if every character of it maps
    pub fn mapped_str(&self, text: &str) -> Option<String> {
        let mut mapped = String::new();
        self.onto(&mut mapped).write_str(text).ok()?;
        Some(mapped)
    }

    pub fn onto<'b, S: Write>(&self, other: &'b mut S) -> Mapper<'b, S> {
        Mapper {
            inner: other,
            conf: self.conf,
        }
    }

    /// Write `text`, already styled and raised or lowered, with this mapper's lines through each
    /// char that takes them
    fn write_lined(&mut self, text: &str) -> fmt::Result {
        if self.conf.lines.is_empty() {
            self.inner.write_str(text)
        } else {
            for cluster in text.graphemes(true) {
                self.inner.write_str(cluster)?;
                // a line through brackets would suggest they're part of the marked text
                if cluster.chars().next().is_some_and(|base| {
                    !base.is_whitespace() && !is_bracket_char(base) && !is_combining_mark(base)
                }) {
                    for line in self.conf.lines.marks() {
                        if !cluster.contains(line) {
                            self.inner.write_char(line)?;
                        }
                    }
                }
            }
            Ok(())
        }
    }

    /// Write `base`, one char already styled and raised or lowered with only marks after it, with
    /// `mark` over it and this mapper's lines through it
    fn write_accented(&mut self, base: &str, mark: char) -> fmt::Result {
        let mut accented = String::new();
        if let Some(composed) = only(base.chars()).and_then(|chr| compose(chr, mark)) {
            accented.push(composed);
        } else {
            accented.push_str(base);
            accented.push(mark);
        }
        self.write_lined(&accented)
    }
}

impl<W: fmt::Write + ?Sized> fmt::Write for Mapper<'_, W> {
    /// Only a mapper over a string is asked whether a rendering maps, so a char with no script
    /// form failing here never looks like a real writer's error
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.conf.font.is_plain() && self.conf.sub_sup.is_none() {
            self.write_lined(s)
        } else if self.conf.lines.is_empty() {
            for chr in s.chars() {
                self.inner.write_char(self.conf.map_char(chr)?)?;
            }
            Ok(())
        } else {
            // lines go after each char a reader sees, so they need the whole mapped text
            let mapped = s
                .chars()
                .map(|chr| self.conf.map_char(chr))
                .collect::<Result<String, _>>()?;
            self.write_lined(&mapped)
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
        Simple::Symbol(Symbol { text: $sym, .. })
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

/// Whether `text` is what a reader sees as one char that isn't an emoji, so a mark can sit on it
fn takes_accent(text: &str) -> bool {
    only(text.graphemes(true)).is_some_and(|cluster| !is_emoji(cluster))
}

/// Whether any char a reader sees in `text` is an emoji, which no mark is drawn over
fn has_emoji(text: &str) -> bool {
    text.graphemes(true).any(is_emoji)
}

/// Whether `cluster`, one char as a reader sees it, is drawn as an emoji
fn is_emoji(cluster: &str) -> bool {
    // a lone char that needs U+FE0F to be an emoji, like `↔` or `❤`, is drawn as text without it
    cluster
        .chars()
        .any(|chr| ('\u{1f1e6}'..='\u{1f1ff}').contains(&chr))
        || emojis::get(cluster)
            .is_some_and(|emoji| emoji.as_str() == cluster || cluster.chars().nth(1).is_some())
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

/// Whether `expr` is nothing but a minus prefixing the operand after it
fn is_negation(expr: &Expression<'_>) -> bool {
    only(
        expr.iter()
            .filter(|inter| !matches!(inter, Intermediate::Space(_))),
    )
    .is_some_and(|inter| matches!(inter, Intermediate::ScriptFunc(func) if func.is_negated()))
}

/// How many vertical lines the matrix draws at a column boundary
pub(crate) fn column_rules(matrix: &Matrix<'_>, boundary: usize) -> usize {
    matrix
        .column_lines()
        .iter()
        .filter(|&&line| line == boundary)
        .count()
}

/// The styles of the font command `op`, outermost first, if it is one
pub(crate) fn font_styles(op: &str) -> Option<&'static [Style]> {
    match op {
        "bb" | "mathbf" | "bold" => Some(&[Style::Bold]),
        "bbb" | "mathbb" => Some(&[Style::Double]),
        "cc" | "mathcal" => Some(&[Style::Cal]),
        "tt" | "mathtt" => Some(&[Style::Mono]),
        "fr" | "mathfrak" => Some(&[Style::Frak]),
        "sf" | "mathsf" => Some(&[Style::Sans]),
        "it" | "mathit" | "italic" => Some(&[Style::Italic]),
        "bbit" => Some(&[Style::Bold, Style::Italic]),
        "bbsf" => Some(&[Style::Bold, Style::Sans]),
        "sfit" => Some(&[Style::Italic, Style::Sans]),
        "bbsfit" => Some(&[Style::Bold, Style::Italic, Style::Sans]),
        "bbcc" => Some(&[Style::Bold, Style::Cal]),
        "bbfr" => Some(&[Style::Bold, Style::Frak]),
        _ => None,
    }
}

/// Whether `op` puts a brace over or under its argument, whose labels arrive as scripts
pub(crate) fn is_brace(op: &str) -> bool {
    matches!(op, "ubrace" | "underbrace" | "obrace" | "overbrace")
}

pub(crate) fn root_char(index: &Simple<'_>) -> Option<char> {
    match index {
        num!("2") => Some('√'),
        num!("3") => Some('∛'),
        num!("4") => Some('∜'),
        _ => None,
    }
}

/// One side of a fraction: a simple, possibly with a script or a function applied
pub(crate) trait Operand<'a> {
    /// The operand as a simple, if it carries no script
    fn as_simple(&self) -> Option<&Simple<'a>>;

    /// Whether the operand is a minus prefixing something, which reads badly after `⅟`
    fn is_negated(&self) -> bool;

    /// The sign this operand prefixes, and the operand it prefixes it to
    fn peel_sign(&self) -> Option<(&'a str, &Self)>;

    /// Render the operand with its grouping brackets dropped
    fn inline_stripped<W: fmt::Write>(&self, conf: Conf, out: &mut Mapper<'_, W>) -> fmt::Result;

    /// Render the operand as written, brackets and all
    fn inline_as_written<W: fmt::Write>(&self, conf: Conf, out: &mut Mapper<'_, W>) -> fmt::Result;
}

impl<'a> Operand<'a> for Simple<'a> {
    fn as_simple(&self) -> Option<&Simple<'a>> {
        Some(self)
    }

    fn is_negated(&self) -> bool {
        match self {
            Simple::Signed(signed) => signed.sign == "-" && signed.operand().as_option().is_some(),
            simple => paren_contents(simple).is_some_and(is_negation),
        }
    }

    fn peel_sign(&self) -> Option<(&'a str, &Self)> {
        match self {
            Simple::Signed(signed) => Some((signed.sign, signed.operand())),
            _ => None,
        }
    }

    fn inline_stripped<W: fmt::Write>(&self, conf: Conf, out: &mut Mapper<'_, W>) -> fmt::Result {
        conf.inline_simple_stripped(self, out)
    }

    fn inline_as_written<W: fmt::Write>(&self, conf: Conf, out: &mut Mapper<'_, W>) -> fmt::Result {
        conf.inline_simple(self, out)
    }
}

impl<'a> Operand<'a> for ScriptFunc<'a> {
    fn as_simple(&self) -> Option<&Simple<'a>> {
        match self {
            script_func!(simple) => Some(simple),
            _ => None,
        }
    }

    fn is_negated(&self) -> bool {
        match self {
            ScriptFunc::Signed(signed) => {
                signed.sign == "-" && !matches!(signed.operand().as_simple(), Some(Simple::Missing))
            }
            func => func.as_simple().is_some_and(Operand::is_negated),
        }
    }

    fn peel_sign(&self) -> Option<(&'a str, &Self)> {
        match self {
            ScriptFunc::Signed(signed) => Some((signed.sign, signed.operand())),
            _ => None,
        }
    }

    fn inline_stripped<W: fmt::Write>(&self, conf: Conf, out: &mut Mapper<'_, W>) -> fmt::Result {
        conf.inline_scriptfunc_stripped(self, out)
    }

    fn inline_as_written<W: fmt::Write>(&self, conf: Conf, out: &mut Mapper<'_, W>) -> fmt::Result {
        conf.inline_scriptfunc(self, out)
    }
}

impl Conf {
    pub(crate) fn stripped<'s, 'a>(self, simple: &'s Simple<'a>) -> Option<&'s Expression<'a>> {
        if self.strip_brackets {
            // a placeholder stands in for an empty group, brackets and all
            paren_contents(simple).filter(|expr| self.placeholders.is_none() || !expr.is_empty())
        } else {
            None
        }
    }

    /// Whether a part of the math isn't there yet; a group with nothing in it is one being
    /// typed, as long as its brackets were going to be dropped
    fn is_missing(self, simple: &Simple<'_>) -> bool {
        matches!(simple, Simple::Missing) || (self.strip_brackets && is_empty_grouping(simple))
    }

    /// What stands in for `simple` when it is a part that isn't there yet
    pub(crate) fn placeholder(self, simple: &Simple<'_>) -> Option<char> {
        match self.placeholders {
            Some(marks) if self.is_missing(simple) => Some(marks.char),
            _ => None,
        }
    }

    /// What stands in for `expr` when it has nothing in it, so a part is being typed there
    pub(crate) fn empty_placeholder(self, expr: &Expression<'_>) -> Option<char> {
        match self.placeholders {
            Some(marks) if expr.is_empty() => Some(marks.char),
            _ => None,
        }
    }

    /// What stands in for `group`'s contents when it was opened with nothing in it and its
    /// brackets stay
    pub(crate) fn group_placeholder(self, group: &Group<'_>) -> Option<char> {
        if group.left_bracket.is_empty() {
            None
        } else {
            self.empty_placeholder(&group.expr)
        }
    }

    /// What stands in for a subscript that isn't there yet
    pub(crate) fn sub_placeholder(self) -> Option<char> {
        self.placeholders.map(|marks| marks.sub)
    }

    /// What stands in for a superscript that isn't there yet
    pub(crate) fn sup_placeholder(self) -> Option<char> {
        self.placeholders.map(|marks| marks.sup)
    }

    pub(crate) fn unwrap_single<'s, 'a>(self, simple: &'s Simple<'a>) -> &'s Simple<'a> {
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
        if name_without_argument(simple.arg()) {
            Ok(())
        } else {
            self.inline_applied_arg(simple.arg(), hugs_argument(simple.arg()), out)
        }
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
            && let Some(text) = self.mapped_operand(index, sup_conf)
        {
            out.inner.write_str(&text)?;
            self.inline_root("√", radicand, out)
        } else {
            self.inline_bgeneric("root", index, radicand, out)
        }
    }

    /// `simple` rendered without its grouping brackets or `out`'s lines, but in its font and script
    fn render_unlined(
        self,
        simple: &Simple<'_>,
        out: &Mapper<impl fmt::Write>,
    ) -> Result<String, fmt::Error> {
        self.mapped_operand(simple, out.conf.unlined())
            .ok_or(fmt::Error)
    }

    /// Write `simple` as it was typed, brackets and all, from its
    /// [`render_unlined`][Conf::render_unlined] rendering
    fn write_as_written(
        self,
        simple: &Simple<'_>,
        unlined: &str,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        if let Simple::Group(group) = simple
            && self.stripped(simple).is_some()
        {
            out.write_str(left_bracket_str(group.left_bracket))?;
            out.write_lined(unlined)?;
            out.write_str(right_bracket_str(group.right_bracket))
        } else {
            out.write_lined(unlined)
        }
    }

    /// The relation symbol for `=` with `over`, rendered as `text`, stacked above it
    fn equals_char(text: &str) -> Option<char> {
        match text {
            "∘" => Some('\u{2257}'),
            "⋆" => Some('\u{225b}'),
            "△" => Some('\u{225c}'),
            "def" => Some('\u{225d}'),
            "m" => Some('\u{225e}'),
            "?" => Some('\u{225f}'),
            _ => None,
        }
    }

    /// Write a name's argument after it
    fn inline_applied_arg<'a>(
        self,
        arg: &impl Operand<'a>,
        hugs: bool,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let mut text = String::new();
        arg.inline_as_written(self, &mut out.onto(&mut text))?;
        Self::write_applied_arg(&text, hugs, out)
    }

    /// Write a name's argument, already written as `text`, after it; a space separates them
    /// unless the argument hugs the name or renders to nothing
    fn write_applied_arg(text: &str, hugs: bool, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        if !hugs && !text.is_empty() {
            out.write_char(' ')?;
        }
        out.inner.write_str(text)
    }

    fn inline_bgeneric(
        self,
        op: &str,
        first: &Simple<'_>,
        second: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(op)?;
        self.inline_applied_arg(first, false, out)?;
        self.inline_applied_arg(second, false, out)
    }

    fn inline_overset(
        self,
        over: &Simple<'_>,
        base: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let mark = match self.unwrap_single(over) {
            iden!(letter) => combining_letter(letter),
            _ => None,
        };
        if let Some(mark) = mark {
            let text = self.render_unlined(base, out)?;
            if takes_accent(&text) {
                out.write_accented(&text, mark)
            } else {
                out.write_lined(&text)?;
                self.inline_sub_or_sup(over, out.conf.with_sup(), '^', self.sup_placeholder(), out)
            }
        } else if matches!(self.unwrap_single(base), symb!("=")) {
            let text = self.render_unlined(over, out)?;
            if let Some(relation) = Self::equals_char(&text) {
                out.write_char(relation)
            } else {
                self.inline_simple_stripped(base, out)?;
                if let Some(raised) =
                    self.mapped_script(over, out.conf.with_sup(), self.sup_placeholder())
                {
                    out.inner.write_str(&raised)
                } else {
                    out.write_char('^')?;
                    self.write_as_written(over, &text, out)
                }
            }
        } else {
            self.inline_simple_stripped(base, out)?;
            self.inline_sub_or_sup(over, out.conf.with_sup(), '^', self.sup_placeholder(), out)
        }
    }

    pub(crate) fn inline_simplebinary(
        self,
        simple: &SimpleBinary<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match (simple.op, simple.first(), simple.second()) {
            ("root", index, radicand) => self.inline_nroot(index, radicand, out),
            ("frac", numer, denom) => {
                let compact = self.mapped_frac(numer, denom, out);
                if let Some(text) = compact {
                    out.inner.write_str(&text)
                } else {
                    self.inline_plain_frac(numer, denom, out)
                }
            }
            ("stackrel" | "overset", over, base) => self.inline_overset(over, base, out),
            ("underset", under, base) => {
                self.inline_simple_stripped(base, out)?;
                self.inline_sub_or_sup(under, out.conf.with_sub(), '_', self.sub_placeholder(), out)
            }
            // styling and annotations that plain text can't carry
            ("color" | "id" | "class", _, arg) => self.inline_simple_stripped(arg, out),
            (op, first, second) => self.inline_bgeneric(op, first, second, out),
        }
    }

    fn inline_font(
        self,
        styles: &[Style],
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        self.inline_simple_stripped(arg, &mut out.with_font(styles))
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

    /// No line is drawn through an emoji, so an argument with one falls back to `op arg`
    fn inline_modi(
        self,
        op: &str,
        line: char,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        let text = self.render_unlined(arg, out)?;
        if has_emoji(&text) {
            self.write_command(op, arg, &text, out)
        } else {
            out.with_modifier(line).write_lined(&text)
        }
    }

    /// Write `op` and the argument `arg`, rendered as `text`, as they were typed
    fn write_command(
        self,
        op: &str,
        arg: &Simple<'_>,
        text: &str,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(op)?;
        let mut written = String::new();
        self.write_as_written(arg, text, &mut out.onto(&mut written))?;
        Self::write_applied_arg(&written, false, out)
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
        let text = self.render_unlined(arg, out)?;
        if takes_accent(&text) {
            out.write_accented(&text, mark)
        } else if let Some(line) = line
            && !has_emoji(&text)
        {
            out.with_modifier(line).write_lined(&text)
        } else {
            self.write_command(op, arg, &text, out)
        }
    }

    fn inline_ugeneric(
        self,
        op: &str,
        arg: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        out.write_str(op)?;
        self.inline_applied_arg(arg, false, out)
    }

    pub(crate) fn inline_simpleunary(
        self,
        simple: &SimpleUnary<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        if let Some((open, close)) = delimiters(simple.op) {
            self.inline_sfunc(open, simple.arg(), close, out)
        } else if let Some(styles) = font_styles(simple.op) {
            self.inline_font(styles, simple.arg(), out)
        } else {
            match (simple.op, simple.arg()) {
                ("sqrt", arg) => self.inline_root("√", arg, out),
                // any bracket around literal text only delimits it
                ("text" | "mbox", Simple::Group(group)) => self.inline_expression(&group.expr, out),
                ("text" | "mbox", arg) => self.inline_simple(arg, out),
                (op @ "overline", arg) => self.inline_modi(op, OVERLINE, arg, out),
                (op @ ("underline" | "ul"), arg) => self.inline_modi(op, UNDERLINE, arg, out),
                (op @ "cancel", arg) => self.inline_modi(op, CANCEL, arg, out),
                (op @ "hat", arg) => self.inline_char_modi(op, '\u{0302}', None, arg, out),
                (op @ "tilde", arg) => self.inline_char_modi(op, '\u{0303}', None, arg, out),
                // a bar over several chars is an overline
                (op @ "bar", arg) => {
                    self.inline_char_modi(op, '\u{0304}', Some(OVERLINE), arg, out)
                }
                (op @ "dot", arg) => self.inline_char_modi(op, '\u{0307}', None, arg, out),
                (op @ "ddot", arg) => self.inline_char_modi(op, '\u{0308}', None, arg, out),
                (op @ ("overarc" | "overparen"), arg) => {
                    self.inline_char_modi(op, '\u{0311}', None, arg, out)
                }
                (op @ "vec", arg) => self.inline_char_modi(op, '\u{20D7}', None, arg, out),
                (op, arg) => self.inline_ugeneric(op, arg, out),
            }
        }
    }

    fn inline_matrix(self, matrix: &Matrix<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        let left = left_bracket_str(matrix.left_bracket);
        let right = right_bracket_str(matrix.right_bracket);
        // rows in a missing bracket would run together, and rows in bars would read as absolute
        // values
        let (row_left, row_right) = if [left, right]
            .iter()
            .any(|bracket| matches!(*bracket, "" | "|" | "||"))
        {
            ("(", ")")
        } else {
            (left, right)
        };
        out.write_str(left)?;
        for (i, row) in matrix.rows().enumerate() {
            if i > 0 {
                out.write_char(',')?;
            }
            out.write_str(row_left)?;
            for (j, expr) in row.iter().enumerate() {
                let rules = column_rules(matrix, j);
                if j > 0 && rules == 0 {
                    out.write_char(',')?;
                }
                for _ in 0..rules {
                    out.write_char('|')?;
                }
                if let Some(chr) = self.empty_placeholder(expr) {
                    out.write_char(chr)?;
                } else {
                    self.grid_cell().inline_expression(expr, out)?;
                }
            }
            for _ in 0..column_rules(matrix, row.len()) {
                out.write_char('|')?;
            }
            out.write_str(row_right)?;
        }
        out.write_str(right)
    }

    fn inline_group(self, group: &Group<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        out.write_str(left_bracket_str(group.left_bracket))?;
        if let Some(chr) = self.group_placeholder(group) {
            out.write_char(chr)?;
        } else {
            self.inline_expression(&group.expr, out)?;
        }
        out.write_str(right_bracket_str(group.right_bracket))
    }

    pub(crate) fn inline_simple(
        self,
        simple: &Simple<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        if let Some(chr) = self.placeholder(simple) {
            out.write_char(chr)
        } else {
            match simple {
                Simple::Missing => Ok(()),
                &Simple::Number(num) => out.write_str(num),
                &Simple::Text(text) => Self::inline_text(text, out),
                &Simple::Ident(ident) | &Simple::Operator(ident) => out.write_str(ident),
                &Simple::Symbol(Symbol { text, .. }) | &Simple::Sign(text) => {
                    out.write_str(symbol_str(text, self.skin_tone))
                }
                Simple::Signed(signed) => {
                    out.write_str(symbol_str(signed.sign, self.skin_tone))?;
                    self.inline_simple(signed.operand(), out)
                }
                Simple::Func(func) => self.inline_simplefunc(func, out),
                Simple::Unary(unary) => self.inline_simpleunary(unary, out),
                Simple::Binary(binary) => self.inline_simplebinary(binary, out),
                Simple::Group(group) => self.inline_group(group, out),
                Simple::Matrix(matrix) => self.inline_matrix(matrix, out),
            }
        }
    }

    /// Write quoted `text`, each control char that isn't whitespace, which a terminal would act
    /// on rather than show, as `�`
    fn inline_text(text: &str, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        let mut pieces = text.split(|chr: char| chr.is_control() && !chr.is_whitespace());
        if let Some(first) = pieces.next() {
            out.write_str(first)?;
        }
        for piece in pieces {
            out.write_char(char::REPLACEMENT_CHARACTER)?;
            out.write_str(piece)?;
        }
        Ok(())
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

    /// `operand` with its brackets stripped and every char mapped through `conf`, if all map
    pub(crate) fn mapped_operand<'a>(
        self,
        operand: &impl Operand<'a>,
        conf: MapperConf,
    ) -> Option<String> {
        let mut text = String::new();
        operand
            .inline_stripped(self, &mut conf.wrap(&mut text))
            .ok()?;
        Some(text)
    }

    /// `script` with every char mapped through `conf`, if all map; where a script can go at all,
    /// one that isn't there yet is its `placeholder` as is
    pub(crate) fn mapped_script(
        self,
        script: &Simple<'_>,
        conf: Option<MapperConf>,
        placeholder: Option<char>,
    ) -> Option<String> {
        let conf = conf?;
        if let Some(chr) = placeholder
            && self.is_missing(script)
        {
            Some(chr.to_string())
        } else {
            self.mapped_operand(script, conf)
        }
    }

    /// Write `script` through `conf` when every char maps, otherwise after a literal `marker`
    fn inline_sub_or_sup(
        self,
        script: &Simple<'_>,
        conf: Option<MapperConf>,
        marker: char,
        placeholder: Option<char>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        if let Some(text) = self.mapped_script(script, conf, placeholder) {
            out.inner.write_str(&text)
        } else {
            out.write_char(marker)?;
            self.inline_simple(script, out)
        }
    }

    fn inline_script(self, script: &Script<'_>, out: &mut Mapper<impl fmt::Write>) -> fmt::Result {
        match script {
            Script::None => Ok(()),
            Script::Sub(sub) => {
                self.inline_sub_or_sup(sub, out.conf.with_sub(), '_', self.sub_placeholder(), out)
            }
            Script::Super(sup) => {
                self.inline_sub_or_sup(sup, out.conf.with_sup(), '^', self.sup_placeholder(), out)
            }
            Script::Subsuper(sub, sup) => {
                if let Some(lower) =
                    self.mapped_script(sub, out.conf.with_sub(), self.sub_placeholder())
                    && let Some(upper) =
                        self.mapped_script(sup, out.conf.with_sup(), self.sup_placeholder())
                {
                    out.inner.write_str(&lower)?;
                    out.inner.write_str(&upper)
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
        if func.arg().as_simple().is_some_and(name_without_argument) {
            Ok(())
        } else {
            self.inline_applied_arg(func.arg(), func_hugs_argument(func), out)
        }
    }

    fn inline_scriptfunc(
        self,
        func: &ScriptFunc<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        match func {
            ScriptFunc::Simple(simple) => self.inline_simplescript(simple, out),
            ScriptFunc::Func(func) => self.inline_func(func, out),
            ScriptFunc::Signed(signed) => {
                out.write_str(symbol_str(signed.sign, self.skin_tone))?;
                self.inline_scriptfunc(signed.operand(), out)
            }
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

    /// `numer` over `denom` as a vulgar fraction character, with any signs prefixing the
    /// numerator written in front of it
    ///
    /// The character has no numerator to carry the sign, so the sign goes before the whole
    /// fraction. A sign on the denominator stays where it was typed and so never matches.
    ///
    /// This does not look at [`vulgar_fracs`][crate::Conf::vulgar_fracs]; every caller checks it
    /// first.
    pub(crate) fn signed_vulgar_frac<'a>(
        self,
        numer: &impl Operand<'a>,
        denom: &impl Operand<'a>,
        font: Font,
    ) -> Option<String> {
        let mut signs = String::new();
        let mut unsigned = numer;
        while let Some((sign, operand)) = unsigned.peel_sign() {
            signs.push_str(symbol_str(sign, self.skin_tone));
            unsigned = operand;
        }
        let frac = extract_vulgar_frac(
            unsigned.as_simple()?,
            denom.as_simple()?,
            self.strip_brackets,
            |text| font.leaves(text),
        )?;
        signs.push(frac);
        Some(signs)
    }

    /// `numer` over `denom` as a vulgar fraction, after `⅟`, or as a superscript over a
    /// subscript, when one of those fits
    fn mapped_frac<'a>(
        self,
        numer: &impl Operand<'a>,
        denom: &impl Operand<'a>,
        out: &Mapper<impl fmt::Write>,
    ) -> Option<String> {
        if self.vulgar_fracs
            && let Some(frac) = self.signed_vulgar_frac(numer, denom, out.conf.font)
        {
            out.mapped_str(&frac)
        } else if self.script_fracs()
            && let Some(sub_conf) = out.conf.with_sub()
            && let Some(lower) = self.mapped_operand(denom, sub_conf)
        {
            // a styled 1 has no place in `⅟`
            if self.vulgar_fracs
                && matches!(
                    numer.as_simple().map(|num| self.unwrap_single(num)),
                    Some(num!("1"))
                )
                && out.conf.font.leaves("1")
                && !denom.is_negated()
                && let Some(marker) = out.mapped_char('⅟')
            {
                Some(marker + &lower)
            } else {
                let upper = self.mapped_operand(numer, out.conf.with_sup()?)?;
                let slash = out.mapped_char('⁄')?;
                Some(upper + &slash + &lower)
            }
        } else {
            None
        }
    }

    /// Brackets stay: without them a slash makes the order of operations unclear
    fn inline_plain_frac<'a>(
        self,
        numer: &impl Operand<'a>,
        denom: &impl Operand<'a>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        numer.inline_as_written(self, out)?;
        out.write_char('/')?;
        denom.inline_as_written(self, out)
    }

    pub(crate) fn inline_expression(
        self,
        expr: &Expression<'_>,
        out: &mut Mapper<impl fmt::Write>,
    ) -> fmt::Result {
        // a space in a script steps narrower, which a fixed-width font can't draw below a full
        // cell, so the multi-line layout would show a script's operators as padded as full-size
        // math
        let spacing_operators =
            self.spaced_operators && !(self.layout == Layout::Block && out.conf.sub_sup.is_some());
        let mut prev_edge = None;
        let mut prev_spaced_op = false;
        // the runs typed on both sides of an item that renders to nothing both belong in the output
        let mut typed = String::new();
        for inter in expr.iter() {
            // an item that renders to nothing, like an empty group, must not leave a space behind
            let mut text = String::new();
            let (leading_edge, trailing_edge) = {
                let mut item = out.onto(&mut text);
                match inter {
                    Intermediate::Space(space) => {
                        if self.keep_spaces {
                            typed.push_str(space);
                        }
                        continue;
                    }
                    Intermediate::ScriptFunc(func) => {
                        self.inline_scriptfunc(func, &mut item)?;
                        scriptfunc_edges(func)
                    }
                    Intermediate::Frac(frac) => {
                        match self.mapped_frac(&frac.numer, &frac.denom, &item) {
                            Some(compact) => item.inner.write_str(&compact)?,
                            None => self.inline_plain_frac(&frac.numer, &frac.denom, &mut item)?,
                        }
                        frac_edges(frac)
                    }
                }
            };
            if text.is_empty() {
                continue;
            }
            let spaced_op = spacing_operators && inter_is_spaced_op(inter);
            if let Some(prev_edge) = prev_edge {
                if typed.is_empty() {
                    if prev_spaced_op || spaced_op || needs_space(prev_edge, leading_edge) {
                        out.write_char(' ')?;
                    }
                } else if self.layout == Layout::Block {
                    // the multi-line layout places this rendering as one of its lines, so a typed
                    // tab or newline would throw off its width
                    out.write_str(&" ".repeat(UnicodeWidthStr::width(&*typed)))?;
                } else {
                    out.write_str(&typed)?;
                }
            }
            out.inner.write_str(&text)?;
            prev_edge = Some(trailing_edge);
            prev_spaced_op = spaced_op;
            typed.clear();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Conf, Layout, Placeholders, SkinTone};

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
            layout: Layout::InlineScript,
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
            layout: Layout::InlineScript,
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
        assert_eq!(res, "⅟ₛᵢₙ\u{2009}ₓ");

        let opts = Conf {
            layout: Layout::InlinePlain,
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
            layout: Layout::InlinePlain,
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

        // a sign carries the operand after it, so it stays within the argument it signs
        let res = super::super::parse_unicode("root -1 2").to_string();
        assert_eq!(res, "⁻¹√2");

        let res = super::super::parse_unicode("stackrel -1 2").to_string();
        assert_eq!(res, "2⁻¹");

        let res = super::super::parse_unicode("overset -x y").to_string();
        assert_eq!(res, "y⁻ˣ");

        let res = super::super::parse_unicode("frac -1 2").to_string();
        assert_eq!(res, "-½");
    }

    #[test]
    fn functions() {
        let res = super::super::parse_unicode("sin x/x").to_string();
        assert_eq!(res, "ˢⁱⁿ\u{2009}ˣ⁄ₓ");
    }

    #[test]
    fn script() {
        let res = super::super::parse_unicode("x^sin x").to_string();
        assert_eq!(res, "xˢⁱⁿ\u{2009}ˣ");

        let res = super::super::parse_unicode("x^vec(x)").to_string();
        assert_eq!(res, "xˣ\u{20D7}");

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
    fn control_chars_in_text_are_replaced() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("\"a\u{1b}b\u{1}c\u{7f}d\u{9b}e\""), "a�b�c�d�e");
        // whitespace stays as typed
        assert_eq!(render("\"a\tb\nc\u{85}d\""), "a\tb\nc\u{85}d");
        assert_eq!(render("bb \"x\u{1}\""), "𝐱�");
    }

    #[test]
    fn matrix() {
        let opts = Conf::default();

        let res = opts.parse("[ [x, y], [a, b] ]").to_string();
        assert_eq!(res, "[[x,y],[a,b]]");
    }

    #[test]
    fn a_brace_is_written_as_the_command() {
        for layout in [Layout::InlineScript, Layout::InlinePlain] {
            let conf = Conf {
                layout,
                ..Default::default()
            };
            let render = |inp: &str| conf.parse(inp).to_string();
            assert_eq!(render("ubrace(a+b)"), "ubrace (a+b)");
            assert_eq!(render("underbrace(x+y)"), "underbrace (x+y)");
            assert_eq!(render("obrace(a+b)"), "obrace (a+b)");
            assert_eq!(render("overbrace(x+y)"), "overbrace (x+y)");
            assert_eq!(render("ubrace(x)"), "ubrace (x)");
            assert_eq!(render("ubrace(a+b)_c"), "ubrace (a+b)_c");
            assert_eq!(render("ubrace(a+b)_2"), "ubrace (a+b)₂");
            assert_eq!(render("ubrace(x)_n"), "ubrace (x)ₙ");
            assert_eq!(render("underbrace(x+y)_n"), "underbrace (x+y)ₙ");
            assert_eq!(render("obrace(a+b)^c"), "obrace (a+b)ᶜ");
            assert_eq!(render("overbrace(x+y)^2"), "overbrace (x+y)²");
            assert_eq!(render("ubrace(a+b)_c^d"), "ubrace (a+b)_c^d");
        }
    }

    #[test]
    fn matrix_rows_without_a_bracket_to_repeat() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("{(a, if x),(b, else):}"), "{(a,if x),(b,else)");
        assert_eq!(render("{[a,b],[c,d]:}"), "{(a,b),(c,d)");
        assert_eq!(render("{:(a,b),(c,d):}"), "(a,b),(c,d)");
        assert_eq!(render("|(a,b),(c,d)|"), "|(a,b),(c,d)|");
        assert_eq!(render("||(a,b),(c,d)||"), "||(a,b),(c,d)||");
        assert_eq!(render("<<(a,b),(c,d)>>"), "⟨⟨a,b⟩,⟨c,d⟩⟩");
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
    fn signed_vulgar_fracs() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("-1/2"), "-½");
        assert_eq!(render("+1/2"), "+½");
        assert_eq!(render("pm 1/2"), "±½");
        assert_eq!(render("mp 1/2"), "∓½");
        assert_eq!(render("- -1/2"), "--½");
        assert_eq!(render("-1/2+1/3"), "-½+⅓");
        assert_eq!(render("[[1/2,-1],[3,-1/4]]"), "[[½,-1],[3,-¼]]");
        // a fraction with no character form keeps the sign on its numerator
        assert_eq!(render("-3/7"), "⁻³⁄₇");
        assert_eq!(render("-x/y"), "-x/y");
        // a sign on the denominator stays where it was typed
        assert_eq!(render("1/-2"), "¹⁄₋₂");
        assert_eq!(render("-1/-2"), "⁻¹⁄₋₂");
    }

    #[test]
    fn signed_vulgar_fracs_in_scripts() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("x^(-1/2)"), "x^(-½)");
        assert_eq!(render("x_(-1/2)"), "x_(-½)");
    }

    #[test]
    fn signed_vulgar_fracs_plain() {
        let opts = Conf {
            layout: Layout::InlinePlain,
            ..Default::default()
        };
        assert_eq!(opts.parse("-1/2").to_string(), "-½");
        assert_eq!(opts.parse("pm 1/2").to_string(), "±½");
        assert_eq!(opts.parse("-3/7").to_string(), "-3/7");
        assert_eq!(opts.parse("1/-2").to_string(), "1/-2");
    }

    #[test]
    fn signed_fracs_without_vulgar() {
        let opts = Conf {
            vulgar_fracs: false,
            ..Default::default()
        };
        assert_eq!(opts.parse("-1/2").to_string(), "⁻¹⁄₂");
        assert_eq!(opts.parse("pm 1/2").to_string(), "±1/2");
    }

    #[test]
    fn config_no_strip_no_vulgar_no_script() {
        let opts = Conf {
            strip_brackets: false,
            vulgar_fracs: false,
            layout: Layout::InlinePlain,
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
            layout: Layout::InlinePlain,
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
            layout: Layout::InlinePlain,
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
            layout: Layout::InlineScript,
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
    fn a_styled_letter_without_a_script_form_is_not_raised() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("x^(bbb R)"), "x^(ℝ)");
        assert_eq!(render("x^bb x"), "x^𝐱");
        assert_eq!(render("1/(bb x)"), "1/(𝐱)");
        assert_eq!(render("bb(x^2)"), "𝐱^𝟐");
        // a font that leaves a char alone keeps its script form
        assert_eq!(render("bb(x^+)"), "𝐱⁺");
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
    fn nested_fonts_combine() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("bb(it x)"), "𝒙");
        assert_eq!(render("it(bb x)"), "𝒙");
        assert_eq!(render("bbit(x)"), "𝒙");
        assert_eq!(render("bb(bbit x)"), "𝒙");
        assert_eq!(render("cc(bb x)"), "\u{1d501}"); // bold script x
        assert_eq!(render("hat(bb x)"), "\u{1d431}\u{0302}");
        assert_eq!(render("cc(hat(bb x))"), "\u{1d501}\u{0302}");
        assert_eq!(render("cc(overset(a)(bb x))"), "\u{1d501}\u{0363}");
    }

    #[test]
    fn a_fraction_keeps_its_font() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("bb(1/2)"), "𝟏/𝟐");
        assert_eq!(render("bb(1/x)"), "𝟏/𝐱");
        assert_eq!(render("bb(-1/2)"), "-𝟏/𝟐");
        // a font that leaves the digits alone keeps the fraction char
        assert_eq!(render("cc(1/2)"), "½");
        assert_eq!(render("cc(1/3)"), "⅓");
        let plain = Conf::default().with_vulgar_fracs(false);
        assert_eq!(plain.parse("bb(1/2)").to_string(), "𝟏/𝟐");
    }

    #[test]
    fn deeply_nested_marks_render_quickly() {
        let nested = |open: &str, close: &str| {
            let depth = 40;
            format!("{}x{}", open.repeat(depth), close.repeat(depth))
        };
        let start = std::time::Instant::now();
        for (open, close) in [
            ("hat(", ")"),
            ("bar(", ")"),
            ("overset(a)(", ")"),
            ("stackrel(", ")(=)"),
        ] {
            super::super::parse_unicode(&nested(open, close)).to_string();
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn a_mark_under_a_line_still_sits_on_its_char() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("cancel(hat x)"), "x\u{0302}\u{0336}");
        assert_eq!(render("bar(hat x)"), "x\u{0302}\u{0304}");
        assert_eq!(render("ul(vec x)"), "x\u{20d7}\u{0332}");
        assert_eq!(render("cancel(stackrel ? =)"), "\u{225f}\u{0336}");
        assert_eq!(render("cancel(overset a x)"), "x\u{0363}\u{0336}");
        // the command a mark can't sit on is lined like everything else
        assert_eq!(
            render("cancel(hat(xy))"),
            "h\u{0336}a\u{0336}t\u{0336} (x\u{0336}y\u{0336})"
        );
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn a_mark_sits_on_a_char_that_already_has_marks() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("hat(dot q)"), "q\u{0307}\u{0302}");
        assert_eq!(render("hat(dot x)"), "\u{1e8b}\u{0302}");
        assert_eq!(render("hat(cancel x)"), "x\u{0336}\u{0302}");
        assert_eq!(render("vec(ul x)"), "x\u{0332}\u{20d7}");
        assert_eq!(render("overset(a)(hat q)"), "q\u{0302}\u{0363}");
        assert_eq!(render("overset(a)(x)"), "x\u{0363}");
        let dark = Conf {
            skin_tone: SkinTone::Dark,
            ..Default::default()
        };
        assert_eq!(dark.parse("hat :wave:").to_string(), "hat 👋🏿");
    }

    #[test]
    fn no_line_goes_through_an_emoji() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        let dark = Conf {
            skin_tone: SkinTone::Dark,
            ..Default::default()
        };
        assert_eq!(dark.parse("cancel :wave:").to_string(), "cancel 👋🏿");
        assert_eq!(
            render("cancel :family_man_woman_boy:"),
            "cancel 👨\u{200d}👩\u{200d}👦"
        );
        assert_eq!(render("ul :heart:"), "ul ❤\u{fe0f}");
        assert_eq!(render("cancel :us:"), "cancel 🇺🇸");
        assert_eq!(
            render("cancel :england:"),
            "cancel 🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}"
        );
        assert_eq!(render("overline :one:"), "overline 1\u{fe0f}\u{20e3}");
        assert_eq!(render("cancel( :us: x)"), "cancel (🇺🇸x)");
        assert_eq!(render("bar( :us: x)"), "bar (🇺🇸x)");
        assert_eq!(render("ul(cancel :us:)"), "ul (cancel 🇺🇸)");
        assert_eq!(render("cancel(x y)"), "x\u{0336}y\u{0336}");
    }

    #[test]
    fn no_mark_sits_on_an_emoji() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        let dark = Conf {
            skin_tone: SkinTone::Dark,
            ..Default::default()
        };
        assert_eq!(render("hat 🇦🇨"), "hat 🇦🇨");
        assert_eq!(render("hat :us:"), "hat 🇺🇸");
        assert_eq!(render("dot 😶\u{200d}🌫\u{fe0f}"), "dot 😶\u{200d}🌫\u{fe0f}");
        assert_eq!(
            render("vec :family_man_woman_boy:"),
            "vec 👨\u{200d}👩\u{200d}👦"
        );
        assert_eq!(dark.parse("tilde :wave:").to_string(), "tilde 👋🏿");
        assert_eq!(render("hat 👋🏽"), "hat 👋🏽");
        assert_eq!(render("ddot :one:"), "ddot 1\u{fe0f}\u{20e3}");
        assert_eq!(render("hat :heart:"), "hat ❤\u{fe0f}");
        assert_eq!(render("bar :us:"), "bar 🇺🇸");
        assert_eq!(render("overset(a) :us:"), "🇺🇸ᵃ");
        assert_eq!(render("hat( :us: x)"), "hat (🇺🇸x)");
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn a_mark_still_sits_on_a_symbol_drawn_as_text() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("hat ->"), "→\u{0302}");
        assert_eq!(render("vec <->"), "↔\u{20d7}");
        assert_eq!(render("hat sum"), "∑\u{0302}");
        assert_eq!(render("hat ❤"), "❤\u{0302}");
        assert_eq!(render("tilde \"한\""), "한\u{0303}");
        assert_eq!(render("cancel <->"), "↔\u{0336}");
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn a_styled_char_keeps_its_font_under_a_mark() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("bb(hat a)"), "\u{1d41a}\u{0302}");
        assert_eq!(render("bb(hat q)"), "\u{1d42a}\u{0302}");
        assert_eq!(render("bb(dot x)"), "\u{1d431}\u{0307}");
        assert_eq!(render("bbb(tilde n)"), "\u{1d55f}\u{0303}");
        assert_eq!(render("cc(ddot A)"), "\u{1d49c}\u{0308}");
        // an unstyled char still takes its precomposed form
        assert_eq!(render("hat a"), "â");
        assert_eq!(render("ddot iota"), "ϊ");
    }

    #[test]
    #[allow(clippy::unicode_not_nfc)]
    fn a_mark_sits_on_a_raised_or_lowered_char() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("x_(overline a)"), "xₐ\u{0305}");
        assert_eq!(render("x_(bar a)"), "xₐ\u{0304}");
        assert_eq!(render("x^hat a"), "xᵃ\u{0302}");
        assert_eq!(render("x^vec x"), "xˣ\u{20d7}");
        assert_eq!(render("x^overset(a)(x)"), "xˣ\u{0363}");
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
        // the sign and the operand it prefixes are one argument, so both sit inside
        assert_eq!(render("abs -b"), "|-b|");
        assert_eq!(render("norm -x"), "||-x||");
        assert_eq!(render("floor -x"), "⌊-x⌋");
        assert_eq!(render("ceil -x"), "⌈-x⌉");
        // text/mbox renders its content with no delimiters
        assert_eq!(render("text(hi)"), "hi");
    }

    #[test]
    fn overline_underline_cancel() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("overline(x)"), "x\u{0305}");
        assert_eq!(render("underline(x)"), "x\u{0332}");
        assert_eq!(render("cancel(x)"), "x\u{0336}");
        // nested lines all stay, innermost nearest the char, and none is drawn twice
        assert_eq!(render("cancel(overline x)"), "x\u{0305}\u{0336}");
        assert_eq!(render("ul(cancel x)"), "x\u{0336}\u{0332}");
        assert_eq!(render("cancel(cancel x)"), "x\u{0336}");
        assert_eq!(render("ul(bar(xy))"), "x\u{0305}\u{0332}y\u{0305}\u{0332}");
        assert_eq!(render("bar(bar(xy))"), "x\u{0305}y\u{0305}");
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
        assert_eq!(render("(1)/x^2"), "(1)/x²"); // grouped one-over
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
        // operators stay tight, a sign beside a word among them
        assert_eq!(render("x = -1"), "x=-1");
        assert_eq!(render("dx - dy"), "dx-dy");
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
        assert_eq!(render("frac 1 -2"), "¹⁄₋₂");
    }

    #[test]
    fn a_sign_prefixing_a_function_argument() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("sin -x"), "sin -x");
        assert_eq!(render("f -x"), "f -x");
        assert_eq!(render("sin^2 -x"), "sin² -x");
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

    #[test]
    fn placeholders_are_off_by_default() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("sum_"), "∑");
        assert_eq!(render("1/"), "⅟");
        assert_eq!(render("sqrt"), "√");
        assert_eq!(render("bb"), "");
        assert_eq!(render("text"), "");
        assert_eq!(render("sqrt("), "√(");
        assert_eq!(render("1/()"), "⅟");
        assert_eq!(render("[[1,2],[3,]]"), "[[1,2],[3,]]");
        assert_eq!(render("[[],[]]"), "[[],[]]");
        assert_eq!(render("[[1,2],[,]]"), "[[1,2],[,]]");
        assert_eq!(render("[(a,|,b),(c,|,d)]"), "[[a|b],[c|d]]");
        assert_eq!(render("[(a,|,b),(c,|,)]"), "[[a|b],[c|]]");
    }

    #[test]
    fn placeholders_for_missing_arguments() {
        let conf = Conf::default().with_placeholders(Some(Placeholders::default()));
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("sqrt"), "√□");
        assert_eq!(render("bb"), "□");
        assert_eq!(render("text"), "□");
        assert_eq!(render("abs"), "|□|");
        assert_eq!(render("root 3"), "∛□");
        assert_eq!(render("root"), "root □ □");
        assert_eq!(render("1/2 + bb"), "½+□");
        // nothing is missing once the argument is there
        assert_eq!(render("sqrt x"), "√x");
        assert_eq!(render("bb R"), "𝐑");
    }

    #[test]
    fn placeholders_for_missing_fraction_parts() {
        let conf = Conf::default().with_placeholders(Some(Placeholders::default()));
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("1/"), "1/□");
        assert_eq!(render("x/"), "x/□");
        assert_eq!(render("frac"), "□/□");
        assert_eq!(render("frac 1"), "1/□");
        assert_eq!(render("x^(1/)"), "x^(1/□)");
        assert_eq!(render("1/2"), "½");
    }

    #[test]
    fn placeholders_for_missing_scripts() {
        let conf = Conf::default().with_placeholders(Some(Placeholders::default()));
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("x^"), "x⸋");
        assert_eq!(render("sum_"), "∑▫");
        assert_eq!(render("sum_(i=1)^"), "∑ᵢ₌₁⸋");
        assert_eq!(render("sin^"), "sin⸋");
        assert_eq!(render("sqrt(x^"), "√(x⸋");
        // nothing can be raised twice, so the inner script falls back to a literal marker
        assert_eq!(render("x^(y^)"), "x^(y⸋)");
        assert_eq!(render("underset x"), "□ₓ");
        assert_eq!(render("overset"), "□⸋");
        // after a literal marker the script is an ordinary missing argument
        assert_eq!(render("x_y^"), "x_y^□");
    }

    #[test]
    fn placeholders_for_empty_groups() {
        let conf = Conf::default().with_placeholders(Some(Placeholders::default()));
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("sqrt("), "√□");
        assert_eq!(render("sqrt()"), "√□");
        assert_eq!(render("1/("), "1/□");
        assert_eq!(render("abs()"), "|□|");
        assert_eq!(render("x^("), "x⸋");
        assert_eq!(render("x_()"), "x▫");
        assert_eq!(render("("), "□");
        assert_eq!(render("{: :}"), "□");
        assert_eq!(render("[]"), "□");
        // an operand is there, even if what follows it isn't
        assert_eq!(render("(x+)"), "(x+)");
        // a bracket the person typed is not a part that is missing
        assert_eq!(render(")"), ")");
        // brackets that do more than group stay, with the mark between them
        assert_eq!(render("(: :)"), "⟨□⟩");
        assert_eq!(render("<<>>"), "⟨□⟩");
    }

    #[test]
    fn placeholders_for_empty_matrix_cells() {
        let conf = Conf::default().with_placeholders(Some(Placeholders::default()));
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("[[1,2],[3,]]"), "[[1,2],[3,□]]");
        assert_eq!(render("[[],[]]"), "[[□],[□]]");
        assert_eq!(render("[[1,2],[,]]"), "[[1,2],[□,□]]");
        assert_eq!(render("[[1,2],[3,4]]"), "[[1,2],[3,4]]");
        // the lines of an augmented matrix are untouched
        assert_eq!(render("[(a,|,b),(c,|,d)]"), "[[a|b],[c|d]]");
        assert_eq!(render("[(a,|,b),(c,|,)]"), "[[a|b],[c|□]]");
        assert_eq!(render("[(|,a,b,|),(|,c,d,|)]"), "[[|a,b|],[|c,d|]]");
    }

    #[test]
    fn placeholders_keep_brackets_that_are_not_dropped() {
        let conf = Conf::default()
            .with_strip_brackets(false)
            .with_placeholders(Some(Placeholders::default()));
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("abs()"), "|(□)|");
        assert_eq!(render("x^()"), "x^(□)");
        assert_eq!(render("sqrt()"), "√(□)");
        assert_eq!(render("("), "(□");
        assert_eq!(render(")"), ")");
        assert_eq!(render("[[1,2],["), "[[1,2],[□");
    }

    #[test]
    fn placeholders_leave_empty_quotes_alone() {
        let conf = Conf::default().with_placeholders(Some(Placeholders::default()));
        let render = |inp: &str| conf.parse(inp).to_string();
        // an empty quote is text the person wrote, not a part being typed
        assert_eq!(render("\"\""), "");
        assert_eq!(render("text()"), "");
        assert_eq!(render("text("), "");
        assert_eq!(render("x = \"\""), "x=");
    }

    #[test]
    fn placeholders_with_plain_fractions() {
        let conf = Conf {
            layout: Layout::InlinePlain,
            placeholders: Some(Placeholders::default()),
            ..Default::default()
        };
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("(x/n)/"), "(x/n)/□");
        assert_eq!(render("x/n+1/"), "x/n+1/□");
        // a mark has no raised or lowered form, so a fraction holding one keeps its slash
        // whichever one-line layout is picked
        let scripted = Conf {
            layout: Layout::InlineScript,
            ..conf
        };
        assert_eq!(scripted.parse("(x/n)/").to_string(), "(ˣ⁄ₙ)/□");
    }

    #[test]
    fn placeholders_can_be_chosen() {
        let marks = Placeholders::default()
            .with_char('?')
            .with_sub('.')
            .with_sup('!');
        let conf = Conf::default().with_placeholders(Some(marks));
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("sqrt"), "√?");
        assert_eq!(render("1/"), "1/?");
        assert_eq!(render("x_"), "x.");
        assert_eq!(render("x^"), "x!");
        assert_eq!(render("sum_(i=1)^"), "∑ᵢ₌₁!");
    }

    #[test]
    fn function_names_take_no_placeholder() {
        let conf = Conf::default().with_placeholders(Some(Placeholders::default()));
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("f"), "f");
        assert_eq!(render("sin"), "sin");
        assert_eq!(render("sin^2"), "sin²");
        assert_eq!(render("sqrt sin"), "√sin");
    }

    #[test]
    fn nothing_to_render_leaves_no_space() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("hat"), "hat"); // a command with no argument
        assert_eq!(render("g color(red)"), "g"); // an argument that renders to nothing
        assert_eq!(render("dx g color(red)() dy"), "dx g dy");
    }

    #[test]
    fn double_struck_greek() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("bbb(Gamma)"), "ℾ");
        assert_eq!(render("bbb(Pi)"), "ℿ");
        assert_eq!(render("bbb(gamma)"), "ℽ");
        assert_eq!(render("bbb(pi)"), "ℼ");
        assert_eq!(render("bbb(sum)"), "⅀");
    }

    #[test]
    fn similar_and_wide_spaces() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("a~b"), "a∼b");
        assert_eq!(render("a sim b"), "a∼b");
        assert_eq!(render("a quad b"), "a\u{2003}b");
        assert_eq!(render("a qquad b"), "a\u{2003}\u{2003}b");
        // an escaped space stays one plain space
        assert_eq!(render("a \\ b"), "a b");
    }

    #[test]
    fn spaces_are_dropped_by_default() {
        let conf = Conf::default();
        assert_eq!(conf.parse("a + b").to_string(), "a+b");
        assert_eq!(conf.parse("(- x) y").to_string(), "(-x)y");
    }

    #[test]
    fn kept_spaces_are_written_as_typed() {
        let conf = Conf::default().with_keep_spaces(true);
        assert_eq!(conf.parse(" a  +\tb ").to_string(), "a  +\tb");
        assert_eq!(conf.parse("(a , b)").to_string(), "(a , b)");
        assert_eq!(conf.parse("x^(a b)").to_string(), "xᵃ\u{2009}ᵇ");
        assert_eq!(
            conf.parse("[[a b, c], [d, e]]").to_string(),
            "[[a b,c],[d,e]]"
        );
    }

    #[test]
    fn kept_spaces_within_one_part_are_dropped() {
        let conf = Conf::default().with_keep_spaces(true);
        assert_eq!(conf.parse("1 / 2  x ^ 2").to_string(), "½  x²");
        assert_eq!(conf.parse("sqrt  x").to_string(), "√x");
    }

    #[test]
    fn kept_spaces_leave_needed_spaces_alone() {
        let conf = Conf::default().with_keep_spaces(true);
        assert_eq!(conf.parse("sinx lim_x y").to_string(), "sin x limₓ y");
        assert_eq!(conf.parse("sin x").to_string(), "sin x");
    }

    #[test]
    fn kept_spaces_outlive_what_renders_to_nothing() {
        let conf = Conf::default().with_keep_spaces(true);
        assert_eq!(conf.parse("g color(red)").to_string(), "g");
        assert_eq!(conf.parse("color(red)() a").to_string(), "a");
        assert_eq!(conf.parse("a  color(red)() b").to_string(), "a   b");
        assert_eq!(conf.parse("a  color(red)()  b").to_string(), "a    b");
    }

    #[test]
    fn kept_spaces_do_not_hide_a_negated_denominator() {
        let conf = Conf::default();
        assert_eq!(conf.parse("1/(- x)").to_string(), "¹⁄₋ₓ");
        assert_eq!(
            conf.with_keep_spaces(true).parse("1/(- x)").to_string(),
            "¹⁄₋ₓ"
        );
    }

    #[test]
    fn a_kept_space_after_a_prefixing_sign_is_dropped() {
        let conf = Conf::default().with_keep_spaces(true);
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("- x"), "-x");
        assert_eq!(render("+ x"), "+x");
        assert_eq!(render("+- x"), "±x");
        assert_eq!(render("pm x"), "±x");
        assert_eq!(render("-+ x"), "∓x");
        assert_eq!(render("mp x"), "∓x");
        assert_eq!(render("1 + - x"), "1 + -x");
        assert_eq!(render("a (- b)"), "a (-b)");
        assert_eq!(render("- (a + b)"), "-(a + b)");
        assert_eq!(render("(- x)/2"), "⁻ˣ⁄₂");
        assert_eq!(render("x^(- a)"), "x⁻ᵃ");
        assert_eq!(render("x_(- a)"), "x₋ₐ");
        assert_eq!(render("[[- a, b], [c, - d]]"), "[[-a,b],[c,-d]]");
        // the sign prefixes an operand that isn't there, and the space before it goes anyway
        assert_eq!(render("-"), "-");
        assert_eq!(render("- "), "-");
    }

    #[test]
    fn a_kept_space_around_a_binary_sign_stays() {
        let conf = Conf::default().with_keep_spaces(true);
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("a - b"), "a - b");
        assert_eq!(render("a  -  b"), "a  -  b");
        assert_eq!(render("a  -\tb"), "a  -\tb");
        assert_eq!(render("- x + y"), "-x + y");
        // the second sign follows an operator, so only the space after it goes
        assert_eq!(render("a - - b"), "a - -b");
        // the scripts don't stand in for the big operator they sit on, so the sign prefixes
        assert_eq!(render("sum_(i=1)^n - i"), "∑ᵢ₌₁ⁿ -i");
    }

    #[test]
    fn kept_tabs_and_newlines_are_written_as_typed() {
        let conf = Conf::default().with_keep_spaces(true);
        assert_eq!(conf.parse("a\tb").to_string(), "a\tb");
        assert_eq!(conf.parse("a\nb").to_string(), "a\nb");
        assert_eq!(conf.parse("a\n\nb").to_string(), "a\n\nb");
    }

    #[test]
    fn kept_spaces_in_a_script_step_narrower() {
        let conf = Conf::default().with_keep_spaces(true);
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("x^(a b)"), "xᵃ\u{2009}ᵇ");
        assert_eq!(render("x^(a\tb)"), "xᵃ\u{2009}ᵇ");
        assert_eq!(render("x^(a\u{2003}b)"), "xᵃ\u{2002}ᵇ");
        assert_eq!(render("x^(a\u{200a}b)"), "xᵃ\u{200a}ᵇ");
        assert_eq!(render("x_(i j)"), "xᵢ\u{2009}ⱼ");
        assert_eq!(render("x_(i\tj)"), "xᵢ\u{2009}ⱼ");
        assert_eq!(render("x_(i\u{2003}j)"), "xᵢ\u{2002}ⱼ");
        assert_eq!(render("x_(i\u{200a}j)"), "xᵢ\u{200a}ⱼ");
    }

    #[test]
    fn a_space_symbol_in_a_script_steps_narrower() {
        let render = |inp: &str| super::super::parse_unicode(inp).to_string();
        assert_eq!(render("x^(a quad b)"), "xᵃ\u{2002}ᵇ");
        assert_eq!(render("x^(a qquad b)"), "xᵃ\u{2002}\u{2002}ᵇ");
        assert_eq!(render("x_(i enspace j)"), "xᵢ\u{2004}ⱼ");
        assert_eq!(render("x_(i thinspace j)"), "xᵢ\u{200a}ⱼ");
        assert_eq!(render("x^(a \\ b)"), "xᵃ\u{2009}ᵇ");

        let plain = Conf {
            layout: Layout::InlinePlain,
            ..Default::default()
        };
        assert_eq!(plain.parse("x^(a quad b)").to_string(), "xᵃ\u{2002}ᵇ");
        assert_eq!(plain.parse("x^(a quad b)/y").to_string(), "xᵃ\u{2002}ᵇ/y");
    }

    #[test]
    fn kept_spaces_with_plain_fractions() {
        let conf = Conf {
            layout: Layout::InlinePlain,
            keep_spaces: true,
            ..Default::default()
        };
        assert_eq!(conf.parse("x / y   z").to_string(), "x/y   z");
        assert_eq!(conf.parse("1 / 2  x ^ 2").to_string(), "½  x²");
        assert_eq!(conf.parse("a\tb").to_string(), "a\tb");
    }

    #[test]
    fn kept_spaces_with_placeholders() {
        let conf = Conf::default()
            .with_keep_spaces(true)
            .with_placeholders(Some(Placeholders::default()));
        let render = |inp: &str| conf.parse(inp).to_string();
        // the mark renders, so neither run around it is joined to the other
        assert_eq!(render("a  color(red)() b"), "a  □ b");
        assert_eq!(render("color(red)() a"), "□ a");
        assert_eq!(render("[[a  b,],[d,e]]"), "[[a  b,□],[d,e]]");
    }

    #[test]
    fn operators_join_their_operands_by_default() {
        let conf = Conf::default();
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("a+b"), "a+b");
        assert_eq!(render("a+b=c"), "a+b=c");
        assert_eq!(render("x<=y"), "x≤y");
        assert_eq!(render("a in A"), "a∈A");
    }

    #[test]
    fn spaced_operators_get_a_space_on_either_side() {
        let conf = Conf::default().with_spaced_operators(true);
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("a+b"), "a + b");
        assert_eq!(render("a+b=c"), "a + b = c");
        assert_eq!(render("x<=y"), "x ≤ y");
        assert_eq!(render("a in A"), "a ∈ A");
    }

    #[test]
    fn a_spaced_sign_still_hugs_its_operand() {
        let conf = Conf::default().with_spaced_operators(true);
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("-x"), "-x");
        assert_eq!(render("-x+1"), "-x + 1");
        assert_eq!(render("x = -1"), "x = -1");
        assert_eq!(render("1 - - x"), "1 - -x");
        // `:=` gets no spaces of its own yet, but the sign after it still hugs
        assert_eq!(render("a := -b"), "a:=-b");
    }

    #[test]
    fn spaced_operators_leave_word_spacing_alone() {
        let conf = Conf::default().with_spaced_operators(true);
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("a and b"), "a and b");
        assert_eq!(render("a and -b"), "a and -b");
        assert_eq!(render("x mod y"), "x mod y");
        assert_eq!(render("sinx+1"), "sin x + 1");
        assert_eq!(render("x > 0 or x < -1"), "x > 0 or x < -1");
    }

    #[test]
    fn a_spaced_operator_in_a_script_steps_narrower() {
        let conf = Conf::default().with_spaced_operators(true);
        assert_eq!(Conf::default().parse("x^(a+b)").to_string(), "xᵃ⁺ᵇ");
        assert_eq!(conf.parse("x^(a+b)").to_string(), "xᵃ\u{2009}⁺\u{2009}ᵇ");
        assert_eq!(conf.parse("x_(i+1)").to_string(), "xᵢ\u{2009}₊\u{2009}₁");
        assert_eq!(
            conf.parse("sum_(i=1)^n i").to_string(),
            "∑ᵢ\u{2009}₌\u{2009}₁ⁿ i"
        );

        let plain = conf.with_layout(Layout::InlinePlain);
        assert_eq!(plain.parse("x^(a+b)").to_string(), "xᵃ\u{2009}⁺\u{2009}ᵇ");
        assert_eq!(plain.parse("x_(i+1)").to_string(), "xᵢ\u{2009}₊\u{2009}₁");

        let brackets = conf.with_strip_brackets(false);
        assert_eq!(
            brackets.parse("x^(a+b)").to_string(),
            "x⁽ᵃ\u{2009}⁺\u{2009}ᵇ⁾"
        );
    }

    #[test]
    fn a_script_keeps_the_spaces_the_option_did_not_add() {
        let conf = Conf::default().with_spaced_operators(true);
        assert_eq!(conf.parse("sin x/x").to_string(), "ˢⁱⁿ\u{2009}ˣ⁄ₓ");
        assert_eq!(conf.parse("x^sin x").to_string(), "xˢⁱⁿ\u{2009}ˣ");
        assert_eq!(conf.parse("x^(a quad b)").to_string(), "xᵃ\u{2002}ᵇ");

        let typed = conf.with_keep_spaces(true);
        assert_eq!(typed.parse("x^(a b)").to_string(), "xᵃ\u{2009}ᵇ");
        assert_eq!(
            typed.parse("x^(a  +b)").to_string(),
            "xᵃ\u{2009}\u{2009}⁺\u{2009}ᵇ"
        );
        assert_eq!(
            typed.parse("x_(i  =1)").to_string(),
            "xᵢ\u{2009}\u{2009}₌\u{2009}₁"
        );
    }

    #[test]
    fn spaced_operators_inside_a_grid() {
        let conf = Conf::default().with_spaced_operators(true);
        assert_eq!(
            Conf::default().parse("[[a+b,c],[d,e-f]]").to_string(),
            "[[a+b,c],[d,e-f]]"
        );
        assert_eq!(
            conf.parse("[[a+b,c],[d,e-f]]").to_string(),
            "[[a + b,c],[d,e - f]]"
        );
    }

    #[test]
    fn spaced_operators_yield_to_what_was_typed() {
        let conf = Conf::default()
            .with_keep_spaces(true)
            .with_spaced_operators(true);
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("a+b"), "a + b");
        assert_eq!(render("a  +b"), "a  + b");
        assert_eq!(render("a+ b"), "a + b");
        assert_eq!(render("a  +  b"), "a  +  b");
        assert_eq!(render("1 + - x"), "1 + -x");
        assert_eq!(render("- x"), "-x");
    }

    #[test]
    fn spaced_operators_with_plain_fractions() {
        let conf = Conf {
            layout: Layout::InlinePlain,
            spaced_operators: true,
            ..Default::default()
        };
        assert_eq!(conf.parse("x/y+z").to_string(), "x/y + z");
        assert_eq!(conf.parse("1 / 2 + x").to_string(), "½ + x");

        let typed = conf.with_keep_spaces(true);
        assert_eq!(typed.parse("x/y+z").to_string(), "x/y + z");
        assert_eq!(typed.parse("x/y  +z").to_string(), "x/y  + z");
    }

    #[test]
    fn spaced_operators_around_a_placeholder() {
        let conf = Conf::default()
            .with_spaced_operators(true)
            .with_placeholders(Some(Placeholders::default()));
        assert_eq!(conf.parse("a+()").to_string(), "a + □");
        assert_eq!(conf.parse("()+a").to_string(), "□ + a");
        // the mark has no raised form, so the script is written out at full size
        assert_eq!(conf.parse("x^(a+())").to_string(), "x^(a + □)");
    }

    #[test]
    fn a_spaced_sign_hugs_after_a_comma_or_a_bare_big_operator() {
        let conf = Conf::default().with_spaced_operators(true);
        let render = |inp: &str| conf.parse(inp).to_string();
        assert_eq!(render("(1,-2)"), "(1,-2)");
        assert_eq!(render("f(x,-y)"), "f(x,-y)");
        assert_eq!(render("x_(1,-2)"), "x₁,₋₂");
        assert_eq!(render("sum -x"), "∑-x");
        assert_eq!(render("int -x"), "∫-x");
        assert_eq!(render("prod +x"), "∏+x");
        // the scripts don't stand in for the big operator they sit on, so the sign prefixes
        assert_eq!(render("sum_(i=1)^n - i"), "∑ᵢ\u{2009}₌\u{2009}₁ⁿ -i");

        let plain = conf.with_layout(Layout::InlinePlain);
        assert_eq!(plain.parse("(1,-2)").to_string(), "(1,-2)");
        assert_eq!(plain.parse("sum -x").to_string(), "∑-x");
    }
}
