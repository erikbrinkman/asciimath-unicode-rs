#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]

use std::{fmt, iter};
use unicode_width::UnicodeWidthStr;

use super::Conf;
use super::inline::{Mapper, MapperConf};
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

    fn height(&self) -> usize {
        self.lines.len()
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
        let bar_width = numer.width.max(denom.width);
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
            let left_col = tall_bracket_left(left, self.height()).with_baseline(self.baseline);
            let right_col = tall_bracket_right(right, self.height()).with_baseline(self.baseline);
            let new_baseline = self.height() / 2;
            left_col
                .beside(self)
                .beside(right_col)
                .with_baseline(new_baseline)
        }
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

fn tall_bracket_left(bracket: &str, height: usize) -> Block {
    if bracket.is_empty() {
        Block {
            lines: vec![String::new(); height],
            baseline: height / 2,
            width: 0,
        }
    } else if height <= 1 {
        Block::text(bracket)
    } else {
        let (top, mid_top, mid_bot, bot) = match bracket {
            "(" | "left(" => ('⎛', '⎜', '⎜', '⎝'),
            "[" | "left[" => ('⎡', '⎢', '⎢', '⎣'),
            "⟨" | "(:" | "langle" | "<<" => ('╱', '⎜', '⎜', '╲'),
            "⌊" | "|__" | "lfloor" => ('⎢', '⎢', '⎜', '⌊'),
            "⌈" | "|~" | "lceiling" => ('⌈', '⎢', '⎢', '⎜'),
            "{" if height == 2 => ('⎰', ' ', ' ', '⎱'),
            "{" if height % 2 == 0 => ('⎧', '⎭', '⎫', '⎩'),
            "{" => ('⎧', '│', '⎨', '⎩'),
            // "|", "|:", and anything else
            _ => ('│', '│', '│', '│'),
        };
        let mut lines = Vec::with_capacity(height);
        lines.push(top.to_string());
        for idx in 1..height - 1 {
            lines.push(
                if idx == height / 2 {
                    mid_bot
                } else if idx == height / 2 - 1 {
                    mid_top
                } else {
                    '│'
                }
                .to_string(),
            );
        }
        lines.push(bot.to_string());
        Block {
            lines,
            baseline: height / 2,
            width: 1,
        }
    }
}

fn tall_bracket_right(bracket: &str, height: usize) -> Block {
    if bracket.is_empty() {
        Block {
            lines: vec![String::new(); height],
            baseline: height / 2,
            width: 0,
        }
    } else if height <= 1 {
        Block::text(bracket)
    } else {
        let (top, mid_top, mid_bot, bot) = match bracket {
            ")" | "right)" => ('⎞', '⎟', '⎟', '⎠'),
            "]" | "right]" => ('⎤', '⎥', '⎥', '⎦'),
            "⟩" | ":)" | "rangle" | ">>" => ('╲', '⎟', '⎟', '╱'),
            "⌋" | "__|" | "rfloor" => ('⎥', '⎥', '⎟', '⌋'),
            "⌉" | "~|" | "rceiling" => ('⌉', '⎥', '⎥', '⎟'),
            "}" if height == 2 => ('⎱', ' ', ' ', '⎰'),
            "}" if height % 2 == 0 => ('⎫', '⎩', '⎧', '⎭'),
            "}" => ('⎫', '│', '⎬', '⎭'),
            // "|", ":|", and anything else
            _ => ('│', '│', '│', '│'),
        };
        let mut lines = Vec::with_capacity(height);
        lines.push(top.to_string());
        for idx in 1..height - 1 {
            lines.push(
                if idx == height / 2 {
                    mid_bot
                } else if idx == height / 2 - 1 {
                    mid_top
                } else {
                    '│'
                }
                .to_string(),
            );
        }
        lines.push(bot.to_string());
        Block {
            lines,
            baseline: height / 2,
            width: 1,
        }
    }
}

