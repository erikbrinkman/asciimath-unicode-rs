//! What both layouts ask of the parsed math

use super::tokens::bracket_str;

use asciimath_parser::SymbolClass;
use asciimath_parser::tree::{
    Expression, Frac, Func, Group, Intermediate, Script, ScriptFunc, Simple, SimpleScript, Symbol,
};

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

/// The contents of a group whose brackets only group
pub(crate) fn paren_contents<'s, 'a>(simple: &'s Simple<'a>) -> Option<&'s Expression<'a>> {
    match simple {
        Simple::Group(Group {
            left_bracket,
            expr,
            right_bracket,
        }) if matches!(bracket_str(left_bracket), "(" | "[" | "{" | "")
            && matches!(bracket_str(right_bracket), ")" | "]" | "}" | "") =>
        {
            Some(expr)
        }
        _ => None,
    }
}

/// Whether a group was opened with nothing in it yet and its brackets only group
pub(crate) fn is_empty_grouping(simple: &Simple<'_>) -> bool {
    match simple {
        Simple::Group(Group { left_bracket, .. }) if !left_bracket.is_empty() => {
            paren_contents(simple).is_some_and(|expr| expr.is_empty())
        }
        _ => false,
    }
}

/// The brackets a delimiting command like `abs` puts around its argument
pub(crate) fn delimiters(op: &str) -> Option<(&'static str, &'static str)> {
    match op {
        "abs" | "Abs" => Some(("|", "|")),
        "ceil" => Some(("⌈", "⌉")),
        "floor" => Some(("⌊", "⌋")),
        "norm" => Some(("||", "||")),
        _ => None,
    }
}

/// The lone unscripted simple an expression consists of, if any
pub(crate) fn single_simple<'s, 'a>(expr: &'s Expression<'a>) -> Option<&'s Simple<'a>> {
    if let [
        Intermediate::ScriptFunc(ScriptFunc::Simple(SimpleScript {
            simple,
            script: Script::None,
        })),
    ] = &**expr
    {
        Some(simple)
    } else {
        None
    }
}

/// `simple` without brackets that only group a lone simple
pub(crate) fn unwrap_parens<'s, 'a>(simple: &'s Simple<'a>) -> &'s Simple<'a> {
    paren_contents(simple)
        .and_then(single_simple)
        .unwrap_or(simple)
}

fn simple_str<'a>(simple: &Simple<'a>) -> Option<&'a str> {
    match simple {
        &Simple::Number(text) | &Simple::Ident(text) => Some(text),
        _ => None,
    }
}

/// The vulgar fraction char for `numer` over `denom`, dropping brackets that only group if
/// `strip`, when `unstyled` holds for the text of both
pub(crate) fn extract_vulgar_frac(
    numer: &Simple<'_>,
    denom: &Simple<'_>,
    strip: bool,
    unstyled: impl Fn(&str) -> bool,
) -> Option<char> {
    let (num, den) = if strip {
        (unwrap_parens(numer), unwrap_parens(denom))
    } else {
        (numer, denom)
    };
    let (num, den) = (simple_str(num)?, simple_str(den)?);
    if unstyled(num) && unstyled(den) {
        vulgar_frac_char(num, den)
    } else {
        None
    }
}

/// The class of a symbol
fn symbol_class(simple: &Simple<'_>) -> Option<SymbolClass> {
    match *simple {
        Simple::Symbol(Symbol { class, .. }) => Some(class),
        _ => None,
    }
}

/// How an item behaves at one of its edges when deciding whether a space separates it from its
/// neighbor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edge {
    /// a relation, binary operator, comma, or unscripted big operator; hugs its neighbors inline
    Operator,
    /// a word that reads as its own token, like `mod`, `and`, `dx`, or a function name
    Word,
    /// a function-like word such as `lim` or `max` that touches a following bracket
    Applied,
    /// a symbol taking an operand, with scripts
    Scripted,
    /// an opening bracket
    Bracket,
    /// anything else
    Operand,
}

fn simple_edges(simple: &Simple<'_>, script: &Script<'_>) -> (Edge, Edge) {
    let scripted = !matches!(script, Script::None);
    match (simple, symbol_class(simple)) {
        (Simple::Func(_), _) => (Edge::Word, Edge::Operand),
        (Simple::Group(_) | Simple::Matrix(_), _) => (Edge::Bracket, Edge::Operand),
        (Simple::Sign(_), _) if !scripted => (Edge::Operator, Edge::Operator),
        (_, Some(class)) if class.takes_operand() => {
            let after = if scripted {
                Edge::Scripted
            } else if class.is_name() {
                Edge::Applied
            } else {
                Edge::Operator
            };
            let before = if class.is_name() {
                Edge::Word
            } else {
                Edge::Operand
            };
            (before, after)
        }
        (_, Some(class)) if class.is_name() => (Edge::Word, Edge::Word),
        (_, Some(class))
            if !scripted && (class.joins_operands() || class == SymbolClass::Separator) =>
        {
            (Edge::Operator, Edge::Operator)
        }
        _ => (Edge::Operand, Edge::Operand),
    }
}

/// The outer edges of a scripted item or applied function
pub(crate) fn scriptfunc_edges(func: &ScriptFunc<'_>) -> (Edge, Edge) {
    match func {
        ScriptFunc::Simple(SimpleScript { simple, script }) => simple_edges(simple, script),
        // the sign is written against its operand, so the operand's right edge is the pair's
        ScriptFunc::Signed(signed) => (Edge::Operand, scriptfunc_edges(signed.operand()).1),
        // a bare name like the `f` in `def` is just a letter
        ScriptFunc::Func(func)
            if matches!(
                func.arg(),
                ScriptFunc::Simple(SimpleScript {
                    simple: Simple::Missing,
                    ..
                })
            ) =>
        {
            (Edge::Operand, Edge::Applied)
        }
        ScriptFunc::Func(_) => (Edge::Word, Edge::Operand),
    }
}

