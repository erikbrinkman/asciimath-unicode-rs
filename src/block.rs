//! The multi-line layout

use std::fmt::Write;
use std::{fmt, iter};
use unicode_width::UnicodeWidthStr;

use super::Conf;
use super::ast::{ArgGap, Separator, delimiters, separated_items, unwrap_parens};
use super::inline::{
    Font, Mapper, MapperConf, Operand, Shift, column_rules, font_styles, is_brace, root_char,
};
use super::tokens::bracket_str;

use asciimath_parser::tree::{
    Expression, Frac, Func, Group, Intermediate, Matrix, ScriptFunc, Simple, SimpleBinary,
    SimpleFunc, SimpleScript, SimpleUnary,
};

/// A 2D text block for multi-line rendering.
/// All lines are padded to `width` display columns with trailing spaces.
#[derive(Debug, Clone)]
pub(crate) struct Block {
    lines: Vec<String>,
    baseline: usize,
    width: usize,
    /// Whether a fraction bar is drawn in it
    bar: bool,
}

impl Block {
    /// One line of `text`, each control char or line or paragraph separator, which would throw
    /// off the lines stacked with this one, written as a space if it is whitespace and as `�`
    /// otherwise
    fn text(text: impl Into<String>) -> Self {
        let owned: String = text
            .into()
            .chars()
            .map(|chr| {
                if !chr.is_control() && !matches!(chr, '\u{2028}' | '\u{2029}') {
                    chr
                } else if chr.is_whitespace() {
                    ' '
                } else {
                    char::REPLACEMENT_CHARACTER
                }
            })
            .collect();
        let width = UnicodeWidthStr::width(&*owned);
        Block {
            lines: vec![owned],
            baseline: 0,
            width,
            bar: false,
        }
    }

    fn space(width: usize) -> Self {
        Block {
            lines: vec![" ".repeat(width)],
            baseline: 0,
            width,
            bar: false,
        }
    }

    /// The same line repeated to fill a block `height` tall
    fn repeated(line: String, height: usize, baseline: usize) -> Self {
        Block {
            width: UnicodeWidthStr::width(&*line),
            lines: vec![line; height],
            baseline,
            bar: false,
        }
    }

    fn height(&self) -> usize {
        self.lines.len()
    }

    /// Whether nothing at all is written in it
    fn is_empty(&self) -> bool {
        self.lines.iter().all(String::is_empty)
    }

    fn is_multiline(&self) -> bool {
        self.lines.len() > 1
    }

    /// How many lines are below the baseline
    fn depth(&self) -> usize {
        self.height() - 1 - self.baseline
    }

    fn beside(self, other: Self) -> Self {
        let above = self.baseline.max(other.baseline);
        let below = self.depth().max(other.depth());
        let width = self.width + other.width;
        let bar = self.bar || other.bar;
        let right = other.pad_vertical(above, below);
        let lines = self
            .pad_vertical(above, below)
            .lines
            .into_iter()
            .zip(right.lines)
            .map(|(mut left, right)| {
                left.push_str(&right);
                left
            })
            .collect();
        Block {
            lines,
            baseline: above,
            width,
            bar,
        }
    }

    fn stack_frac(numer: Self, denom: Self) -> Self {
        // a nested fraction's bar would be as long as this one without the overhang
        let overhang = if numer.bar || denom.bar { 2 } else { 0 };
        let bar_width = numer.width.max(denom.width) + overhang;
        let bar = "─".repeat(bar_width);
        let baseline = numer.lines.len();

        let lines = numer
            .lines
            .into_iter()
            .map(|line| center_pad(&line, numer.width, bar_width))
            .chain(iter::once(bar))
            .chain(
                denom
                    .lines
                    .into_iter()
                    .map(|line| center_pad(&line, denom.width, bar_width)),
            )
            .collect();

        Block {
            baseline,
            width: bar_width,
            lines,
            bar: true,
        }
    }

    fn with_brackets(self, left: &str, right: &str) -> Self {
        if left.is_empty() && right.is_empty() {
            self
        } else {
            let left_col =
                tall_bracket(left, self.height(), left_bracket_pieces).with_baseline(self.baseline);
            let right_col = tall_bracket(right, self.height(), right_bracket_pieces)
                .with_baseline(self.baseline);
            left_col.beside(self).beside(right_col)
        }
    }

    /// Attach scripts to the right in one column, `sup` above this block and `sub` below it
    fn with_scripts(self, sub: Option<Self>, sup: Option<Self>) -> Self {
        let sup_height = sup.as_ref().map_or(0, Block::height);
        let width = sub
            .iter()
            .chain(&sup)
            .map(|block| block.width)
            .max()
            .unwrap_or(0);
        let blank = " ".repeat(width);
        let bar = sub.iter().chain(&sup).any(|block| block.bar);
        let lines = sup
            .into_iter()
            .flat_map(|block| block.pad_right(width).lines)
            .chain(iter::repeat_n(blank, self.height()))
            .chain(
                sub.into_iter()
                    .flat_map(|block| block.pad_right(width).lines),
            )
            .collect();
        let column = Block {
            lines,
            baseline: sup_height + self.baseline,
            width,
            bar,
        };
        self.beside(column)
    }

    /// Attach scripts already raised or lowered to the right, on the baseline of a one-line block
    /// and otherwise with `sup` level with its top line and `sub` with its bottom one
    fn with_small_scripts(self, sub: Option<String>, sup: Option<String>) -> Self {
        if self.is_multiline() {
            let top = sup.map(Block::text);
            let bottom = sub.map(Block::text);
            let width = top
                .iter()
                .chain(&bottom)
                .map(|block| block.width)
                .max()
                .unwrap_or(0);
            let line = |block: Option<Block>| {
                block.map_or_else(
                    || " ".repeat(width),
                    |block| block.pad_right(width).lines.swap_remove(0),
                )
            };
            let lines = iter::once(line(top))
                .chain(iter::repeat_n(" ".repeat(width), self.height() - 2))
                .chain(iter::once(line(bottom)))
                .collect();
            let column = Block {
                lines,
                baseline: self.baseline,
                width,
                bar: false,
            };
            self.beside(column)
        } else {
            let text = sub.unwrap_or_default() + &sup.unwrap_or_default();
            self.beside(Block::text(text))
        }
    }

    /// This function name followed by the argument `arg` renders, meeting it as `gap` says
    fn with_arg(self, gap: ArgGap, arg: impl FnOnce() -> Self) -> Self {
        if gap == ArgGap::Bare {
            self
        } else {
            let arg = arg();
            if gap.spaced(arg.is_empty()) {
                self.beside(Block::space(1)).beside(arg)
            } else {
                self.beside(arg)
            }
        }
    }

    fn pad_right(mut self, width: usize) -> Self {
        if width > self.width {
            let extra = " ".repeat(width - self.width);
            for line in &mut self.lines {
                line.push_str(&extra);
            }
            self.width = width;
        }
        self
    }

    fn with_baseline(mut self, new_baseline: usize) -> Self {
        self.baseline = new_baseline;
        self
    }

    /// Pad vertically: ensure `above` lines above baseline, `below` lines below.
    fn pad_vertical(mut self, above: usize, below: usize) -> Self {
        let cur_below = self.depth();
        if above > self.baseline {
            let extra: Vec<String> = iter::repeat_with(|| " ".repeat(self.width))
                .take(above - self.baseline)
                .collect();
            self.lines.splice(0..0, extra);
        }
        if below > cur_below {
            self.lines
                .extend(iter::repeat_with(|| " ".repeat(self.width)).take(below - cur_below));
        }
        self.baseline = above;
        self
    }

    /// Center horizontally to `width` display columns.
    fn pad_center(mut self, width: usize) -> Self {
        if width > self.width {
            for line in &mut self.lines {
                *line = center_pad(line, self.width, width);
            }
            self.width = width;
        }
        self
    }