#[allow(clippy::too_many_lines)]
fn is_spaced_operator(sym: &str) -> bool {
    matches!(
        sym,
        "+" | "-"
            | "="
            | "!="
            | "ne"
            | "<"
            | "lt"
            | "<="
            | "le"
            | "lt="
            | "leq"
            | ">"
            | "gt"
            | ">="
            | "ge"
            | "gt="
            | "geq"
            | "mlt"
            | "ll"
            | "mgt"
            | "gg"
            | "-<"
            | "prec"
            | "-lt"
            | ">-"
            | "succ"
            | "-<="
            | "preceq"
            | ">-="
            | "succeq"
            | "in"
            | "!in"
            | "notin"
            | "sub"
            | "subset"
            | "sup"
            | "supset"
            | "sube"
            | "subseteq"
            | "supe"
            | "supseteq"
            | "-="
            | "equiv"
            | "~="
            | "cong"
            | "~~"
            | "approx"
            | "~"
            | "sim"
            | "prop"
            | "propto"
            | "=>"
            | "implies"
            | "<=>"
            | "iff"
            | "AA"
            | "forall"
            | "EE"
            | "exists"
            | "|--"
            | "vdash"
            | "|=="
            | "models"
            | "and"
            | "or"
            | "if"
            | "+-"
            | "pm"
            | "-+"
            | "mp"
            | "xx"
            | "times"
            | "-:"
            | "div"
            | "divide"
            | "*"
            | "cdot"
            | "**"
            | "ast"
            | "o+"
            | "oplus"
            | "ox"
            | "otimes"
            | "o."
            | "odot"
            | "^^"
            | "wedge"
            | "land"
            | "vv"
            | "vee"
            | "lor"
            | "nn"
            | "cap"
            | "uu"
            | "cup"
            | "rarr"
            | "rightarrow"
            | "->"
            | "to"
            | "larr"
            | "leftarrow"
            | "<-"
            | "harr"
            | "leftrightarrow"
            | "<->"
            | "rArr"
            | "Rightarrow"
            | "==>"
            | "lArr"
            | "Leftarrow"
            | "<=="
            | "hArr"
            | "Leftrightarrow"
            | "<==>"
            | "|->"
            | "mapsto"
    )
}

fn is_spaced_ident(id: &str) -> bool {
    matches!(id, "+" | "-" | "=" | ">" | "<" | "≤" | "≥" | "≠")
}

fn inter_is_spaced_op(inter: &Intermediate<'_>) -> bool {
    match inter {
        Intermediate::ScriptFunc(ScriptFunc::Simple(SimpleScript {
            simple: Simple::Symbol(sym),
            script: Script::None,
        })) => is_spaced_operator(sym),
        Intermediate::ScriptFunc(ScriptFunc::Simple(SimpleScript {
            simple: Simple::Ident(id),
            script: Script::None,
        })) => is_spaced_ident(id),
        _ => false,
    }
}

impl Conf {
    fn block_inline_simple(self, simple: &Simple<'_>) -> Block {
        let mut s = String::new();
        self.inline_simple(simple, &mut Mapper::new(&mut s))
            .unwrap_or_else(|_| unreachable!("write to String is infallible"));
        Block::text(s)
    }

    pub(crate) fn block_expression(self, expr: &Expression<'_>) -> Block {
        let mut items = expr.iter();
        let Some(first) = items.next() else {
            return Block::empty();
        };
        let mut result = self.block_intermediate(first);
        for inter in items {
            let block = self.block_intermediate(inter);
            if inter_is_spaced_op(inter) {
                result = result.beside(Block::space(1));
                result = result.beside(block);
                result = result.beside(Block::space(1));
            } else {
                result = result.beside(block);
            }
        }
        result
    }

    fn block_intermediate(self, inter: &Intermediate<'_>) -> Block {
        match inter {
            Intermediate::ScriptFunc(sf) => self.block_scriptfunc(sf),
            Intermediate::Frac(frac) => self.block_frac(frac),
        }
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
        match script {
            Script::None => base,
            Script::Sub(sub) => {
                let conf = MapperConf {
                    sub_sup: Some(subscript_char),
                    ..MapperConf::default()
                };
                let mut out = String::new();
                if self.inline_simple(sub, &mut conf.wrap(&mut out)).is_ok() {
                    base.beside(Block::text(out))
                } else {
                    // Vertical: sub below-right
                    let sub_blk = self.block_simple(sub);
                    let original = base.baseline;
                    let base_h = base.height();
                    base.with_baseline(base_h)
                        .beside(sub_blk.with_baseline(0))
                        .with_baseline(original)
                }
            }
            Script::Super(sup) => {
                let conf = MapperConf {
                    sub_sup: Some(superscript_char),
                    ..MapperConf::default()
                };
                let mut out = String::new();
                if self.inline_simple(sup, &mut conf.wrap(&mut out)).is_ok() {
                    base.beside(Block::text(out))
                } else {
                    // Vertical: sup above-right
                    let sup_blk = self.block_simple(sup);
                    let new_baseline = sup_blk.height() + base.baseline;
                    let sup_h = sup_blk.height();
                    base.with_baseline(0)
                        .beside(sup_blk.with_baseline(sup_h))
                        .with_baseline(new_baseline)
                }
            }
            Script::Subsuper(sub, sup) => {
                let lower_conf = MapperConf {
                    sub_sup: Some(subscript_char),
                    ..MapperConf::default()
                };
                let upper_conf = MapperConf {
                    sub_sup: Some(superscript_char),
                    ..MapperConf::default()
                };
                let mut subscript = String::new();
                let mut superscript = String::new();
                if self
                    .inline_simple(sub, &mut lower_conf.wrap(&mut subscript))
                    .is_ok()
                    && self
                        .inline_simple(sup, &mut upper_conf.wrap(&mut superscript))
                        .is_ok()
                {
                    base.beside(Block::text(format!("{subscript}{superscript}")))
                } else {
                    // Vertical: sup above-right, then sub below-right
                    let upper = self.block_simple(sup);
                    let new_baseline = upper.height() + base.baseline;
                    let upper_h = upper.height();
                    let with_sup = base
                        .with_baseline(0)
                        .beside(upper.with_baseline(upper_h))
                        .with_baseline(new_baseline);

                    let lower = self.block_simple(sub);
                    let original = with_sup.baseline;
                    let with_sup_h = with_sup.height();
                    with_sup
                        .with_baseline(with_sup_h)
                        .beside(lower.with_baseline(0))
                        .with_baseline(original)
                }
            }
        }
    }