/// The outer edges of a fraction, from its numerator and its denominator
pub(crate) fn frac_edges(frac: &Frac<'_>) -> (Edge, Edge) {
    (
        scriptfunc_edges(&frac.numer).0,
        scriptfunc_edges(&frac.denom).1,
    )
}

/// Whether the edges where two adjacent items of an expression meet need a space between them so
/// words stay legible
fn needs_space(prev: Edge, next: Edge) -> bool {
    match (prev, next) {
        (Edge::Operator, _) | (_, Edge::Operator) => false,
        (Edge::Word | Edge::Scripted, _) | (_, Edge::Word) => true,
        (Edge::Applied, next) => next != Edge::Bracket,
        _ => false,
    }
}

/// What separates an item of an expression from the one before it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Separator<'t> {
    /// the whitespace typed between them
    Typed(&'t str),
    /// one space
    Space,
    /// nothing
    Nothing,
}

/// What separates an item from the one before it: the whitespace typed between them if any was
/// kept, and otherwise a space where either is an operator getting spaces or the words need one
fn separator(
    typed: &str,
    prev_edge: Edge,
    prev_spaced_op: bool,
    next_edge: Edge,
    next_spaced_op: bool,
) -> Separator<'_> {
    if !typed.is_empty() {
        Separator::Typed(typed)
    } else if prev_spaced_op || next_spaced_op || needs_space(prev_edge, next_edge) {
        Separator::Space
    } else {
        Separator::Nothing
    }
}

/// Each item of `expr` that renders to something, as `render` renders it after what separates it
/// from the item before; `render` gives `None` for an item that renders to nothing, like an empty
/// group, so it leaves no space behind
pub(crate) fn separated_items<'e, 'a, T>(
    expr: &'e Expression<'a>,
    keep_spaces: bool,
    spaced_operators: bool,
    mut render: impl FnMut(&'e Intermediate<'a>, Separator<'_>) -> Option<T> + 'e,
) -> impl Iterator<Item = T> + 'e {
    // the runs typed on both sides of an item that renders to nothing both belong in the output
    let mut typed = String::new();
    let mut prev: Option<(Edge, bool)> = None;
    expr.iter().filter_map(move |inter| {
        let (leading_edge, trailing_edge) = match inter {
            Intermediate::Space(space) => {
                if keep_spaces {
                    typed.push_str(space);
                }
                return None;
            }
            Intermediate::ScriptFunc(func) => scriptfunc_edges(func),
            Intermediate::Frac(frac) => frac_edges(frac),
        };
        let spaced_op = spaced_operators && inter_is_spaced_op(inter);
        let sep = match prev {
            Some((prev_edge, prev_spaced_op)) => {
                separator(&typed, prev_edge, prev_spaced_op, leading_edge, spaced_op)
            }
            None => Separator::Nothing,
        };
        let item = render(inter, sep)?;
        typed.clear();
        prev = Some((trailing_edge, spaced_op));
        Some(item)
    })
}

/// Whether an item is an operator that gets a space on either side of it
///
/// A sign joining the operands around it is one; a sign prefixing its operand arrives as a
/// [`Signed`][asciimath_parser::tree::Signed] operand instead and so never matches.
pub(crate) fn inter_is_spaced_op(inter: &Intermediate<'_>) -> bool {
    match inter {
        Intermediate::ScriptFunc(ScriptFunc::Simple(SimpleScript {
            simple: Simple::Sign(_),
            script: Script::None,
        })) => true,
        Intermediate::ScriptFunc(ScriptFunc::Simple(SimpleScript {
            simple,
            script: Script::None,
        })) => symbol_class(simple).is_some_and(SymbolClass::joins_operands),
        _ => false,
    }
}

/// Whether a function name is written directly against its argument, as in `f(x)` or `f'`
fn hugs_argument(arg: &Simple<'_>) -> bool {
    matches!(
        arg,
        Simple::Missing
            | Simple::Group(_)
            | Simple::Matrix(_)
            | Simple::Symbol(Symbol {
                text: "'" | "prime",
                ..
            })
    )
}

/// How a function name meets the argument after it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgGap {
    /// no argument was written; a name like `f` or `sin` reads fine alone, so nothing follows it,
    /// not even a placeholder
    Bare,
    /// the argument touches the name
    Hugs,
    /// a space separates them
    Spaced,
}

impl ArgGap {
    /// How a name meets `arg`
    pub(crate) fn of_simple(arg: &Simple<'_>) -> Self {
        if matches!(arg, Simple::Missing) {
            ArgGap::Bare
        } else if hugs_argument(arg) {
            ArgGap::Hugs
        } else {
            ArgGap::Spaced
        }
    }

    /// How the name of `func` meets its argument
    pub(crate) fn of_func(func: &Func<'_>) -> Self {
        match func.arg() {
            ScriptFunc::Simple(SimpleScript {
                simple: Simple::Missing,
                script: Script::None,
            }) => ArgGap::Bare,
            ScriptFunc::Simple(SimpleScript { simple, .. }) if hugs_argument(simple) => {
                ArgGap::Hugs
            }
            _ => ArgGap::Spaced,
        }
    }

    /// Whether a space goes between the name and an argument, which renders to nothing if `empty`
    pub(crate) fn spaced(self, empty: bool) -> bool {
        self == ArgGap::Spaced && !empty
    }
}