    /// This block with a brace as wide as it drawn below it when `under` and above it otherwise,
    /// and `label` centered beyond the brace
    fn with_brace(self, under: bool, label: Option<Self>) -> Self {
        if self.width == 0 {
            self
        } else {
            let label_width = label.as_ref().map_or(0, |block| block.width);
            let width = self.width.max(label_width);
            let bar = self.bar || label.as_ref().is_some_and(|block| block.bar);
            let brace = Block::text(brace_line(self.width, under)).pad_center(width);
            let label_lines = label.map_or_else(Vec::new, |block| block.pad_center(width).lines);
            let arg = self.pad_center(width);
            if under {
                Block {
                    baseline: arg.baseline,
                    lines: arg
                        .lines
                        .into_iter()
                        .chain(brace.lines)
                        .chain(label_lines)
                        .collect(),
                    width,
                    bar,
                }
            } else {
                Block {
                    baseline: label_lines.len() + 1 + arg.baseline,
                    lines: label_lines
                        .into_iter()
                        .chain(brace.lines)
                        .chain(arg.lines)
                        .collect(),
                    width,
                    bar,
                }
            }
        }
    }
}

/// A horizontal brace `width` columns wide, its tick pointing down when `under` and up otherwise
fn brace_line(width: usize, under: bool) -> String {
    let (left, right, tick, (tick_left, tick_right)) = if under {
        ('╰', '╯', '┬', ('╮', '╭'))
    } else {
        ('╭', '╮', '┴', ('╯', '╰'))
    };
    match width {
        0 => String::new(),
        1 => tick.to_string(),
        2 => format!("{left}{right}"),
        // an even width has no middle column, so the tick is split over the two middle ones
        _ if width.is_multiple_of(2) => {
            let run = "─".repeat((width - 4) / 2);
            format!("{left}{run}{tick_left}{tick_right}{run}{right}")
        }
        _ => {
            let run = "─".repeat((width - 3) / 2);
            format!("{left}{run}{tick}{run}{right}")
        }
    }
}

impl fmt::Display for Block {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, line) in self.lines.iter().enumerate() {
            if idx > 0 {
                formatter.write_str("\n")?;
            }
            formatter.write_str(line.trim_end_matches(' '))?;
        }
        Ok(())
    }
}

fn center_pad(line: &str, current_width: usize, target_width: usize) -> String {
    if current_width >= target_width {
        line.to_string()
    } else {
        let left = (target_width - current_width).div_ceil(2);
        let right = target_width - current_width - left;
        format!("{}{line}{}", " ".repeat(left), " ".repeat(right))
    }
}

/// Glyphs for a tall bracket: top, the two middle rows, bottom, and the fill between them
type BracketPieces = (char, char, char, char, char);

/// The pieces of a tall left bracket, from its [`bracket_str`] form
fn left_bracket_pieces(bracket: &str, height: usize) -> BracketPieces {
    match bracket {
        "(" => ('⎛', '⎜', '⎜', '⎝', '⎜'),
        "[" => ('⎡', '⎢', '⎢', '⎣', '⎢'),
        "⟨" => ('╱', '⎜', '⎜', '╲', '⎜'),
        "⌊" => ('⎢', '⎢', '⎢', '⌊', '⎢'),
        "⌈" => ('⌈', '⎢', '⎢', '⎢', '⎢'),
        "{" if height == 2 => ('⎰', ' ', ' ', '⎱', ' '),
        "{" if height.is_multiple_of(2) => ('⎧', '⎭', '⎫', '⎩', '⎪'),
        "{" => ('⎧', '⎪', '⎨', '⎩', '⎪'),
        "||" => ('‖', '‖', '‖', '‖', '‖'),
        _ => ('│', '│', '│', '│', '│'),
    }
}

/// The pieces of a tall right bracket, from its [`bracket_str`] form
fn right_bracket_pieces(bracket: &str, height: usize) -> BracketPieces {
    match bracket {
        ")" => ('⎞', '⎟', '⎟', '⎠', '⎟'),
        "]" => ('⎤', '⎥', '⎥', '⎦', '⎥'),
        "⟩" => ('╲', '⎟', '⎟', '╱', '⎟'),
        "⌋" => ('⎥', '⎥', '⎥', '⌋', '⎥'),
        "⌉" => ('⌉', '⎥', '⎥', '⎥', '⎥'),
        "}" if height == 2 => ('⎱', ' ', ' ', '⎰', ' '),
        "}" if height.is_multiple_of(2) => ('⎫', '⎩', '⎧', '⎭', '⎪'),
        "}" => ('⎫', '⎪', '⎬', '⎭', '⎪'),
        "||" => ('‖', '‖', '‖', '‖', '‖'),
        _ => ('│', '│', '│', '│', '│'),
    }
}

fn tall_bracket(bracket: &str, height: usize, pieces: fn(&str, usize) -> BracketPieces) -> Block {
    if bracket.is_empty() {
        Block::repeated(String::new(), height, height / 2)
    } else if height <= 1 {
        Block::text(bracket)
    } else {
        let (top, mid_top, mid_bot, bot, fill) = pieces(bracket, height);
        let lines = (0..height)
            .map(|idx| {
                if idx == 0 {
                    top
                } else if idx == height - 1 {
                    bot
                } else if idx == height / 2 {
                    mid_bot
                } else if idx == height / 2 - 1 {
                    mid_top
                } else {
                    fill
                }
                .to_string()
            })
            .collect();
        Block {
            lines,
            baseline: height / 2,
            width: 1,
            bar: false,
        }
    }
}

/// One line of the separator at a matrix column boundary: `rules` vertical lines, with the column
/// gap around them unless the boundary sits against a bracket
fn column_separator(rules: usize, boundary: usize, num_cols: usize) -> String {
    let gap = if boundary == 0 || boundary == num_cols {
        ""
    } else {
        " "
    };
    format!("{gap}{}{gap}", "\u{2502}".repeat(rules))
}

