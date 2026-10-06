#![allow(missing_docs, clippy::must_use_candidate)]

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
pub fn paren_contents<'s, 'a>(simple: &'s Simple<'a>) -> Option<&'s Expression<'a>> {
    match simple {
        Simple::Group(Group {
            left_bracket,
            expr,
            right_bracket,
        }) if matches!(
            *left_bracket,
            "(" | "left(" | "[" | "left[" | "{" | "{:" | ""
        ) && matches!(
            *right_bracket,
            ")" | "right)" | "]" | "right]" | "}" | ":}" | ""
        ) =>
        {
            Some(expr)
        }
        _ => None,
    }
}

/// Whether a group was opened with nothing in it yet and its brackets only group
pub fn is_empty_grouping(simple: &Simple<'_>) -> bool {
    match simple {
        Simple::Group(Group { left_bracket, .. }) if !left_bracket.is_empty() => {
            paren_contents(simple).is_some_and(|expr| expr.is_empty())
        }
        _ => false,
    }
}

/// Whether a function name was written with no argument after it; a name like `f` or `sin` reads
/// fine alone, so it gets no placeholder
pub fn name_without_argument(arg: &Simple<'_>) -> bool {
    matches!(arg, Simple::Missing)
}

/// The lone unscripted simple an expression consists of, if any
pub fn single_simple<'s, 'a>(expr: &'s Expression<'a>) -> Option<&'s Simple<'a>> {
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

pub fn unwrap_parens<'s, 'a>(simple: &'s Simple<'a>) -> &'s Simple<'a> {
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

pub fn extract_vulgar_frac(numer: &Simple<'_>, denom: &Simple<'_>, strip: bool) -> Option<char> {
    let (num, den) = if strip {
        (unwrap_parens(numer), unwrap_parens(denom))
    } else {
        (numer, denom)
    };
    vulgar_frac_char(simple_str(num)?, simple_str(den)?)
}

#[allow(clippy::too_many_lines)]
pub fn is_spaced_operator(sym: &str) -> bool {
    matches!(
        sym,
        "+" | "-"
            | "="
            | "!="
            | "ne"
            | "≠"
            | "<"
            | "lt"
            | "<="
            | "le"
            | "lt="
            | "leq"
            | "≤"
            | ">"
            | "gt"
            | ">="
            | "ge"
            | "gt="
            | "geq"
            | "≥"
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
            | "!sub"
            | "nsub"
            | "notsubset"
            | "!sup"
            | "nsup"
            | "notsupset"
            | "!sube"
            | "nsubseteq"
            | "notsubseteq"
            | "!supe"
            | "nsupseteq"
            | "notsupseteq"
            | "!-="
            | "notequiv"
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
            | "rightleftharpoons"
    )
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
    /// a big operator or `lim` with scripts; always separated from what follows
    Scripted,
    /// an opening bracket
    Bracket,
    /// anything else
    Operand,
}

fn is_big_operator(sym: &str) -> bool {
    matches!(
        sym,
        "sum"
            | "prod"
            | "int"
            | "oint"
            | "iint"
            | "iiint"
            | "oiint"
            | "oiiint"
            | "^^^"
            | "bigwedge"
            | "vvv"
            | "bigvee"
            | "nnn"
            | "bigcap"
            | "uuu"
            | "bigcup"
    )
}

fn simple_edges(simple: &Simple<'_>, script: &Script<'_>) -> (Edge, Edge) {
    let scripted = !matches!(script, Script::None);
    match *simple {
        Simple::Symbol(Symbol {
            text: "and" | "or" | "if" | "mod" | "dx" | "dy" | "dz" | "dt",
            ..
        }) => (Edge::Word, Edge::Word),
        Simple::Symbol(Symbol { text, .. }) if is_big_operator(text) => (
            Edge::Operand,
            if scripted {
                Edge::Scripted
            } else {
                Edge::Operator
            },
        ),
        Simple::Symbol(Symbol { text, .. }) | Simple::Sign(text)
            if !scripted && (text == "," || is_spaced_operator(text)) =>
        {
            (Edge::Operator, Edge::Operator)
        }
        Simple::Symbol(Symbol {
            text: "lim" | "Lim" | "dim" | "min" | "max" | "lub" | "glb",
            ..
        }) => (
            Edge::Word,
            if scripted {
                Edge::Scripted
            } else {
                Edge::Applied
            },
        ),
        Simple::Func(_) => (Edge::Word, Edge::Operand),
        Simple::Group(_) | Simple::Matrix(_) => (Edge::Bracket, Edge::Operand),
        _ => (Edge::Operand, Edge::Operand),
    }
}

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
pub fn needs_space(prev: Edge, next: Edge) -> bool {
    match (prev, next) {
        (Edge::Operator, _) | (_, Edge::Operator) => false,
        (Edge::Word | Edge::Scripted, _) | (_, Edge::Word) => true,
        (Edge::Applied, next) => next != Edge::Bracket,
        _ => false,
    }
}

/// Whether an item is an operator that gets a space on either side of it
///
/// A sign joining the operands around it is one; a sign prefixing its operand arrives as a
/// [`Signed`][asciimath_parser::tree::Signed] operand instead and so never matches.
pub(crate) fn inter_is_spaced_op(inter: &Intermediate<'_>) -> bool {
    match inter {
        Intermediate::ScriptFunc(ScriptFunc::Simple(SimpleScript {
            simple:
                Simple::Symbol(Symbol { text, .. }) | Simple::Sign(text) | Simple::Operator(text),
            script: Script::None,
        })) => is_spaced_operator(text),
        _ => false,
    }
}

/// Whether a function name is written directly against its argument, as in `f(x)` or `f'`
pub fn hugs_argument(arg: &Simple<'_>) -> bool {
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

pub fn func_hugs_argument(func: &Func<'_>) -> bool {
    match func.arg() {
        ScriptFunc::Simple(SimpleScript { simple, .. }) => hugs_argument(simple),
        ScriptFunc::Func(_) | ScriptFunc::Signed(_) => false,
    }
}
