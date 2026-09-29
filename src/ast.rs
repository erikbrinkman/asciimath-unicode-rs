#![allow(missing_docs, clippy::must_use_candidate)]

use asciimath_parser::tree::{
    Expression, Group, Intermediate, Script, ScriptFunc, Simple, SimpleScript,
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
