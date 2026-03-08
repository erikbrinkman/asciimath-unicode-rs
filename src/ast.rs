#![allow(missing_docs, clippy::must_use_candidate)]

use asciimath_parser::tree::{Expression, Intermediate, Script, ScriptFunc, Simple, SimpleScript};

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

pub fn extract_single_char(expr: &Expression<'_>) -> Option<char> {
    if let [
        Intermediate::ScriptFunc(ScriptFunc::Simple(SimpleScript {
            simple,
            script: Script::None,
        })),
    ] = &**expr
    {
        match simple {
            &Simple::Ident(s) | &Simple::Number(s) => {
                let mut iter = s.chars();
                let first = iter.next();
                if iter.next().is_none() { first } else { None }
            }
            _ => None,
        }
    } else {
        None
    }
}

pub fn extract_simple_str<'a>(simple: &Simple<'a>, strip: bool) -> Option<&'a str> {
    match simple {
        &Simple::Number(n) => Some(n),
        &Simple::Ident(i) => Some(i),
        Simple::Group(g) if strip => {
            if let [
                Intermediate::ScriptFunc(ScriptFunc::Simple(SimpleScript {
                    simple,
                    script: Script::None,
                })),
            ] = &*g.expr
            {
                extract_simple_str(simple, false)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn extract_vulgar_frac(numer: &Simple<'_>, denom: &Simple<'_>, strip: bool) -> Option<char> {
    let num = extract_simple_str(numer, strip)?;
    let den = extract_simple_str(denom, strip)?;
    vulgar_frac_char(num, den)
}
