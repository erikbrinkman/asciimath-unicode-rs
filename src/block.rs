#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]

use std::{fmt, iter, mem};
use unicode_width::UnicodeWidthStr;

use super::Conf;
use super::ast::{
    frac_edges, func_hugs_argument, hugs_argument, inter_is_spaced_op, is_operator_to_a_sign,
    is_unary_sign, name_without_argument, needs_space, scriptfunc_edges,
};
use super::inline::{Mapper, MapperConf, Operand, column_rules};
use super::tokens::{left_bracket_str, right_bracket_str, subscript_char, superscript_char};

use asciimath_parser::tree::{
    Expression, Frac, Func, Group, Intermediate, Matrix, Script, ScriptFunc, Simple, SimpleBinary,
    SimpleFunc, SimpleScript, SimpleUnary,
};

/// A 2D text block for multi-line rendering.
/// All lines are padded to `width` display columns with trailing spaces.
#[derive(Debug, Clone)]
pub struct Block {
    lines: Vec<String>,
    baseline: usize,
    width: usize,
}

impl Block {
    fn text(text: impl Into<String>) -> Self {
        let owned = text.into();
        let width = UnicodeWidthStr::width(&*owned);
        Block {
            lines: vec![owned],
            baseline: 0,
            width,
        }
    }

    fn empty() -> Self {
        Block {
            lines: vec![String::new()],
            baseline: 0,
            width: 0,
        }
    }

    fn space(n: usize) -> Self {
        Block {
            lines: vec![" ".repeat(n)],
            baseline: 0,
            width: n,
        }
    }

    /// The same line repeated to fill a block `height` tall
    fn repeated(line: String, height: usize, baseline: usize) -> Self {
        Block {
            width: UnicodeWidthStr::width(&*line),
            lines: vec![line; height],
            baseline,
        }
    }

    fn height(&self) -> usize {
        self.lines.len()
    }

    fn has_bar(&self) -> bool {
        self.lines.iter().any(|line| line.contains('─'))
    }

    fn is_multiline(&self) -> bool {
        self.lines.len() > 1
    }

    fn beside(mut self, mut other: Self) -> Self {
        let above = self.baseline.max(other.baseline);
        let self_below = self.lines.len() - self.baseline;
        let other_below = other.lines.len() - other.baseline;
        let below = self_below.max(other_below);
        let new_width = self.width + other.width;

        // Pad self above and below to align baselines
        self.lines = iter::repeat_with(|| " ".repeat(self.width))
            .take(above - self.baseline)
            .chain(self.lines)
            .chain(iter::repeat_with(|| " ".repeat(self.width)).take(below - self_below))
            .collect();

        // Pad other above and below to align baselines
        other.lines = iter::repeat_with(|| " ".repeat(other.width))
            .take(above - other.baseline)
            .chain(other.lines)
            .chain(iter::repeat_with(|| " ".repeat(other.width)).take(below - other_below))
            .collect();

        // Zip and concat
        let lines = self
            .lines
            .into_iter()
            .zip(other.lines)
            .map(|(mut left, right)| {
                left.push_str(&right);
                left
            })
            .collect();

        Block {
            lines,
            baseline: above,
            width: new_width,
        }
    }

    fn stack_frac(numer: Self, denom: Self) -> Self {
        // a nested fraction's bar would be as long as this one without the overhang
        let overhang = if numer.has_bar() || denom.has_bar() {
            2
        } else {
            0
        };
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
            let new_baseline = self.height() / 2;
            left_col
                .beside(self)
                .beside(right_col)
                .with_baseline(new_baseline)
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
        };
        self.beside(column)
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
        let cur_below = self.lines.len() - 1 - self.baseline;
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
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, line) in self.lines.iter().enumerate() {
            if idx > 0 {
                f.write_str("\n")?;
            }
            f.write_str(line.trim_end())?;
        }
        Ok(())
    }
}