    fn block_simple(self, simple: &Simple<'_>) -> Block {
        match simple {
            Simple::Missing => Block::empty(),
            Simple::Group(group) => self.block_group(group),
            Simple::Matrix(matrix) => self.block_matrix(matrix),
            Simple::Unary(unary) => self.block_unary(unary),
            Simple::Binary(binary) => self.block_binary(binary),
            Simple::Func(func) => self.block_simplefunc(func),
            _ => self.block_inline_simple(simple),
        }
    }

    fn block_simplefunc(self, func: &SimpleFunc<'_>) -> Block {
        let name = Block::text(func.func);
        let arg = self.block_simple(func.arg());
        name.beside(Block::space(1)).beside(arg)
    }

    fn block_unary(self, unary: &SimpleUnary<'_>) -> Block {
        if unary.op == "sqrt" {
            let arg = self.block_simple(unary.arg());
            if arg.is_multiline() {
                return Block::text("√").beside(arg);
            }
            let mut s = String::from("√");
            self.inline_simple(unary.arg(), &mut Mapper::new(&mut s))
                .unwrap_or_else(|_| unreachable!("write to String is infallible"));
            Block::text(s)
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

    fn try_script_simplefrac(self, numer: &Simple<'_>, denom: &Simple<'_>) -> Option<String> {
        if self.script_fracs {
            let mut text = String::new();
            self.inline_simplefrac(numer, denom, &mut Mapper::new(&mut text))
                .ok()?;
            Some(text)
        } else {
            None
        }
    }

    fn try_script_frac(self, frac: &Frac<'_>) -> Option<String> {
        if self.script_fracs {
            let mut text = String::new();
            self.inline_frac(frac, &mut Mapper::new(&mut text)).ok()?;
            Some(text)
        } else {
            None
        }
    }

    fn block_simplefrac(self, numer: &Simple<'_>, denom: &Simple<'_>) -> Block {
        if self.vulgar_fracs
            && let Some(frac) = super::ast::extract_vulgar_frac(numer, denom, self.strip_brackets)
        {
            Block::text(frac)
        } else if let Some(text) = self.try_script_simplefrac(numer, denom) {
            Block::text(text)
        } else {
            Block::stack_frac(
                self.block_simple_or_expr_stripped(numer),
                self.block_simple_or_expr_stripped(denom),
            )
        }
    }

    /// If `strip_brackets` is on and simple is a group, render the inner expression.
    fn block_simple_or_expr_stripped(self, simple: &Simple<'_>) -> Block {
        if self.strip_brackets
            && let Simple::Group(Group { expr, .. }) = simple
        {
            self.block_expression(expr)
        } else {
            self.block_simple(simple)
        }
    }

    fn block_frac(self, frac: &Frac<'_>) -> Block {
        if let (
            ScriptFunc::Simple(SimpleScript {
                simple: num,
                script: Script::None,
            }),
            ScriptFunc::Simple(SimpleScript {
                simple: den,
                script: Script::None,
            }),
        ) = (&frac.numer, &frac.denom)
        {
            self.block_simplefrac(num, den)
        } else if let Some(text) = self.try_script_frac(frac) {
            Block::text(text)
        } else {
            Block::stack_frac(
                self.block_scriptfunc_for_frac(&frac.numer),
                self.block_scriptfunc_for_frac(&frac.denom),
            )
        }
    }

    fn block_scriptfunc_for_frac(self, sf: &ScriptFunc<'_>) -> Block {
        match sf {
            ScriptFunc::Simple(SimpleScript {
                simple,
                script: Script::None,
            }) => self.block_simple_or_expr_stripped(simple),
            _ => self.block_scriptfunc(sf),
        }
    }

    fn block_group(self, group: &Group<'_>) -> Block {
        let inner = self.block_expression(&group.expr);
        let left = left_bracket_str(group.left_bracket);
        let right = right_bracket_str(group.right_bracket);
        inner.with_brackets(left, right)
    }

    fn block_matrix(self, matrix: &Matrix<'_>) -> Block {
        let num_rows = matrix.num_rows();
        let num_cols = matrix.num_cols();
        let sep_width = 2;

        let mut cells = Vec::with_capacity(num_rows * num_cols);
        for row in matrix.rows() {
            for expr in row {
                cells.push(self.block_expression(expr));
            }
        }

        // one width, one above, one below for all cells
        let col_width = cells.iter().map(|c| c.width).max().unwrap_or_default();
        let above = cells.iter().map(|c| c.baseline).max().unwrap_or_default();
        let below = cells
            .iter()
            .map(|c| c.height() - 1 - c.baseline)
            .max()
            .unwrap_or_default();

        // build rows
        let total_width = col_width * num_cols + (num_cols - 1) * sep_width;
        let mut grid_lines: Vec<String> = Vec::new();
        let mut cells = cells.into_iter();
        for _ in 0..num_rows {
            if !grid_lines.is_empty() {
                grid_lines.push(" ".repeat(total_width));
            }
            let mut cell_row = cells
                .by_ref()
                .take(num_cols)
                .map(|cell| cell.pad_vertical(above, below).pad_center(col_width));
            let mut row_block: Block = cell_row
                .next()
                .unwrap_or_else(|| unreachable!("must have at least one col"));
            for cell in cell_row {
                row_block = row_block.beside(Block::space(sep_width)).beside(cell);
            }
            grid_lines.extend(row_block.lines);
        }

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
        let arg = self.block_scriptfunc(func.arg());
        name_with_script.beside(Block::space(1)).beside(arg)
    }
}

#[cfg(test)]
mod tests {
    use super::{Block, Conf};
    use crate::tokens;
    use std::fmt::Write;

    fn render_block(input: &str) -> String {
        render_block_conf(input, Conf::default())
    }

    fn render_block_conf(input: &str, conf: Conf) -> String {
        let mut out = String::new();
        let expr = tokens::parse(input);
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
    fn script_frac_passthrough() {
        assert_eq!(render_block("x/n"), "ˣ⁄ₙ");
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
            script_fracs: false,
            ..Default::default()
        };
        let result = render_block_conf("(x+1)/y", conf);
        assert_eq!(result, "x + 1\n─────\n  y");
    }

    #[test]
    fn stacked_frac_denom_expr() {
        let conf = Conf {
            vulgar_fracs: false,
            script_fracs: false,
            ..Default::default()
        };
        let result = render_block_conf("x/(y+1)", conf);
        assert_eq!(result, "  x\n─────\ny + 1");
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
        let result = render_block("[[a,b],[c,d]]");
        assert!(result.contains('a'));
        assert!(result.contains('b'));
        assert!(result.contains('c'));
        assert!(result.contains('d'));
        assert!(result.contains('⎡') || result.contains('['));
    }

    #[test]
    fn frac_plus_term() {
        let result = render_block("x/y + z");
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[1].contains('+'));
        assert!(lines[1].contains('z'));
    }

    #[test]
    fn operator_spacing() {
        let conf = Conf {
            vulgar_fracs: false,
            script_fracs: false,
            ..Default::default()
        };
        let result = render_block_conf("x + y", conf);
        assert_eq!(result, "x + y");
    }

    #[test]
    fn existing_tests_still_pass_inline() {
        let conf = Conf::default();
        let res = conf.parse("sum_(i=1)^n i^3=((n(n+1))/2)^2").to_string();
        assert_eq!(res, "∑₍ᵢ₌₁₎ⁿi³=(ⁿ⁽ⁿ⁺¹⁾⁄₂)²");
    }

    #[test]
    fn angle_bracket_height() {
        let result = render_block("<< (x + 1) / x_y >>");
        eprintln!("angle bracket result:\n{result}");
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 4, "expected 4 lines:\n{result}");
    }

    #[test]
    fn conf_block_true_uses_block_rendering() {
        let conf = Conf {
            block: true,
            ..Default::default()
        };
        let result = conf.parse("x/y").to_string();
        assert_eq!(result, "x\n─\ny");
    }
}