/// What `render` writes on one line in `font`, as a block
fn one_line(font: Font, render: impl FnOnce(&mut Mapper<'_, String>) -> fmt::Result) -> Block {
    let mut text = String::new();
    let conf = MapperConf {
        font,
        ..MapperConf::default()
    };
    render(&mut conf.wrap(&mut text))
        .unwrap_or_else(|_| unreachable!("write to String is infallible"));
    Block::text(text)
}

/// `text` in `font`
fn styled(text: &str, font: Font) -> Block {
    one_line(font, |out| out.write_str(text))
}

impl Conf {
    fn block_inline_simple(self, simple: &Simple<'_>, font: Font) -> Block {
        one_line(font, |out| self.inline_simple(simple, out))
    }

    /// `sign` in `font`, written against the `operand` it prefixes
    fn block_signed(self, sign: &str, operand: Block, font: Font) -> Block {
        self.block_inline_simple(&Simple::Sign(sign), font)
            .beside(operand)
    }

    /// Render `expr` in the multi-line layout in `font`, spacing its items
    pub(crate) fn block_expression(self, expr: &Expression<'_>, font: Font) -> Block {
        let items = separated_items(
            expr,
            self.keep_spaces,
            self.spaced_operators,
            |inter, sep| {
                let block = match inter {
                    Intermediate::Space(_) => None,
                    Intermediate::ScriptFunc(func) => Some(self.block_scriptfunc(func, font)),
                    Intermediate::Frac(frac) => Some(self.block_frac(frac, font)),
                }
                .filter(|block| !block.is_empty())?;
                let gap = match sep {
                    Separator::Typed(run) => UnicodeWidthStr::width(run),
                    Separator::Space => 1,
                    Separator::Nothing => 0,
                };
                Some((gap, block))
            },
        );
        let mut result = Block::space(0);
        for (gap, block) in items {
            if gap > 0 {
                result = result.beside(Block::space(gap));
            }
            result = result.beside(block);
        }
        result
    }

    fn block_scriptfunc(self, sf: &ScriptFunc<'_>, font: Font) -> Block {
        match sf {
            ScriptFunc::Simple(ss) => self.block_simplescript(ss, font),
            ScriptFunc::Func(func) => self.block_func(func, font),
            ScriptFunc::Signed(signed) => self.block_signed(
                signed.sign,
                self.block_scriptfunc(signed.operand(), font),
                font,
            ),
        }
    }

    fn block_simplescript(self, ss: &SimpleScript<'_>, font: Font) -> Block {
        if let Simple::Unary(unary) = &ss.simple
            && is_brace(unary.op)
            && self.placeholder(&ss.simple).is_none()
        {
            let under = matches!(unary.op, "ubrace" | "underbrace");
            let (label, sub, sup) = if under {
                (ss.script.sub(), None, ss.script.sup())
            } else {
                (ss.script.sup(), ss.script.sub(), None)
            };
            let braced = self.block_simple_stripped(unary.arg(), font).with_brace(
                under,
                label.map(|label| self.block_simple_stripped(label, font)),
            );
            self.block_scripts(braced, sub, sup, font)
        } else {
            let base_block = self.block_simple(&ss.simple, font);
            self.block_scripts(base_block, ss.script.sub(), ss.script.sup(), font)
        }
    }

    /// `base` with `sub` and `sup` attached, shifted on its lines when every one has a shifted
    /// form and stacked beside it otherwise
    fn block_scripts(
        self,
        base: Block,
        sub: Option<&Simple<'_>>,
        sup: Option<&Simple<'_>>,
        font: Font,
    ) -> Block {
        let styled = MapperConf {
            font,
            ..MapperConf::default()
        };
        let lower = sub.map(|script| self.mapped_script(script, styled, Shift::Sub));
        let upper = sup.map(|script| self.mapped_script(script, styled, Shift::Sup));
        if lower.is_none() && upper.is_none() {
            base
        } else if lower.as_ref().is_none_or(Option::is_some)
            && upper.as_ref().is_none_or(Option::is_some)
        {
            base.with_small_scripts(lower.flatten(), upper.flatten())
        } else {
            base.with_scripts(
                sub.map(|script| self.block_simple_stripped(script, font)),
                sup.map(|script| self.block_simple_stripped(script, font)),
            )
        }
    }

    fn block_simple(self, simple: &Simple<'_>, font: Font) -> Block {
        if self.placeholder(simple).is_some() {
            self.block_inline_simple(simple, font)
        } else {
            match simple {
                Simple::Missing => Block::space(0),
                Simple::Group(group) => self.block_group(group, font),
                Simple::Matrix(matrix) => self.block_matrix(matrix, font),
                Simple::Unary(unary) => self.block_unary(unary, font),
                Simple::Binary(binary) => self.block_binary(binary, font),
                Simple::Func(func) => self.block_simplefunc(func, font),
                Simple::Signed(signed) => {
                    self.block_signed(signed.sign, self.block_simple(signed.operand(), font), font)
                }
                _ => self.block_inline_simple(simple, font),
            }
        }
    }

    fn block_simplefunc(self, func: &SimpleFunc<'_>, font: Font) -> Block {
        styled(func.func, font).with_arg(ArgGap::of_simple(func.arg()), || {
            self.block_simple(func.arg(), font)
        })
    }

    fn block_unary(self, unary: &SimpleUnary<'_>, font: Font) -> Block {
        if let Some((left, right)) = delimiters(unary.op) {
            self.block_simple_stripped(unary.arg(), font)
                .with_brackets(left, right)
        } else if let Some(styles) = font_styles(unary.op) {
            self.block_simple_stripped(unary.arg(), font.within(styles))
        } else if unary.op == "sqrt" {
            styled("√", font).beside(self.block_simple(self.unwrap_single(unary.arg()), font))
        } else if is_brace(unary.op) {
            self.block_simple_stripped(unary.arg(), font)
                .with_brace(matches!(unary.op, "ubrace" | "underbrace"), None)
        } else {
            one_line(font, |out| self.inline_simpleunary(unary, out))
        }
    }

    fn block_binary(self, binary: &SimpleBinary<'_>, font: Font) -> Block {
        match (binary.op, binary.first(), binary.second()) {
            ("frac", numer, denom) => self.block_simplefrac(numer, denom, font),
            ("root", index, radicand) => self.block_nroot(index, radicand, font),
            ("color" | "id" | "class", _, arg) => self.block_simple_stripped(arg, font),
            ("stackrel" | "overset", over, base) => {
                self.block_set(binary, None, Some(over), base, font)
            }
            ("underset", under, base) => self.block_set(binary, Some(under), None, base, font),
            (op, first, second) => self.block_bgeneric(op, first, second, font),
        }
    }

    /// `op` followed by its two arguments as typed
    fn block_bgeneric(
        self,
        op: &str,
        first: &Simple<'_>,
        second: &Simple<'_>,
        font: Font,
    ) -> Block {
        styled(op, font)
            .with_arg(ArgGap::Spaced, || self.block_simple(first, font))
            .with_arg(ArgGap::Spaced, || self.block_simple(second, font))
    }

    /// The root of `radicand` with `index`; the index is never shown as-is, so its brackets are
    /// always syntax
    fn block_nroot(self, index: &Simple<'_>, radicand: &Simple<'_>, font: Font) -> Block {
        let styled_conf = MapperConf {
            font,
            ..MapperConf::default()
        };
        let root = if let Some(root) = root_char(unwrap_parens(index)) {
            Some(styled(root.encode_utf8(&mut [0; 4]), font))
        } else {
            styled_conf
                .with_shift(Shift::Sup)
                .and_then(|raised| self.mapped_operand(index, raised))
                .map(|index| Block::text(index).beside(styled("√", font)))
        };
        match root {
            Some(root) => root.beside(self.block_simple(self.unwrap_single(radicand), font)),
            None => self.block_bgeneric("root", index, radicand, font),
        }
    }

    /// `base` with `under` below it and `over` above it, as scripts when it is several lines
    /// tall
    fn block_set(
        self,
        binary: &SimpleBinary<'_>,
        under: Option<&Simple<'_>>,
        over: Option<&Simple<'_>>,
        base: &Simple<'_>,
        font: Font,
    ) -> Block {
        let base_block = self.block_simple_stripped(base, font);
        if base_block.is_multiline() {
            self.block_scripts(base_block, under, over, font)
        } else {
            one_line(font, |out| self.inline_simplebinary(binary, out))
        }
    }

    /// `numer` over `denom` as the one vulgar fraction character, signs included, or `None`
    /// when [`vulgar_fracs`][crate::Conf::vulgar_fracs] is off
    ///
    /// Only that form replaces a stack: the superscript over subscript one would stack or not
    /// depending on which letters have script forms.
    fn block_vulgar_frac<'a>(
        self,
        numer: &impl Operand<'a>,
        denom: &impl Operand<'a>,
        font: Font,
    ) -> Option<Block> {
        if self.vulgar_fracs {
            self.signed_vulgar_frac(numer, denom, font)
                .map(|frac| styled(&frac, font))
        } else {
            None
        }
    }

    fn block_simplefrac(self, numer: &Simple<'_>, denom: &Simple<'_>, font: Font) -> Block {
        match self.block_vulgar_frac(numer, denom, font) {
            Some(vulgar) => vulgar,
            None => Block::stack_frac(
                self.block_simple_stripped(numer, font),
                self.block_simple_stripped(denom, font),
            ),
        }
    }

    /// If `strip_brackets` is on and simple is a group, render the inner expression.
    fn block_simple_stripped(self, simple: &Simple<'_>, font: Font) -> Block {
        if let Some(expr) = self.stripped(simple) {
            self.block_expression(expr, font)
        } else {
            self.block_simple(simple, font)
        }
    }

    fn block_frac(self, frac: &Frac<'_>, font: Font) -> Block {
        match self.block_vulgar_frac(&frac.numer, &frac.denom, font) {
            Some(vulgar) => vulgar,
            None => Block::stack_frac(
                self.block_scriptfunc_for_frac(&frac.numer, font),
                self.block_scriptfunc_for_frac(&frac.denom, font),
            ),
        }
    }

    fn block_scriptfunc_for_frac(self, sf: &ScriptFunc<'_>, font: Font) -> Block {
        match sf.as_simple() {
            Some(simple) => self.block_simple_stripped(simple, font),
            None => self.block_scriptfunc(sf, font),
        }
    }

    fn block_group(self, group: &Group<'_>, font: Font) -> Block {
        let inner = match self.group_placeholder(group) {
            Some(chr) => one_line(font, |out| out.write_char(chr)),
            None => self.block_expression(&group.expr, font),
        };
        let left = bracket_str(group.left_bracket);
        let right = bracket_str(group.right_bracket);
        inner.with_brackets(left, right)
    }

    fn block_cell(self, expr: &Expression<'_>, font: Font) -> Block {
        match self.empty_placeholder(expr) {
            Some(chr) => one_line(font, |out| out.write_char(chr)),
            None => self.grid_cell().block_expression(expr, font),
        }
    }

    fn block_matrix(self, matrix: &Matrix<'_>, font: Font) -> Block {
        let num_cols = matrix.num_cols();
        let rules = column_rules(matrix);

        let rows: Vec<Vec<Block>> = matrix
            .rows()
            .map(|row| row.iter().map(|expr| self.block_cell(expr, font)).collect())
            .collect();
        let col_widths: Vec<usize> = (0..num_cols)
            .map(|col| {
                rows.iter()
                    .map(|row| row[col].width)
                    .max()
                    .unwrap_or_default()
            })
            .collect();
        // rows of tall cells are hard to tell apart without a gap
        let spaced = rows.iter().flatten().any(Block::is_multiline);
        let last_row = rows.len() - 1;

        let row_blocks: Vec<Block> = rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let above = row
                    .iter()
                    .map(|cell| cell.baseline)
                    .max()
                    .unwrap_or_default();
                let below = row.iter().map(Block::depth).max().unwrap_or_default()
                    + usize::from(spaced && index != last_row);
                let height = above + below + 1;
                let rule = |boundary| {
                    let line = column_separator(rules[boundary], boundary, num_cols);
                    Block::repeated(line, height, above)
                };
                let mut row_block = rule(0);
                for (col, (cell, &width)) in row.into_iter().zip(&col_widths).enumerate() {
                    if col > 0 {
                        row_block = row_block.beside(rule(col));
                    }
                    row_block = row_block.beside(cell.pad_vertical(above, below).pad_center(width));
                }
                row_block.beside(rule(num_cols))
            })
            .collect();

        let total_width = row_blocks.first().map_or(0, |row| row.width);
        let bar = row_blocks.iter().any(|row| row.bar);
        let grid_lines: Vec<String> = row_blocks.into_iter().flat_map(|row| row.lines).collect();
        let grid = Block {
            baseline: grid_lines.len() / 2,
            width: total_width,
            lines: grid_lines,
            bar,
        };
        let left = bracket_str(matrix.left_bracket);
        let right = bracket_str(matrix.right_bracket);
        grid.with_brackets(left, right)
    }

    fn block_func(self, func: &Func<'_>, font: Font) -> Block {
        let name = styled(func.func, font);
        self.block_scripts(name, func.script.sub(), func.script.sup(), font)
            .with_arg(ArgGap::of_func(func), || {
                self.block_scriptfunc(func.arg(), font)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{Block, Conf, Font};
    use crate::tokens;
    use crate::{Layout, Placeholders};
    use std::fmt::Write;

    fn render_block(input: &str) -> String {
        render_block_conf(input, Conf::default())
    }

    fn render_block_keeping_spaces(input: &str) -> String {
        render_block_conf(input, Conf::default().with_keep_spaces(true))
    }

    fn render_block_spacing_operators(input: &str) -> String {
        render_block_conf(input, Conf::default().with_spaced_operators(true))
    }

    fn render_block_conf(input: &str, conf: Conf) -> String {
        // block_expression is the multi-line layout, whatever conf it is handed
        let conf = Conf {
            layout: Layout::Block,
            ..conf
        };
        let mut out = String::new();
        let expr = tokens::parse(input, conf.keep_spaces);
        write!(out, "{}", conf.block_expression(&expr, Font::default())).unwrap();
        out
    }

    #[test]
    fn block_text() {
        let block = Block::text("hello");
        assert_eq!(block.width, 5);
        assert_eq!(block.baseline, 0);
        assert_eq!(block.height(), 1);
        assert_eq!(format!("{block}"), "hello");
    }

    #[test]
    fn block_beside() {
        let left = Block::text("ab");
        let right = Block::text("cd");
        let result = left.beside(right);
        assert_eq!(result.width, 4);
        assert_eq!(format!("{result}"), "abcd");
    }

    #[test]
    fn block_beside_different_heights() {
        let left = Block {
            lines: vec!["a".to_string(), "b".to_string()],
            baseline: 0,
            width: 1,
            bar: false,
        };
        let right = Block::text("x");
        let result = left.beside(right);
        assert_eq!(result.height(), 2);
        assert_eq!(result.width, 2);
        assert_eq!(format!("{result}"), "ax\nb");
    }

    #[test]
    fn block_stack_frac() {
        let numer = Block::text("x");
        let denom = Block::text("y");
        let frac = Block::stack_frac(numer, denom);
        assert_eq!(frac.height(), 3);
        assert_eq!(frac.baseline, 1);
        assert_eq!(format!("{frac}"), "x\n─\ny");
    }

    #[test]
    fn block_stack_frac_different_widths() {
        let numer = Block::text("abc");
        let denom = Block::text("d");
        let frac = Block::stack_frac(numer, denom);
        assert_eq!(frac.width, 3);
        assert_eq!(format!("{frac}"), "abc\n───\n d");
    }

    #[test]
    fn block_display_strips_trailing_spaces() {
        let block = Block {
            lines: vec!["ab  ".to_string(), "c   ".to_string()],
            baseline: 0,
            width: 4,
            bar: false,
        };
        assert_eq!(format!("{block}"), "ab\nc");
    }

    #[test]
    fn a_space_symbol_at_the_end_stays() {
        assert_eq!(render_block("a quad"), "a\u{2003}");
        assert_eq!(render_block("a thinspace"), "a\u{2009}");
    }

    #[test]
    fn simple_passthrough() {
        assert_eq!(render_block("x"), "x");
        assert_eq!(render_block("42"), "42");
        assert_eq!(render_block("alpha"), "α");
    }

    #[test]
    fn inline_superscript() {
        assert_eq!(render_block("x^2"), "x²");
    }

    #[test]
    fn inline_subscript() {
        assert_eq!(render_block("x_x"), "xₓ");
    }

    #[test]
    fn vulgar_frac_passthrough() {
        assert_eq!(render_block("1/2"), "½");
    }

    #[test]
    fn signed_vulgar_frac_passthrough() {
        assert_eq!(render_block("-1/2"), "-½");
        assert_eq!(render_block("pm 1/2"), "±½");
        assert_eq!(render_block("- -1/2"), "--½");
        assert_eq!(render_block("[[1/2,-1],[3,-1/4]]"), "⎡½  -1⎤\n⎣3  -¼⎦");
        assert_eq!(render_block("x^(-1/2)"), " -½\nx");
    }

    #[test]
    fn signed_fracs_stack_with_sign_on_numerator() {
        assert_eq!(render_block("-3/7"), "-3\n──\n 7");
        assert_eq!(render_block("-x/y"), "-x\n──\n y");
        // a sign on the denominator stays where it was typed
        assert_eq!(render_block("1/-2"), " 1\n──\n-2");
        assert_eq!(render_block("-1/-2"), "-1\n──\n-2");
    }

    #[test]
    fn letter_fracs_stack() {
        // whether letters have script forms doesn't decide the layout
        assert_eq!(render_block("x/n"), "x\n─\nn");
        assert_eq!(render_block("n/2"), "n\n─\n2");
    }

    #[test]
    fn stacked_frac_with_expressions() {
        assert_eq!(render_block_conf("(x+1)/y", stacked()), "x+1\n───\n y");
    }

    #[test]
    fn stacked_frac_denom_expr() {
        assert_eq!(render_block_conf("x/(y+1)", stacked()), " x\n───\ny+1");
    }

    #[test]
    fn placeholders() {
        let conf = Conf::default().with_placeholders(Some(Placeholders::default()));
        assert_eq!(render_block_conf("1/", conf), "1\n─\n□");
        assert_eq!(render_block_conf("sqrt", conf), "√□");
        assert_eq!(render_block_conf("x^", conf), "x⸋");
        assert_eq!(render_block_conf("sum_", conf), "∑▫");
        assert_eq!(render_block_conf("sum_(i=1)^", conf), "∑ᵢ₌₁⸋");
        assert_eq!(render_block_conf("x_y^", conf), " □\nx\n y");
        assert_eq!(render_block_conf("f", conf), "f");
        assert_eq!(render_block_conf("sin", conf), "sin");
        assert_eq!(render_block_conf("sqrt sin", conf), "√sin");
        assert_eq!(render_block_conf("x/(", conf), "x\n─\n□");
        assert_eq!(render_block_conf("(1/2)/(", conf), "½\n─\n□");
        assert_eq!(render_block_conf(")", conf), ")");
        assert_eq!(render_block_conf("(: :)", conf), "⟨□⟩");
        assert_eq!(render_block_conf("\"\"", conf), "");
        assert_eq!(render_block_conf("[[1,2],[3,]]", conf), "⎡1  2⎤\n⎣3  □⎦");
        assert_eq!(render_block_conf("[[],[]]", conf), "⎡□⎤\n⎣□⎦");
        assert_eq!(render_block_conf("[[1,2],[,]]", conf), "⎡1  2⎤\n⎣□  □⎦");
        assert_eq!(render_block_conf("[[1,2],[3,4]]", conf), "⎡1  2⎤\n⎣3  4⎦");
        // the lines of an augmented matrix are untouched
        assert_eq!(
            render_block_conf("[(a,|,b),(c,|,d)]", conf),
            "⎡a │ b⎤\n⎣c │ d⎦"
        );
        assert_eq!(
            render_block_conf("[(a,|,b),(c,|,)]", conf),
            "⎡a │ b⎤\n⎣c │ □⎦"
        );
        let kept = Conf {
            strip_brackets: false,
            ..conf
        };
        assert_eq!(render_block_conf("abs()", kept), "|(□)|");
        assert_eq!(render_block_conf("[[1,2],[", kept), "[[1,2],[□");
        let bare = Conf {
            placeholders: None,
            ..conf
        };
        assert_eq!(render_block_conf("[[1,2],[3,]]", bare), "⎡1  2⎤\n⎣3   ⎦");
        assert_eq!(render_block_conf("[[],[]]", bare), "⎡⎤\n⎣⎦");
        assert_eq!(render_block_conf("[[1,2],[,]]", bare), "⎡1  2⎤\n⎣    ⎦");
        assert_eq!(
            render_block_conf("[(a,|,b),(c,|,d)]", bare),
            "⎡a │ b⎤\n⎣c │ d⎦"
        );
        assert_eq!(
            render_block_conf("[(a,|,b),(c,|,)]", bare),
            "⎡a │ b⎤\n⎣c │  ⎦"
        );
        assert_eq!(render_block("1/"), "1\n─\n");
        assert_eq!(render_block("x^"), "x");
    }

    #[test]
    fn vertical_subscript() {
        let result = render_block("x_y");
        assert_eq!(result, "x\n y");
    }

    #[test]
    fn vertical_superscript() {
        let result = render_block("x^rho");
        assert_eq!(result, " ρ\nx");
    }

    #[test]
    fn group_single_line() {
        assert_eq!(render_block("(x)"), "(x)");
    }

    #[test]
    fn group_multiline_brackets() {
        let result = render_block("(x/y)");
        assert_eq!(result, "⎛x⎞\n⎜─⎟\n⎝y⎠");
    }

    #[test]
    fn brackets_keep_the_baseline_of_what_they_hold() {
        assert_eq!(
            render_block_conf("(x^(a/b))+1", stacked()),
            "⎛ a⎞\n⎜ ─⎟\n⎜ b⎟\n⎝x ⎠+1"
        );
        assert_eq!(render_block("(x_y)+1"), "⎛x ⎞+1\n⎝ y⎠");
    }

    #[test]
    fn a_small_script_on_a_tall_base_sits_at_its_edge() {
        assert_eq!(render_block("(a/b)^2"), "⎛a⎞²\n⎜─⎟\n⎝b⎠");
        assert_eq!(render_block("(a/b)_2"), "⎛a⎞\n⎜─⎟\n⎝b⎠₂");
        assert_eq!(render_block("(a/b)_i^2"), "⎛a⎞²\n⎜─⎟\n⎝b⎠ᵢ");
        assert_eq!(render_block("[[a],[b]]^T"), "⎡a⎤ᵀ\n⎣b⎦");
    }

    #[test]
    fn matrix_simple() {
        assert_eq!(render_block("[[a,b],[c,d]]"), "⎡a  b⎤\n⎣c  d⎦");
    }

    #[test]
    fn frac_plus_term() {
        assert_eq!(render_block("x/y + z"), "x\n─+z\ny");
        assert_eq!(render_block_spacing_operators("x/y + z"), "x\n─ + z\ny");
    }

    #[test]
    fn operator_spacing() {
        assert_eq!(render_block_conf("x + y", stacked()), "x+y");
        assert_eq!(
            render_block_conf("x + y", stacked().with_spaced_operators(true)),
            "x + y"
        );
    }

    #[test]
    fn angle_bracket_height() {
        assert_eq!(
            render_block("<< (x + 1) / x_y >>"),
            "╱x+1╲\n⎜───⎟\n⎜ x ⎟\n╲  y╱"
        );
    }

    #[test]
    fn block_layout_uses_block_rendering() {
        let conf = Conf::default().with_layout(Layout::Block);
        assert_eq!(conf.parse("x/y").to_string(), "x\n─\ny");
        assert_eq!(conf.parse("x/n").to_string(), "x\n─\nn");
    }

    /// A configuration that stacks every fraction
    fn stacked() -> Conf {
        Conf::default().with_vulgar_fracs(false)
    }

    #[test]
    fn tall_square_brackets() {
        assert_eq!(render_block_conf("[x/y]", stacked()), "⎡x⎤\n⎢─⎥\n⎣y⎦");
    }

    #[test]
    fn tall_curly_brackets() {
        assert_eq!(render_block_conf("{x/y}", stacked()), "⎧x⎫\n⎨─⎬\n⎩y⎭");
    }

    #[test]
    fn tall_vertical_bars() {
        assert_eq!(render_block_conf("|x/y|", stacked()), "│x│\n│─│\n│y│");
    }

    #[test]
    fn tall_double_bars() {
        assert_eq!(render_block_conf("||x/y||", stacked()), "‖x‖\n‖─‖\n‖y‖");
        assert_eq!(render_block("||x||"), "||x||");
    }

    #[test]
    fn tall_angle_brackets() {
        assert_eq!(render_block_conf("(:x/y:)", stacked()), "╱x╲\n⎜─⎟\n╲y╱");
    }

    #[test]
    fn tall_brackets_use_uniform_glyphs() {
        // every interior row of a tall paren uses the paren extension ⎜, never the
        // box-drawing │ filler
        assert_eq!(
            render_block_conf("((a/b)/(c/d))", stacked()),
            "⎛ a ⎞\n⎜ ─ ⎟\n⎜ b ⎟\n⎜───⎟\n⎜ c ⎟\n⎜ ─ ⎟\n⎝ d ⎠"
        );
        // ceiling columns stay in the square-bracket family (no stray paren ⎜/⎟)
        assert_eq!(
            render_block_conf("|~ (a/b)/(c/d) ~|", stacked()),
            "⌈ a ⌉\n⎢ ─ ⎥\n⎢ b ⎥\n⎢───⎥\n⎢ c ⎥\n⎢ ─ ⎥\n⎢ d ⎥"
        );
    }

    #[test]
    fn spaced_symbol_operator() {
        // an operator typed as a word is spaced like any other
        assert_eq!(render_block_conf("a/b xx c/d", stacked()), "a c\n─×─\nb d");
        assert_eq!(
            render_block_conf("a/b xx c/d", stacked().with_spaced_operators(true)),
            "a   c\n─ × ─\nb   d"
        );
    }

    #[test]
    fn vertical_superscript_fraction() {
        let result = render_block_conf("x^(a/b)", stacked());
        assert_eq!(result, " a\n ─\n b\nx");
    }

    #[test]
    fn vertical_subscript_fraction() {
        let result = render_block_conf("x_(a/b)", stacked());
        assert_eq!(result, "x\n a\n ─\n b");
    }

    #[test]
    fn vertical_subsuper_fraction() {
        // superscript stacked above, base, subscript stacked below, in one column
        let result = render_block_conf("x_(a/b)^(c/d)", stacked());
        assert_eq!(result, " c\n ─\n d\nx\n a\n ─\n b");
    }

    #[test]
    fn vertical_subsuper_one_column() {
        assert_eq!(render_block("x_b^q"), " q\nx\n b");
        assert_eq!(render_block("x_y^rho"), " ρ\nx\n y");
    }

    #[test]
    fn block_prefix_minus_and_text() {
        assert_eq!(render_block("a !-= -b"), "a≢-b");
        assert_eq!(render_block("x^-1"), "x⁻¹");
        assert_eq!(render_block("text(hello world)"), "hello world");
    }

    #[test]
    fn block_script_brackets_stripped() {
        assert_eq!(render_block("x_(2i)"), "x₂ᵢ");
        assert_eq!(render_block("sum_(i=1)^n i"), "∑ᵢ₌₁ⁿ i");
    }

    #[test]
    fn a_sign_stays_against_the_grid_it_prefixes() {
        assert_eq!(render_block("-[[a,b],[c,d]]"), " ⎡a  b⎤\n-⎣c  d⎦");
        assert_eq!(render_block("sqrt -[[a,b],[c,d]]"), "  ⎡a  b⎤\n√-⎣c  d⎦");
    }

    #[test]
    fn block_signs_and_operators_stay_tight() {
        assert_eq!(render_block("x = -1"), "x=-1");
        assert_eq!(render_block("-x + 1"), "-x+1");
        assert_eq!(render_block("a - b"), "a-b");
        assert_eq!(render_block("a and -b"), "a and -b");
        assert_eq!(render_block("x > 0 or x < -1"), "x>0 or x<-1");
    }

    #[test]
    fn matrix_tall_content() {
        // a spacer row separates rows only when a cell spans several lines
        let result = render_block_conf("[[a/b,c],[d,e/f]]", stacked());
        assert_eq!(
            result,
            "⎡a   ⎤\n⎢─  c⎥\n⎢b   ⎥\n⎢    ⎥\n⎢   e⎥\n⎢d  ─⎥\n⎣   f⎦"
        );
    }

    #[test]
    fn matrix_column_lines() {
        assert_eq!(render_block("[(a,|,b),(c,|,d)]"), "⎡a │ b⎤\n⎣c │ d⎦");
        assert_eq!(render_block("[(|,a,b,|),(|,c,d,|)]"), "⎡│a  b│⎤\n⎣│c  d│⎦");
        assert_eq!(render_block("[(a,|,|,b),(c,|,|,d)]"), "⎡a ││ b⎤\n⎣c ││ d⎦");
    }

    #[test]
    fn matrix_column_line_spans_tall_rows() {
        assert_eq!(
            render_block("[(x/y,|,b),(c,|,d)]"),
            "⎡x │  ⎤\n⎢─ │ b⎥\n⎢y │  ⎥\n⎢  │  ⎥\n⎣c │ d⎦"
        );
    }

    #[test]
    fn matrix_column_widths() {
        assert_eq!(render_block("[[a,xyz],[c,d]]"), "⎡a  xyz⎤\n⎣c   d ⎦");
        assert_eq!(render_block("[[1,-2],[30,4]]"), "⎡ 1  -2⎤\n⎣30   4⎦");
    }

    #[test]
    fn delimiting_commands_wrap_a_stacked_argument() {
        assert_eq!(render_block("floor(x/y)"), "⎢x⎥\n⎢─⎥\n⌊y⌋");
        assert_eq!(render_block("ceil(x/y)"), "⌈x⌉\n⎢─⎥\n⎢y⎥");
        assert_eq!(render_block("abs(x/n)"), "│x│\n│─│\n│n│");
        assert_eq!(render_block("norm(x/y)"), "‖x‖\n‖─‖\n‖y‖");
        assert_eq!(render_block("abs x"), "|x|");
    }

    #[test]
    fn commands_stack_their_argument() {
        assert_eq!(render_block("bb(x/n)"), "𝐱\n─\n𝐧");
        assert_eq!(render_block("bb(x^2)"), " 𝟐\n𝐱");
        assert_eq!(render_block("root(3)(x/y)"), " ⎛x⎞\n∛⎜─⎟\n ⎝y⎠");
        assert_eq!(render_block("root(n)(x/y)"), "  ⎛x⎞\nⁿ√⎜─⎟\n  ⎝y⎠");
        assert_eq!(
            render_block("root(q)(x/y)"),
            "         ⎛x⎞\nroot (q) ⎜─⎟\n         ⎝y⎠"
        );
        assert_eq!(render_block("sqrt((frac(x)(y)))"), " ⎛x⎞\n√⎜─⎟\n ⎝y⎠");
        for input in ["color(red)(x/y)", "id(a)(x/y)", "class(a)(x/y)"] {
            assert_eq!(render_block(input), "x\n─\ny", "{input}");
        }
        assert_eq!(render_block("overset(a)(x/y)"), "xᵃ\n─\ny");
        assert_eq!(render_block("underset(a)(x/y)"), "x\n─\nyₐ");
        // an argument on one line is written as in the one-line layouts
        assert_eq!(render_block("root(3)(x)"), "∛x");
        assert_eq!(render_block("overset(a)(x)"), "x\u{363}");
        assert_eq!(render_block("bb(1/2)"), "𝟏\n─\n𝟐");
    }

    #[test]
    fn a_lone_combining_mark_is_written() {
        assert_eq!(render_block("\"\u{301}\""), "\u{301}");
        assert_eq!(render_block("hat \"\u{301}\""), "\u{301}\u{302}");
    }

    #[test]
    fn simple_func_in_block() {
        // a function applied to a multiline argument renders beside it.
        let result = render_block_conf("sin(x/y)", stacked());
        assert_eq!(result, "   ⎛x⎞\nsin⎜─⎟\n   ⎝y⎠");
        // an argument that does not hug the name is set off by a space
        assert_eq!(render_block_conf("x^sin Q", stacked()), " sin Q\nx");
    }

    #[test]
    fn spaced_operators_spelled_out() {
        let conf = Conf::default()
            .with_layout(Layout::Block)
            .with_spaced_operators(true);
        assert_eq!(conf.parse("a |-> b").to_string(), "a ↦ b");
        assert_eq!(conf.parse("a mapsto b").to_string(), "a ↦ b");
        assert_eq!(conf.parse("a cdot b").to_string(), "a ⋅ b");
    }

    #[test]
    fn block_sqrt_multiline() {
        // sqrt of a stacked fraction renders the radical beside the block
        let result = render_block_conf("sqrt(x/y)", stacked());
        assert_eq!(result, " ⎛x⎞\n√⎜─⎟\n ⎝y⎠");
    }

    #[test]
    fn block_sqrt_inline() {
        // sqrt of a single-line argument stays inline
        assert_eq!(render_block_conf("sqrt(x)", stacked()), "√x");
        assert_eq!(render_block_conf("sqrt(x+1)", stacked()), "√(x+1)");
    }

    #[test]
    fn block_binary_root() {
        // a non-frac binary falls back to inline rendering
        assert_eq!(render_block_conf("root(3)(x)", stacked()), "∛x");
        assert_eq!(render_block_conf("root(5)(x)", stacked()), "⁵√x");
    }

    #[test]
    fn block_inline_subsuper() {
        // sub/superscriptable scripts render inline even in block mode
        assert_eq!(render_block_conf("x_a^b", stacked()), "xₐᵇ");
    }

    #[test]
    fn block_empty_input() {
        assert_eq!(render_block_conf("", stacked()), "");
    }

    #[test]
    fn block_frac_scriptfunc_numerator() {
        // a numerator with a script stacks like any other
        assert_eq!(render_block_conf("x^2/y", stacked()), "x²\n──\n y");
        assert_eq!(render_block_conf("x_i/y", stacked()), "xᵢ\n──\n y");
    }

    #[test]
    fn block_nested_frac() {
        // the outer bar overhangs so it can't be mistaken for an inner one
        let result = render_block_conf("(a/b)/(c/d)", stacked());
        assert_eq!(result, " a\n ─\n b\n───\n c\n ─\n d");
    }

    #[test]
    fn a_quoted_bar_is_not_a_fraction_bar() {
        assert_eq!(render_block("\"─\"/x"), "─\n─\nx");
        // a fraction in a grid still makes the bar over it overhang
        assert_eq!(render_block("[[a/b]]/c"), " ⎡a⎤\n ⎢─⎥\n ⎣b⎦\n─────\n  c");
    }

    #[test]
    fn invisible_brackets() {
        // `{:` and `:}` are invisible grouping brackets
        assert_eq!(render_block_conf("{:x/y:}", stacked()), "x\n─\ny");
        // with one side invisible, only the other is drawn
        assert_eq!(render_block_conf("(:x/y:}", stacked()), "╱x\n⎜─\n╲y");
        assert_eq!(render_block_conf("{:x/y:)", stacked()), "x╲\n─⎟\ny╱");
    }

    #[test]
    fn applied_identifier_in_stacked_frac() {
        // numerator and denominator are identifiers applied to bracketed arguments
        let result = render_block_conf("f(x)/g(y)", stacked());
        assert_eq!(result, "f(x)\n────\ng(y)");
    }

    #[test]
    fn block_word_spacing() {
        assert_eq!(render_block("sin(x)"), "sin(x)");
        assert_eq!(render_block("dx {: :} dy"), "dx dy");
        assert_eq!(render_block("a mod b"), "a mod b");
        assert_eq!(render_block("a and b"), "a and b");
        assert_eq!(render_block("dx - dy"), "dx-dy");
        assert_eq!(render_block("int_0^1 f(x) dx"), "∫₀¹ f(x) dx");
    }

    #[test]
    fn explicit_frac_binary_in_block() {
        // the `frac(a)(b)` binary form stacks just like infix `a/b`
        assert_eq!(render_block_conf("frac(a)(b)", stacked()), "a\n─\nb");
    }

    #[test]
    fn bare_function_no_trailing_space() {
        // a bare function name with no argument renders without a separator
        assert_eq!(render_block("g"), "g");
        assert_eq!(render_block("g h"), "g h");
    }

    #[test]
    fn nothing_to_render_leaves_no_space() {
        assert_eq!(render_block("dx g obrace() dy"), "dx g dy");
    }

    #[test]
    fn a_space_in_a_script_steps_narrower() {
        assert_eq!(render_block("x^sin x"), "xˢⁱⁿ\u{2009}ˣ");
        assert_eq!(render_block("x^(a quad b)"), "xᵃ\u{2002}ᵇ");
        assert_eq!(render_block("x^(a \\ b)"), "xᵃ\u{2009}ᵇ");
        assert_eq!(render_block("x^text(a\tb)"), "xᵃ\u{2009}ᵇ");
        assert_eq!(render_block("x_(i enspace j)"), "xᵢ\u{2004}ⱼ");
        // a narrower space still takes a whole cell, so the bar spans it
        assert_eq!(
            render_block_conf("x^(a quad b)/y", stacked()),
            "xᵃ\u{2002}ᵇ\n────\n  y"
        );
        assert_eq!(
            render_block("[[x^(a quad b),c],[d,e]]"),
            "⎡xᵃ\u{2002}ᵇ  c⎤\n⎣  d   e⎦"
        );
    }

    #[test]
    fn kept_spaces_do_not_replace_the_usual_spacing() {
        assert_eq!(render_block_keeping_spaces("a+b"), "a+b");
        assert_eq!(render_block_keeping_spaces("a  +b"), "a  +b");
        assert_eq!(render_block_keeping_spaces("x / y   z"), "x\n─   z\ny");
    }

    #[test]
    fn quoted_tabs_and_newlines_are_plain_spaces() {
        assert_eq!(render_block("z+\"a\nb\"/c"), "  a b\nz+───\n   c");
        assert_eq!(render_block("\"a\tb\"/c"), "a b\n───\n c");
        assert_eq!(
            render_block("\"a\u{2028}b\u{2029}c\"/d"),
            "a b c\n─────\n  d"
        );
    }

    #[test]
    fn quoted_control_chars_are_replaced() {
        assert_eq!(render_block("\"a\u{1b}b\"/c"), "a\u{fffd}b\n───\n c");
        assert_eq!(
            render_block("\"a\u{7f}\u{9b}\"/c"),
            "a\u{fffd}\u{fffd}\n───\n c"
        );
    }

    #[test]
    fn spaces_are_dropped_by_default() {
        assert_eq!(render_block("a  b"), "ab");
        assert_eq!(render_block("a\tb"), "ab");
        assert_eq!(render_block("[[a  b,c],[d,e]]"), "⎡ab  c⎤\n⎣ d  e⎦");
    }

    #[test]
    fn kept_spaces_are_as_wide_as_typed() {
        assert_eq!(render_block_keeping_spaces("a  b"), "a  b");
        assert_eq!(render_block_keeping_spaces("a\tb"), "a b");
        assert_eq!(render_block_keeping_spaces("a\nb"), "a b");
        // the one-line rendering of a nested part is placed as one line
        assert_eq!(
            render_block_keeping_spaces("sqrt(a\nb)/c"),
            "√(a b)\n──────\n   c"
        );
        assert_eq!(
            render_block_keeping_spaces("(x^(a\tb))/y"),
            "xᵃ\u{2009}ᵇ\n────\n  y"
        );
    }

    #[test]
    fn a_kept_space_after_a_prefixing_sign_is_dropped() {
        assert_eq!(render_block_keeping_spaces("- x"), "-x");
        assert_eq!(render_block_keeping_spaces("+ x"), "+x");
        assert_eq!(render_block_keeping_spaces("+- x"), "±x");
        assert_eq!(render_block_keeping_spaces("pm x"), "±x");
        assert_eq!(render_block_keeping_spaces("-+ x"), "∓x");
        assert_eq!(render_block_keeping_spaces("mp x"), "∓x");
        assert_eq!(render_block_keeping_spaces("1 + - x"), "1 + -x");
        assert_eq!(render_block_keeping_spaces("a (- b)"), "a (-b)");
        assert_eq!(render_block_keeping_spaces("- (a + b)"), "-(a + b)");
        assert_eq!(render_block_keeping_spaces("- \t x"), "-x");
        assert_eq!(render_block_keeping_spaces("(- x)/2"), "-x\n──\n 2");
        assert_eq!(render_block_keeping_spaces("x^(- a)"), "x⁻ᵃ");
        assert_eq!(render_block_keeping_spaces("x_(- a)"), "x₋ₐ");
        assert_eq!(
            render_block_keeping_spaces("[[- a, b], [c, - d]]"),
            "⎡-a   b⎤\n⎣ c  -d⎦"
        );
        // the sign prefixes an operand that isn't there, and the space before it goes anyway
        assert_eq!(render_block_keeping_spaces("-"), "-");
        assert_eq!(render_block_keeping_spaces("- "), "-");
    }

    #[test]
    fn a_kept_space_around_a_binary_sign_stays() {
        assert_eq!(render_block_keeping_spaces("a - b"), "a - b");
        assert_eq!(render_block_keeping_spaces("a  -  b"), "a  -  b");
        assert_eq!(render_block_keeping_spaces("- x + y"), "-x + y");
        // the second sign follows an operator, so only the space after it goes
        assert_eq!(render_block_keeping_spaces("a - - b"), "a - -b");
        // the scripts don't stand in for the big operator they sit on, so the sign prefixes
        assert_eq!(render_block_keeping_spaces("sum_(i=1)^n - i"), "∑ᵢ₌₁ⁿ -i");
    }

    #[test]
    fn kept_spaces_inside_a_grid_are_dropped() {
        assert_eq!(
            render_block_keeping_spaces("[[a  b,c],[d  e,f]]"),
            "⎡ab  c⎤\n⎣de  f⎦"
        );
        assert_eq!(
            render_block_keeping_spaces("[[sqrt(a\nb),c],[d,e]]"),
            "⎡√(ab)  c⎤\n⎣  d    e⎦"
        );
    }

    #[test]
    fn kept_spaces_outlive_what_renders_to_nothing() {
        assert_eq!(render_block_keeping_spaces("a  obrace() b"), "a   b");
        assert_eq!(render_block_keeping_spaces("a  obrace()  b"), "a    b");
    }

    #[test]
    fn operators_join_their_operands_by_default() {
        assert_eq!(render_block("a+b"), "a+b");
        assert_eq!(render_block("a+b=c"), "a+b=c");
        assert_eq!(render_block("x<=y"), "x≤y");
        assert_eq!(render_block("a in A"), "a∈A");
    }

    #[test]
    fn spaced_operators_get_a_space_on_either_side() {
        assert_eq!(render_block_spacing_operators("a+b"), "a + b");
        assert_eq!(render_block_spacing_operators("a+b=c"), "a + b = c");
        assert_eq!(render_block_spacing_operators("x<=y"), "x ≤ y");
        assert_eq!(render_block_spacing_operators("a in A"), "a ∈ A");
    }

    #[test]
    fn a_spaced_sign_still_hugs_its_operand() {
        assert_eq!(render_block_spacing_operators("-x"), "-x");
        assert_eq!(render_block_spacing_operators("-x+1"), "-x + 1");
        assert_eq!(render_block_spacing_operators("x = -1"), "x = -1");
        assert_eq!(render_block_spacing_operators("1 - - x"), "1 - -x");
        // `:=` gets no spaces of its own yet, but the sign after it still hugs
        assert_eq!(render_block_spacing_operators("a := -b"), "a:=-b");
    }

    #[test]
    fn spaced_operators_leave_word_spacing_alone() {
        assert_eq!(render_block_spacing_operators("a and b"), "a and b");
        assert_eq!(render_block_spacing_operators("a and -b"), "a and -b");
        assert_eq!(render_block_spacing_operators("x mod y"), "x mod y");
        assert_eq!(render_block_spacing_operators("sinx+1"), "sin x + 1");
        assert_eq!(
            render_block_spacing_operators("x > 0 or x < -1"),
            "x > 0 or x < -1"
        );
    }

    #[test]
    fn a_spaced_operator_in_a_script_gets_no_space() {
        assert_eq!(render_block("x^(a+b)"), "xᵃ⁺ᵇ");
        assert_eq!(render_block("x_(i+1)"), "xᵢ₊₁");
        assert_eq!(render_block_spacing_operators("x^(a+b)"), "xᵃ⁺ᵇ");
        assert_eq!(render_block_spacing_operators("x_(i+1)"), "xᵢ₊₁");
        assert_eq!(render_block_spacing_operators("sum_(i=1)^n i"), "∑ᵢ₌₁ⁿ i");
        // a script with no raised form is a line of its own at full size, where a space holds up
        assert_eq!(render_block_spacing_operators("x^(a+Z)"), " a + Z\nx");
        let brackets = Conf::default()
            .with_spaced_operators(true)
            .with_strip_brackets(false);
        assert_eq!(render_block_conf("x^(a+b)", brackets), "x⁽ᵃ⁺ᵇ⁾");
    }

    #[test]
    fn a_script_keeps_the_spaces_the_option_did_not_add() {
        assert_eq!(render_block_spacing_operators("x^sin x"), "xˢⁱⁿ\u{2009}ˣ");
        assert_eq!(
            render_block_spacing_operators("x^(a quad b)"),
            "xᵃ\u{2002}ᵇ"
        );
        assert_eq!(render_block_spacing_operators("x_sin x"), "xₛᵢₙ\u{2009}ₓ");
        let typed = Conf::default()
            .with_keep_spaces(true)
            .with_spaced_operators(true);
        assert_eq!(render_block_conf("x^(a b)", typed), "xᵃ\u{2009}ᵇ");
        assert_eq!(
            render_block_conf("x^(a  +b)", typed),
            "xᵃ\u{2009}\u{2009}⁺ᵇ"
        );
        assert_eq!(
            render_block_conf("x_(i  =1)", typed),
            "xᵢ\u{2009}\u{2009}₌₁"
        );
    }

    #[test]
    fn spaced_operators_inside_a_grid() {
        assert_eq!(render_block("[[a+b,c],[d,e-f]]"), "⎡a+b   c ⎤\n⎣ d   e-f⎦");
        assert_eq!(
            render_block_spacing_operators("[[a+b,c],[d,e-f]]"),
            "⎡a + b    c  ⎤\n⎣  d    e - f⎦"
        );
        assert_eq!(
            render_block_spacing_operators("[(a+b,|,c),(d,|,e-f)]"),
            "⎡a + b │   c  ⎤\n⎣  d   │ e - f⎦"
        );
        // the multi-line layout drops what was typed inside a grid, so this decides there
        let typed = Conf::default()
            .with_keep_spaces(true)
            .with_spaced_operators(true);
        assert_eq!(
            render_block_conf("[[a  +b,c],[d,e]]", typed),
            "⎡a + b  c⎤\n⎣  d    e⎦"
        );
    }

    #[test]
    fn spaced_operators_yield_to_what_was_typed() {
        let conf = Conf::default()
            .with_keep_spaces(true)
            .with_spaced_operators(true);
        assert_eq!(render_block_conf("a+b", conf), "a + b");
        assert_eq!(render_block_conf("a  +b", conf), "a  + b");
        assert_eq!(render_block_conf("a  +  b", conf), "a  +  b");
        assert_eq!(render_block_conf("1 + - x", conf), "1 + -x");
    }

    #[test]
    fn spaced_operators_around_a_placeholder() {
        let conf = Conf::default()
            .with_spaced_operators(true)
            .with_placeholders(Some(Placeholders::default()));
        assert_eq!(render_block_conf("a+()", conf), "a + □");
        // the mark has no raised form, so the script becomes a line of its own at full size
        assert_eq!(render_block_conf("x^(a+())", conf), " a + □\nx");
    }

    #[test]
    fn a_spaced_sign_hugs_after_a_comma_or_a_bare_big_operator() {
        assert_eq!(render_block_spacing_operators("(1,-2)"), "(1,-2)");
        assert_eq!(render_block_spacing_operators("f(x,-y)"), "f(x,-y)");
        assert_eq!(render_block_spacing_operators("x_(1,-2)"), "x₁,₋₂");
        assert_eq!(render_block_spacing_operators("sum -x"), "∑-x");
        assert_eq!(render_block_spacing_operators("int -x"), "∫-x");
        assert_eq!(render_block_spacing_operators("prod +x"), "∏+x");
        // the scripts don't stand in for the big operator they sit on, so the sign prefixes
        assert_eq!(
            render_block_spacing_operators("sum_(i=1)^n - i"),
            "∑ᵢ₌₁ⁿ -i"
        );
    }

    #[test]
    fn a_brace_is_drawn_as_wide_as_its_argument() {
        assert_eq!(render_block("ubrace(a)"), "a\n┬");
        assert_eq!(render_block("ubrace(ab)"), "ab\n╰╯");
        assert_eq!(render_block("ubrace(abc)"), "abc\n╰┬╯");
        assert_eq!(render_block("ubrace(abcd)"), "abcd\n╰╮╭╯");
        assert_eq!(render_block("underbrace(abcde)"), "abcde\n╰─┬─╯");
        assert_eq!(render_block("ubrace(abcdef)"), "abcdef\n╰─╮╭─╯");
        assert_eq!(render_block("obrace(a)"), "┴\na");
        assert_eq!(render_block("obrace(ab)"), "╭╮\nab");
        assert_eq!(render_block("obrace(abc)"), "╭┴╮\nabc");
        assert_eq!(render_block("overbrace(abcd)"), "╭╯╰╮\nabcd");
        assert_eq!(render_block("obrace(abcde)"), "╭─┴─╮\nabcde");
        // a wide char takes two columns of brace
        assert_eq!(render_block("ubrace(\"😀\")"), "😀\n╰╯");
    }

    #[test]
    fn a_brace_label_is_centered_beyond_it() {
        assert_eq!(render_block("ubrace(a+b)_c"), "a+b\n╰┬╯\n c");
        assert_eq!(render_block("obrace(a+b)^c"), " c\n╭┴╮\na+b");
        assert_eq!(render_block("ubrace(x)_n"), "x\n┬\nn");
        assert_eq!(render_block("ubrace(ab)_(n+1)"), " ab\n ╰╯\nn+1");
        assert_eq!(render_block("obrace(abcd)^n"), "  n\n╭╯╰╮\nabcd");
        assert_eq!(render_block("ubrace(x)_(a+b+c)"), "  x\n  ┬\na+b+c");
        assert_eq!(render_block("ubrace(a)_(b/c)"), "a\n┬\nb\n─\nc");
    }

    #[test]
    fn a_braced_block_keeps_its_arguments_baseline() {
        assert_eq!(render_block("ubrace(x/y)"), "x\n─\ny\n┬");
        assert_eq!(render_block("obrace(x/y)"), "┴\nx\n─\ny");
        assert_eq!(render_block("ubrace(x/y)_n"), "x\n─\ny\n┬\nn");
        assert_eq!(
            render_block("a+ubrace(x/y)_n+b"),
            "  x\na+─+b\n  y\n  ┬\n  n"
        );
        assert_eq!(render_block("a+obrace(b)^n"), "  n\n  ┴\na+b");
    }

    #[test]
    fn the_other_script_on_a_brace_keeps_its_place() {
        assert_eq!(render_block("ubrace(a+b)_c^d"), "a+bᵈ\n╰┬╯\n c");
        assert_eq!(render_block("obrace(a+b)_2^c"), " c\n╭┴╮\na+b₂");
        assert_eq!(render_block("ubrace(a+b)^d"), "a+bᵈ\n╰┬╯");
        assert_eq!(render_block("obrace(a+b)_2"), "╭┴╮\na+b₂");
    }
}