fn center_pad(s: &str, current_width: usize, target_width: usize) -> String {
    if current_width >= target_width {
        s.to_string()
    } else {
        let left = (target_width - current_width).div_ceil(2);
        let right = target_width - current_width - left;
        format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
    }
}

/// Glyphs for a tall bracket: top, the two middle rows, bottom, and the fill between them
type BracketPieces = (char, char, char, char, char);

fn left_bracket_pieces(bracket: &str, height: usize) -> BracketPieces {
    match bracket {
        "(" | "left(" => ('⎛', '⎜', '⎜', '⎝', '⎜'),
        "[" | "left[" => ('⎡', '⎢', '⎢', '⎣', '⎢'),
        "⟨" | "(:" | "langle" | "<<" => ('╱', '⎜', '⎜', '╲', '⎜'),
        "⌊" | "|__" | "lfloor" => ('⎢', '⎢', '⎢', '⌊', '⎢'),
        "⌈" | "|~" | "lceiling" => ('⌈', '⎢', '⎢', '⎢', '⎢'),
        "{" if height == 2 => ('⎰', ' ', ' ', '⎱', ' '),
        "{" if height.is_multiple_of(2) => ('⎧', '⎭', '⎫', '⎩', '⎪'),
        "{" => ('⎧', '⎪', '⎨', '⎩', '⎪'),
        // "|", "|:", and anything else
        _ => ('│', '│', '│', '│', '│'),
    }
}

fn right_bracket_pieces(bracket: &str, height: usize) -> BracketPieces {
    match bracket {
        ")" | "right)" => ('⎞', '⎟', '⎟', '⎠', '⎟'),
        "]" | "right]" => ('⎤', '⎥', '⎥', '⎦', '⎥'),
        "⟩" | ":)" | "rangle" | ">>" => ('╲', '⎟', '⎟', '╱', '⎟'),
        "⌋" | "__|" | "rfloor" => ('⎥', '⎥', '⎥', '⌋', '⎥'),
        "⌉" | "~|" | "rceiling" => ('⌉', '⎥', '⎥', '⎥', '⎥'),
        "}" if height == 2 => ('⎱', ' ', ' ', '⎰', ' '),
        "}" if height.is_multiple_of(2) => ('⎫', '⎩', '⎧', '⎭', '⎪'),
        "}" => ('⎫', '⎪', '⎬', '⎭', '⎪'),
        // "|", ":|", and anything else
        _ => ('│', '│', '│', '│', '│'),
    }
}

