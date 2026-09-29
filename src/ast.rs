#![allow(missing_docs, clippy::must_use_candidate)]

use asciimath_parser::tree::{
    Expression, Func, Group, Intermediate, Script, ScriptFunc, Simple, SimpleScript,
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
enum Edge {
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
        Simple::Symbol("and" | "or" | "if") | Simple::Ident("mod" | "dx" | "dy" | "dz" | "dt") => {
            (Edge::Word, Edge::Word)
        }
        Simple::Symbol(sym) if is_big_operator(sym) => (
            Edge::Operand,
            if scripted {
                Edge::Scripted
            } else {
                Edge::Operator
            },
        ),
        Simple::Symbol(sym) if !scripted && (sym == "," || is_spaced_operator(sym)) => {
            (Edge::Operator, Edge::Operator)
        }
        Simple::Ident("lim" | "Lim" | "dim" | "min" | "max" | "lub" | "glb") => (
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

fn scriptfunc_edges(func: &ScriptFunc<'_>) -> (Edge, Edge) {
    match func {
        ScriptFunc::Simple(SimpleScript { simple, script }) => simple_edges(simple, script),
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

fn edges(inter: &Intermediate<'_>) -> (Edge, Edge) {
    match inter {
        Intermediate::ScriptFunc(func) => scriptfunc_edges(func),
        Intermediate::Frac(frac) => (
            scriptfunc_edges(&frac.numer).0,
            scriptfunc_edges(&frac.denom).1,
        ),
    }
}

/// Whether adjacent items of an expression need a space between them so words stay legible
pub fn needs_space(prev: &Intermediate<'_>, next: &Intermediate<'_>) -> bool {
    match (edges(prev).1, edges(next).0) {
        (Edge::Operator, _) | (_, Edge::Operator) => false,
        (Edge::Word | Edge::Scripted, _) | (_, Edge::Word) => true,
        (Edge::Applied, next) => next != Edge::Bracket,
        _ => false,
    }
}

/// Whether a function name is written directly against its argument, as in `f(x)` or `f'`
pub fn hugs_argument(arg: &Simple<'_>) -> bool {
    matches!(
        arg,
        Simple::Missing | Simple::Group(_) | Simple::Matrix(_) | Simple::Symbol("'" | "prime")
    )
}

pub fn func_hugs_argument(func: &Func<'_>) -> bool {
    match func.arg() {
        ScriptFunc::Simple(SimpleScript { simple, .. }) => hugs_argument(simple),
        ScriptFunc::Func(_) => false,
    }
}