fn tall_bracket(bracket: &str, height: usize, pieces: fn(&str, usize) -> BracketPieces) -> Block {
    if bracket.is_empty() {
        Block {
            lines: vec![String::new(); height],
            baseline: height / 2,
            width: 0,
        }
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

impl Conf {
    fn block_inline_simple(self, simple: &Simple<'_>) -> Block {
        let mut s = String::new();
        self.inline_simple(simple, &mut Mapper::new(&mut s))
            .unwrap_or_else(|_| unreachable!("write to String is infallible"));
        Block::text(s)
    }

    pub(crate) fn block_expression(self, expr: &Expression<'_>) -> Block {
        // the runs typed on both sides of an item that renders to nothing both belong in the output
        let mut typed_width = 0;
        let mut items = expr.iter().filter_map(|inter| {
            let rendered = match inter {
                Intermediate::Space(space) => {
                    if self.keep_spaces {
                        typed_width += UnicodeWidthStr::width(*space);
                    }
                    None
                }
                Intermediate::ScriptFunc(func) => {
                    Some((self.block_scriptfunc(func), scriptfunc_edges(func)))
                }
                Intermediate::Frac(frac) => Some((self.block_frac(frac), frac_edges(frac))),
            };
            // an item that renders to nothing, like an empty group, must not leave a space behind
            rendered
                .filter(|(block, _)| block.width > 0)
                .map(|(block, edges)| (inter, block, edges, mem::take(&mut typed_width)))
        });
        let Some((first, first_block, (_, first_trailing_edge), _)) = items.next() else {
            return Block::empty();
        };
        let mut result = first_block;
        let mut prev_edge = first_trailing_edge;
        let mut prev_is_operator = is_operator_to_a_sign(first, first_trailing_edge);
        let mut prev_unary_sign = is_unary_sign(first, None);
        let mut prev_binary_op = inter_is_spaced_op(first) && !prev_unary_sign;
        for (inter, block, (leading_edge, trailing_edge), typed_width) in items {
            let is_op = inter_is_spaced_op(inter);
            let unary_sign = is_unary_sign(inter, Some(prev_is_operator));
            let binary_op = is_op && !unary_sign;
            // a run typed after a unary sign is dropped: the sign hugs its operand
            if typed_width > 0 && !prev_unary_sign {
                result = result.beside(Block::space(typed_width));
            } else if prev_binary_op || binary_op || needs_space(prev_edge, leading_edge) {
                result = result.beside(Block::space(1));
            }
            result = result.beside(block);
            prev_edge = trailing_edge;
            prev_is_operator = is_operator_to_a_sign(inter, trailing_edge);
            prev_unary_sign = unary_sign;
            prev_binary_op = binary_op;
        }
        result
    }

    fn block_scriptfunc(self, sf: &ScriptFunc<'_>) -> Block {
        match sf {
            ScriptFunc::Simple(ss) => self.block_simplescript(ss),
            ScriptFunc::Func(func) => self.block_func(func),
        }
    }

    fn block_simplescript(self, ss: &SimpleScript<'_>) -> Block {
        let base_block = self.block_simple(&ss.simple);
        self.block_apply_script(base_block, &ss.script)
    }

    fn block_apply_script(self, base: Block, script: &Script<'_>) -> Block {
        let lower_conf = Some(MapperConf {
            sub_sup: Some(subscript_char),
            ..MapperConf::default()
        });
        let upper_conf = Some(MapperConf {
            sub_sup: Some(superscript_char),
            ..MapperConf::default()
        });
        match script {
            Script::None => base,
            Script::Sub(sub) => match self.mapped_script(sub, lower_conf, self.sub_placeholder()) {
                Some(text) => base.beside(Block::text(text)),
                None => base.with_scripts(Some(self.block_simple_stripped(sub)), None),
            },
            Script::Super(sup) => match self.mapped_script(sup, upper_conf, self.sup_placeholder())
            {
                Some(text) => base.beside(Block::text(text)),
                None => base.with_scripts(None, Some(self.block_simple_stripped(sup))),
            },
            Script::Subsuper(sub, sup) => {
                if let Some(lower) = self.mapped_script(sub, lower_conf, self.sub_placeholder())
                    && let Some(upper) = self.mapped_script(sup, upper_conf, self.sup_placeholder())
                {
                    base.beside(Block::text(format!("{lower}{upper}")))
                } else {
                    base.with_scripts(
                        Some(self.block_simple_stripped(sub)),
                        Some(self.block_simple_stripped(sup)),
                    )
                }
            }
        }
    }

    fn block_simple(self, simple: &Simple<'_>) -> Block {
        match self.placeholder(simple) {
            Some(chr) => Block::text(chr),
            None => match simple {
                Simple::Missing => Block::empty(),
                Simple::Group(group) => self.block_group(group),
                Simple::Matrix(matrix) => self.block_matrix(matrix),
                Simple::Unary(unary) => self.block_unary(unary),
                Simple::Binary(binary) => self.block_binary(binary),
                Simple::Func(func) => self.block_simplefunc(func),
                _ => self.block_inline_simple(simple),
            },
        }
    }

    fn block_simplefunc(self, func: &SimpleFunc<'_>) -> Block {
        let name = Block::text(func.func);
        let arg = if name_without_argument(func.arg()) {
            Block::empty()
        } else {
            self.block_simple(func.arg())
        };
        if hugs_argument(func.arg()) || arg.width == 0 {
            name.beside(arg)
        } else {
            name.beside(Block::space(1)).beside(arg)
        }
    }

    fn block_unary(self, unary: &SimpleUnary<'_>) -> Block {
        if unary.op == "sqrt"
            && let arg = self.block_simple(unary.arg())
            && arg.is_multiline()
        {
            Block::text("√").beside(arg)
        } else {
            let mut s = String::new();
            let mut mapper = Mapper::new(&mut s);
            self.inline_simpleunary(unary, &mut mapper)
                .unwrap_or_else(|_| unreachable!("write to String is infallible"));
            Block::text(s)
        }
    }

    fn block_binary(self, binary: &SimpleBinary<'_>) -> Block {
        if binary.op == "frac" {
            self.block_simplefrac(binary.first(), binary.second())
        } else {
            let mut s = String::new();
            let mut mapper = Mapper::new(&mut s);
            self.inline_simplebinary(binary, &mut mapper)
                .unwrap_or_else(|_| unreachable!("write to String is infallible"));
            Block::text(s)
        }
    }

    fn block_simplefrac(self, numer: &Simple<'_>, denom: &Simple<'_>) -> Block {
        // script fractions would stack or not depending on which letters have script forms
        if self.vulgar_fracs
            && let Some(frac) = super::ast::extract_vulgar_frac(numer, denom, self.strip_brackets)
        {
            Block::text(frac)
        } else {
            Block::stack_frac(
                self.block_simple_stripped(numer),
                self.block_simple_stripped(denom),
            )
        }
    }

    /// If `strip_brackets` is on and simple is a group, render the inner expression.
    fn block_simple_stripped(self, simple: &Simple<'_>) -> Block {
        if let Some(expr) = self.stripped(simple) {
            self.block_expression(expr)
        } else {
            self.block_simple(simple)
        }
    }

    fn block_frac(self, frac: &Frac<'_>) -> Block {
        if let Some(num) = frac.numer.as_simple()
            && let Some(den) = frac.denom.as_simple()
        {
            self.block_simplefrac(num, den)
        } else {
            Block::stack_frac(
                self.block_scriptfunc_for_frac(&frac.numer),
                self.block_scriptfunc_for_frac(&frac.denom),
            )
        }
    }

    fn block_scriptfunc_for_frac(self, sf: &ScriptFunc<'_>) -> Block {
        match sf.as_simple() {
            Some(simple) => self.block_simple_stripped(simple),
            None => self.block_scriptfunc(sf),
        }
    }

    fn block_group(self, group: &Group<'_>) -> Block {
        let inner = match self.group_placeholder(group) {
            Some(chr) => Block::text(chr),
            None => self.block_expression(&group.expr),
        };
        let left = left_bracket_str(group.left_bracket);
        let right = right_bracket_str(group.right_bracket);
        inner.with_brackets(left, right)
    }

    fn block_cell(self, expr: &Expression<'_>) -> Block {
        match self.empty_placeholder(expr) {
            Some(chr) => Block::text(chr),
            None => self.grid_cell().block_expression(expr),
        }
    }

    fn block_matrix(self, matrix: &Matrix<'_>) -> Block {
        let num_cols = matrix.num_cols();

        let rows: Vec<Vec<Block>> = matrix
            .rows()
            .map(|row| row.iter().map(|expr| self.block_cell(expr)).collect())
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
                let below = row
                    .iter()
                    .map(|cell| cell.height() - 1 - cell.baseline)
                    .max()
                    .unwrap_or_default()
                    + usize::from(spaced && index != last_row);
                let height = above + below + 1;
                let rule = |boundary| {
                    let line = column_separator(column_rules(matrix, boundary), boundary, num_cols);
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
        let grid_lines: Vec<String> = row_blocks.into_iter().flat_map(|row| row.lines).collect();
        let grid = Block {
            baseline: grid_lines.len() / 2,
            width: total_width,
            lines: grid_lines,
        };
        let left = left_bracket_str(matrix.left_bracket);
        let right = right_bracket_str(matrix.right_bracket);
        grid.with_brackets(left, right)
    }

    fn block_func(self, func: &Func<'_>) -> Block {
        let name = Block::text(func.func);
        let name_with_script = self.block_apply_script(name, &func.script);
        let arg = if func.arg().as_simple().is_some_and(name_without_argument) {
            Block::empty()
        } else {
            self.block_scriptfunc(func.arg())
        };
        if func_hugs_argument(func) || arg.width == 0 {
            name_with_script.beside(arg)
        } else {
            name_with_script.beside(Block::space(1)).beside(arg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Block, Conf};
    use crate::tokens;
    use crate::{Layout, Placeholders};
    use std::fmt::Write;

    fn render_block(input: &str) -> String {
        render_block_conf(input, Conf::default())
    }

    fn render_block_keeping_spaces(input: &str) -> String {
        render_block_conf(input, Conf::default().with_keep_spaces(true))
    }

    fn render_block_conf(input: &str, conf: Conf) -> String {
        // block_expression is the multi-line layout, whatever conf it is handed
        let conf = Conf {
            layout: Layout::Block,
            ..conf
        };
        let mut out = String::new();
        let expr = tokens::parse(input, conf.keep_spaces);
        write!(out, "{}", conf.block_expression(&expr)).unwrap();
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
    fn block_empty() {
        let block = Block::empty();
        assert_eq!(block.width, 0);
        assert_eq!(block.height(), 1);
        assert_eq!(format!("{block}"), "");
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
        };
        assert_eq!(format!("{block}"), "ab\nc");
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
    fn letter_fracs_stack() {
        // whether letters have script forms doesn't decide the layout
        assert_eq!(render_block("x/n"), "x\n─\nn");
        assert_eq!(render_block("n/2"), "n\n─\n2");
    }

    #[test]
    fn stacked_frac_xy() {
        let result = render_block("x/y");
        assert_eq!(result, "x\n─\ny");
    }

    #[test]
    fn stacked_frac_with_expressions() {
        let conf = Conf {
            vulgar_fracs: false,
            layout: Layout::Block,
            ..Default::default()
        };
        let result = render_block_conf("(x+1)/y", conf);
        assert_eq!(result, "x + 1\n─────\n  y");
    }

    #[test]
    fn stacked_frac_denom_expr() {
        let conf = Conf {
            vulgar_fracs: false,
            layout: Layout::Block,
            ..Default::default()
        };
        let result = render_block_conf("x/(y+1)", conf);
        assert_eq!(result, "  x\n─────\ny + 1");
    }

    #[test]
    fn placeholders() {
        let conf = Conf {
            layout: Layout::Block,
            placeholders: Some(Placeholders::default()),
            ..Default::default()
        };
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
    fn matrix_simple() {
        assert_eq!(render_block("[[a,b],[c,d]]"), "⎡a  b⎤\n⎣c  d⎦");
    }

    #[test]
    fn frac_plus_term() {
        assert_eq!(render_block("x/y + z"), "x\n─ + z\ny");
    }

    #[test]
    fn operator_spacing() {
        let conf = Conf {
            vulgar_fracs: false,
            layout: Layout::Block,
            ..Default::default()
        };
        let result = render_block_conf("x + y", conf);
        assert_eq!(result, "x + y");
    }

    #[test]
    fn default_layout_stays_inline() {
        let conf = Conf::default();
        let res = conf.parse("sum_(i=1)^n i^3=((n(n+1))/2)^2").to_string();
        assert_eq!(res, "∑ᵢ₌₁ⁿ i³=(ⁿ⁽ⁿ⁺¹⁾⁄₂)²");
    }

    #[test]
    fn angle_bracket_height() {
        assert_eq!(
            render_block("<< (x + 1) / x_y >>"),
            "╱x + 1╲\n⎜─────⎟\n⎜  x  ⎟\n╲   y ╱"
        );
    }

    #[test]
    fn block_layout_uses_block_rendering() {
        let conf = Conf {
            layout: Layout::Block,
            ..Default::default()
        };
        let result = conf.parse("x/y").to_string();
        assert_eq!(result, "x\n─\ny");
    }

    /// Block config that forces stacked fractions (so wrapped content is multiline).
    fn stacked() -> Conf {
        Conf {
            vulgar_fracs: false,
            layout: Layout::Block,
            ..Default::default()
        }
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
        // `xx` renders as a spaced `×`, exercising `is_spaced_operator`.
        let result = render_block_conf("a/b xx c/d", stacked());
        assert_eq!(result, "a   c\n─ × ─\nb   d");
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
        assert_eq!(render_block("a !-= -b"), "a ≢ -b");
        assert_eq!(render_block("x^-1"), "x⁻¹");
        assert_eq!(render_block("text(hello world)"), "hello world");
    }

    #[test]
    fn block_script_brackets_stripped() {
        assert_eq!(render_block("x_(2i)"), "x₂ᵢ");
        assert_eq!(render_block("sum_(i=1)^n i"), "∑ᵢ₌₁ⁿ i");
    }

    #[test]
    fn block_unary_sign() {
        assert_eq!(render_block("x = -1"), "x = -1");
        assert_eq!(render_block("-x + 1"), "-x + 1");
        assert_eq!(render_block("a - b"), "a - b");
        assert_eq!(render_block("a and -b"), "a and -b");
        assert_eq!(render_block("x > 0 or x < -1"), "x > 0 or x < -1");
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
    fn floor_stays_inline_in_block() {
        // floor/ceil/abs fall back to inline rendering even in block mode.
        assert_eq!(render_block_conf("floor(x/y)", stacked()), "⌊x/y⌋");
        assert_eq!(render_block_conf("abs(x/y)", stacked()), "|x/y|");
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
    fn spaced_operator_full_list() {
        // `|->` (mapsto) is the last arm of `is_spaced_operator`, so matching it
        // forces evaluation of every preceding operator pattern.
        let conf = Conf {
            layout: Layout::Block,
            ..Default::default()
        };
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
        // numerator carries a script, so the frac is rendered via the `Frac` path
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
    fn invisible_brackets() {
        // `{:` and `:}` are invisible grouping brackets
        assert_eq!(render_block_conf("{:x/y:}", stacked()), "x\n─\ny");
        // one invisible side exercises the empty-bracket column on the other
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
    fn block_script_fraction() {
        // script fractions are inline-only; block mode stacks
        let conf = Conf {
            layout: Layout::Block,
            ..Default::default()
        };
        assert_eq!(conf.parse("x/n").to_string(), "x\n─\nn");
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
        assert_eq!(render_block_keeping_spaces("a+b"), "a + b");
        assert_eq!(render_block_keeping_spaces("a  +b"), "a  + b");
        assert_eq!(render_block_keeping_spaces("x / y   z"), "x\n─   z\ny");
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
    fn a_kept_space_after_a_unary_sign_is_dropped() {
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
        // a comma and a big operator without scripts are operators to a sign after them
        assert_eq!(render_block_keeping_spaces("(1, - 2)"), "(1, -2)");
        assert_eq!(render_block_keeping_spaces("sum - x"), "∑ -x");
        // a sign with nothing after it has no operand to hug
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
        // a big operator with scripts is no operator to a sign after it
        assert_eq!(render_block_keeping_spaces("sum_(i=1)^n - i"), "∑ᵢ₌₁ⁿ - i");
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
}
