//! Definitions of the relevant tokens and conversions between them

use super::SkinTone;
use asciimath_parser::prefix_map::HashPrefixMap;
use asciimath_parser::tree::Expression;
use asciimath_parser::{ASCIIMATH_TOKENS, SymbolClass, Token, Tokenizer, asciimath_spellings};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Every spelling the tokenizer knows: the parser's and emoji shortcodes
static TOKEN_MAP: LazyLock<HashPrefixMap<Cow<'static, str>, Token>> = LazyLock::new(|| {
    asciimath_spellings()
        .map(|(spelling, token)| (Cow::Borrowed(spelling), token))
        .chain(emojis::iter().flat_map(|emoji| {
            emoji.shortcodes().map(|code| {
                (
                    Cow::Owned(format!(":{code}:")),
                    Token::Symbol(SymbolClass::Glyph),
                )
            })
        }))
        .collect()
});

/// The unicode form of each spelling that renders as something other than typed
static UNICODE_FORMS: LazyLock<HashMap<&str, &str>> = LazyLock::new(|| {
    ASCIIMATH_TOKENS
        .iter()
        .filter_map(|entry| entry.unicode.map(|unicode| (entry.spelling, unicode)))
        .collect()
});

/// The unicode form of a symbol, or of an emoji shortcode in the given skin tone
pub(crate) fn symbol_str(inp: &str, skin_tone: SkinTone) -> &str {
    if let Some(unicode) = UNICODE_FORMS.get(inp) {
        unicode
    } else {
        inp.strip_prefix(':')
            .and_then(|code| code.strip_suffix(':'))
            .and_then(emojis::get_by_shortcode)
            .map_or(inp, |emoji| {
                emoji
                    .with_skin_tone(skin_tone.into())
                    .or_else(|| emoji.with_skin_tone(first_tone(skin_tone).into()))
                    .unwrap_or(emoji)
                    .as_str()
            })
    }
}

/// The tone of the first person of a two-person tone, for an emoji of one person
fn first_tone(skin_tone: SkinTone) -> SkinTone {
    match skin_tone {
        SkinTone::LightAndMediumLight
        | SkinTone::LightAndMedium
        | SkinTone::LightAndMediumDark
        | SkinTone::LightAndDark => SkinTone::Light,
        SkinTone::MediumLightAndLight
        | SkinTone::MediumLightAndMedium
        | SkinTone::MediumLightAndMediumDark
        | SkinTone::MediumLightAndDark => SkinTone::MediumLight,
        SkinTone::MediumAndLight
        | SkinTone::MediumAndMediumLight
        | SkinTone::MediumAndMediumDark
        | SkinTone::MediumAndDark => SkinTone::Medium,
        SkinTone::MediumDarkAndLight
        | SkinTone::MediumDarkAndMediumLight
        | SkinTone::MediumDarkAndMedium
        | SkinTone::MediumDarkAndDark => SkinTone::MediumDark,
        SkinTone::DarkAndLight
        | SkinTone::DarkAndMediumLight
        | SkinTone::DarkAndMedium
        | SkinTone::DarkAndMediumDark => SkinTone::Dark,
        single => single,
    }
}

/// The unicode form of a bracket, empty for an invisible one
pub(crate) fn bracket_str(inp: &str) -> &str {
    UNICODE_FORMS.get(inp).copied().unwrap_or(inp)
}

/// The chars of the unicode form of every bracket in the parser's table
static BRACKET_CHARS: LazyLock<HashSet<char>> = LazyLock::new(|| {
    ASCIIMATH_TOKENS
        .iter()
        .filter(|entry| {
            matches!(
                entry.token,
                Token::OpenBracket | Token::CloseBracket | Token::OpenCloseBracket
            )
        })
        .flat_map(|entry| entry.unicode.unwrap_or(entry.spelling).chars())
        .collect()
});

/// Whether `chr` is part of a bracket
pub(crate) fn is_bracket_char(chr: char) -> bool {
    BRACKET_CHARS.contains(&chr)
}

/// A space one step narrower, for whitespace that lands in a raised or lowered part
///
/// Called only for whitespace, so anything without its own step, a tab or newline among them,
/// comes out thin.
fn narrower_space(space: char) -> char {
    match space {
        '\u{3000}' => '\u{2003}',
        '\u{2001}' | '\u{2003}' => '\u{2002}',
        '\u{2000}' | '\u{2002}' | '\u{2007}' => '\u{2004}',
        '\u{2004}' => '\u{2005}',
        '\u{205f}' | '\u{2006}' | '\u{2009}' | '\u{202f}' | '\u{200a}' => '\u{200a}',
        _ => '\u{2009}',
    }
}

/// The raised form of `inp`, if it has one
pub(crate) fn superscript_char(inp: char) -> Option<char> {
    match inp {
        'a' => Some('ᵃ'),
        'b' => Some('ᵇ'),
        'c' => Some('ᶜ'),
        'd' => Some('ᵈ'),
        'e' => Some('ᵉ'),
        'f' => Some('ᶠ'),
        'g' => Some('ᵍ'),
        'h' => Some('ʰ'),
        'i' => Some('ⁱ'),
        'j' => Some('ʲ'),
        'k' => Some('ᵏ'),
        'l' => Some('ˡ'),
        'm' => Some('ᵐ'),
        'n' => Some('ⁿ'),
        'o' => Some('ᵒ'),
        'p' => Some('ᵖ'),
        'r' => Some('ʳ'),
        's' => Some('ˢ'),
        't' => Some('ᵗ'),
        'u' => Some('ᵘ'),
        'v' => Some('ᵛ'),
        'w' => Some('ʷ'),
        'x' => Some('ˣ'),
        'y' => Some('ʸ'),
        'z' => Some('ᶻ'),
        'A' => Some('ᴬ'),
        'B' => Some('ᴮ'),
        'D' => Some('ᴰ'),
        'E' => Some('ᴱ'),
        'G' => Some('ᴳ'),
        'H' => Some('ᴴ'),
        'I' => Some('ᴵ'),
        'J' => Some('ᴶ'),
        'K' => Some('ᴷ'),
        'L' => Some('ᴸ'),
        'M' => Some('ᴹ'),
        'N' => Some('ᴺ'),
        'O' => Some('ᴼ'),
        'P' => Some('ᴾ'),
        'R' => Some('ᴿ'),
        'T' => Some('ᵀ'),
        'U' => Some('ᵁ'),
        'V' => Some('ⱽ'),
        'W' => Some('ᵂ'),
        '0' => Some('⁰'),
        '1' => Some('¹'),
        '2' => Some('²'),
        '3' => Some('³'),
        '4' => Some('⁴'),
        '5' => Some('⁵'),
        '6' => Some('⁶'),
        '7' => Some('⁷'),
        '8' => Some('⁸'),
        '9' => Some('⁹'),
        '+' => Some('⁺'),
        '-' => Some('⁻'),
        '=' => Some('⁼'),
        '(' => Some('⁽'),
        ')' => Some('⁾'),
        'α' => Some('ᵅ'),
        'β' => Some('ᵝ'),
        'γ' => Some('ᵞ'),
        'δ' => Some('ᵟ'),
        'ε' => Some('ᵋ'),
        'θ' => Some('ᶿ'),
        'ι' => Some('ᶥ'),
        'ϕ' => Some('ᶲ'),
        'φ' => Some('ᵠ'),
        'χ' => Some('ᵡ'),
        // already raised, or without a raised form but needed for legibility
        chr @ ('′' | ',') => Some(chr),
        space if space.is_whitespace() => Some(narrower_space(space)),
        _ => None,
    }
}

/// The lowered form of `inp`, if it has one
pub(crate) fn subscript_char(inp: char) -> Option<char> {
    match inp {
        'a' => Some('ₐ'),
        'e' => Some('ₑ'),
        'h' => Some('ₕ'),
        'i' => Some('ᵢ'),
        'k' => Some('ₖ'),
        'l' => Some('ₗ'),
        'm' => Some('ₘ'),
        'n' => Some('ₙ'),
        'o' => Some('ₒ'),
        'p' => Some('ₚ'),
        'r' => Some('ᵣ'),
        's' => Some('ₛ'),
        't' => Some('ₜ'),
        'u' => Some('ᵤ'),
        'v' => Some('ᵥ'),
        'j' => Some('ⱼ'),
        'x' => Some('ₓ'),
        '0' => Some('₀'),
        '1' => Some('₁'),
        '2' => Some('₂'),
        '3' => Some('₃'),
        '4' => Some('₄'),
        '5' => Some('₅'),
        '6' => Some('₆'),
        '7' => Some('₇'),
        '8' => Some('₈'),
        '9' => Some('₉'),
        '+' => Some('₊'),
        '-' => Some('₋'),
        '=' => Some('₌'),
        '(' => Some('₍'),
        ')' => Some('₎'),
        'β' => Some('ᵦ'),
        'γ' => Some('ᵧ'),
        'ρ' => Some('ᵨ'),
        'φ' => Some('ᵩ'),
        'χ' => Some('ᵪ'),
        // no subscript form, but needed to separate indices
        ',' => Some(','),
        space if space.is_whitespace() => Some(narrower_space(space)),
        _ => None,
    }
}

#[inline]
fn map_range(inp: char, from: char, to: char) -> char {
    char::from_u32((inp as u32) - (from as u32) + (to as u32)).unwrap()
}

/// `inp` in bold, keeping any other style it has
pub(crate) fn bold_map(inp: char) -> char {
    match inp {
        chr @ 'A'..='Z' => map_range(chr, 'A', '\u{1d400}'),
        chr @ 'a'..='z' => map_range(chr, 'a', '\u{1d41a}'),
        chr @ '0'..='9' => map_range(chr, '0', '\u{1d7ce}'),
        chr @ 'Α'..='Ω' => map_range(chr, '\u{0391}', '\u{1d6a8}'),
        'ϴ' => '\u{1d6b9}',
        chr @ 'α'..='ω' => map_range(chr, '\u{03b1}', '\u{1d6c2}'),
        '∂' => '𝛛',
        'ϵ' => '𝛜',
        'ϑ' => '𝛝',
        'ϰ' => '𝛞',
        'ϕ' => '𝛟',
        'ϱ' => '𝛠',
        'ϖ' => '𝛡',
        '∇' => '𝛁',
        'ℎ' => '\u{1d489}',
        chr @ '\u{1d434}'..='\u{1d467}' => map_range(chr, '\u{1d434}', '\u{1d468}'),
        chr @ '\u{1d6e2}'..='\u{1d71b}' => map_range(chr, '\u{1d6e2}', '\u{1d71c}'),
        '\u{212c}' => '\u{1d4d1}',
        '\u{2130}' => '\u{1d4d4}',
        '\u{2131}' => '\u{1d4d5}',
        '\u{210b}' => '\u{1d4d7}',
        '\u{2110}' => '\u{1d4d8}',
        '\u{2112}' => '\u{1d4db}',
        '\u{2133}' => '\u{1d4dc}',
        '\u{211b}' => '\u{1d4e1}',
        '\u{212f}' => '\u{1d4ee}',
        '\u{210a}' => '\u{1d4f0}',
        '\u{2134}' => '\u{1d4f8}',
        chr @ '\u{1d49c}'..='\u{1d4cf}' => map_range(chr, '\u{1d49c}', '\u{1d4d0}'),
        '\u{212d}' => '\u{1d56e}',
        '\u{210c}' => '\u{1d573}',
        '\u{2111}' => '\u{1d574}',
        '\u{211c}' => '\u{1d57d}',
        '\u{2128}' => '\u{1d585}',
        chr @ '\u{1d504}'..='\u{1d537}' => map_range(chr, '\u{1d504}', '\u{1d56c}'),
        chr @ '\u{1d5a0}'..='\u{1d5d3}' => map_range(chr, '\u{1d5a0}', '\u{1d5d4}'),
        chr @ '\u{1d7e2}'..='\u{1d7eb}' => map_range(chr, '\u{1d7e2}', '\u{1d7ec}'),
        chr @ '\u{1d608}'..='\u{1d63b}' => map_range(chr, '\u{1d608}', '\u{1d63c}'),
        chr => chr,
    }
}

/// `inp` in italic, keeping any other style it has
pub(crate) fn italic_map(inp: char) -> char {
    match inp {
        'h' => 'ℎ',
        chr @ 'A'..='Z' => map_range(chr, 'A', '\u{1d434}'),
        chr @ 'a'..='z' => map_range(chr, 'a', '\u{1d44e}'),
        chr @ 'Α'..='Ω' => map_range(chr, '\u{0391}', '\u{1d6e2}'),
        'ϴ' => '\u{1d6f3}',
        chr @ 'α'..='ω' => map_range(chr, '\u{03b1}', '\u{1d6fc}'),
        '∂' => '𝜕',
        'ϵ' => '𝜖',
        'ϑ' => '𝜗',
        'ϰ' => '𝜘',
        'ϕ' => '𝜙',
        'ϱ' => '𝜚',
        'ϖ' => '𝜛',
        '∇' => '𝛻',
        chr @ '\u{1d400}'..='\u{1d433}' => map_range(chr, '\u{1d400}', '\u{1d468}'),
        chr @ '\u{1d6a8}'..='\u{1d6e1}' => map_range(chr, '\u{1d6a8}', '\u{1d71c}'),
        chr @ '\u{1d5a0}'..='\u{1d5d3}' => map_range(chr, '\u{1d5a0}', '\u{1d608}'),
        chr @ '\u{1d5d4}'..='\u{1d607}' => map_range(chr, '\u{1d5d4}', '\u{1d63c}'),
        chr @ '\u{1d756}'..='\u{1d78f}' => map_range(chr, '\u{1d756}', '\u{1d790}'),
        '\u{1d53b}' => '\u{2145}',
        '\u{1d555}' => '\u{2146}',
        '\u{1d556}' => '\u{2147}',
        '\u{1d55a}' => '\u{2148}',
        '\u{1d55b}' => '\u{2149}',
        chr => chr,
    }
}

/// `inp` in calligraphic script, bold kept
pub(crate) fn cal_map(inp: char) -> char {
    match inp {
        'B' => '\u{212c}',
        'E' => '\u{2130}',
        'F' => '\u{2131}',
        'H' => '\u{210b}',
        'I' => '\u{2110}',
        'L' => '\u{2112}',
        'M' => '\u{2133}',
        'R' => '\u{211b}',
        'e' => '\u{212f}',
        'g' => '\u{210a}',
        'o' => '\u{2134}',
        chr @ 'A'..='Z' => map_range(chr, 'A', '\u{1d49c}'),
        chr @ 'a'..='z' => map_range(chr, 'a', '\u{1d4b6}'),
        chr @ '\u{1d400}'..='\u{1d433}' => map_range(chr, '\u{1d400}', '\u{1d4d0}'),
        chr => chr,
    }
}

/// `inp` in fraktur, bold kept
pub(crate) fn frak_map(inp: char) -> char {
    match inp {
        'C' => '\u{212d}',
        'H' => '\u{210c}',
        'I' => '\u{2111}',
        'R' => '\u{211c}',
        'Z' => '\u{2128}',
        chr @ 'A'..='Z' => map_range(chr, 'A', '\u{1d504}'),
        chr @ 'a'..='z' => map_range(chr, 'a', '\u{1d51e}'),
        chr @ '\u{1d400}'..='\u{1d433}' => map_range(chr, '\u{1d400}', '\u{1d56c}'),
        chr => chr,
    }
}

/// `inp` double-struck
pub(crate) fn double_map(inp: char) -> char {
    match inp {
        'C' => '\u{2102}',
        'H' => '\u{210d}',
        'N' => '\u{2115}',
        'P' => '\u{2119}',
        'Q' => '\u{211a}',
        'R' => '\u{211d}',
        'Z' => '\u{2124}',
        chr @ 'A'..='Z' => map_range(chr, 'A', '\u{1d538}'),
        chr @ 'a'..='z' => map_range(chr, 'a', '\u{1d552}'),
        chr @ '0'..='9' => map_range(chr, '0', '\u{1d7d8}'),
        'π' => '\u{213c}',
        'γ' => '\u{213d}',
        'Γ' => '\u{213e}',
        'Π' => '\u{213f}',
        '∑' => '\u{2140}',
        '\u{1d437}' => '\u{2145}',
        '\u{1d451}' => '\u{2146}',
        '\u{1d452}' => '\u{2147}',
        '\u{1d456}' => '\u{2148}',
        '\u{1d457}' => '\u{2149}',
        chr => chr,
    }
}

/// `inp` in sans-serif, keeping any other style it has
pub(crate) fn sans_map(inp: char) -> char {
    match inp {
        chr @ 'A'..='Z' => map_range(chr, 'A', '\u{1d5a0}'),
        chr @ 'a'..='z' => map_range(chr, 'a', '\u{1d5ba}'),
        chr @ '0'..='9' => map_range(chr, '0', '\u{1d7e2}'),
        chr @ '\u{1d400}'..='\u{1d433}' => map_range(chr, '\u{1d400}', '\u{1d5d4}'),
        chr @ '\u{1d7ce}'..='\u{1d7d7}' => map_range(chr, '\u{1d7ce}', '\u{1d7ec}'),
        chr @ '\u{1d6a8}'..='\u{1d6e1}' => map_range(chr, '\u{1d6a8}', '\u{1d756}'),
        'ℎ' => '\u{1d629}',
        chr @ '\u{1d434}'..='\u{1d467}' => map_range(chr, '\u{1d434}', '\u{1d608}'),
        chr @ '\u{1d468}'..='\u{1d49b}' => map_range(chr, '\u{1d468}', '\u{1d63c}'),
        chr @ '\u{1d71c}'..='\u{1d755}' => map_range(chr, '\u{1d71c}', '\u{1d790}'),
        chr => chr,
    }
}

/// `inp` in monospace
pub(crate) fn mono_map(inp: char) -> char {
    match inp {
        chr @ 'A'..='Z' => map_range(chr, 'A', '\u{1d670}'),
        chr @ 'a'..='z' => map_range(chr, 'a', '\u{1d68a}'),
        chr @ '0'..='9' => map_range(chr, '0', '\u{1d7f6}'),
        chr => chr,
    }
}

/// Parse `inp`, with the whitespace between items of an expression only if `keep_spaces`
pub(crate) fn parse(inp: &str, keep_spaces: bool) -> Expression<'_> {
    asciimath_parser::parse_tokens(
        Tokenizer::with_tokens(inp, &*TOKEN_MAP, true)
            .filter(|&(_, token)| keep_spaces || token != Token::Space),
    )
}

#[cfg(test)]
mod tests {
    use super::{ASCIIMATH_TOKENS, SkinTone, Token};
    use std::collections::HashSet;

    #[test]
    fn mapping() {
        // the tokens whose unicode form is the token itself
        const WRITTEN_AS_TYPED: [&str; 21] = [
            "/", "^", "_", ",", "+", "-", "−", "=", ":=", "<", ">", "!", "%", "(", "[", "{", ")",
            "]", "}", "|", "||",
        ];
        for entry in &ASCIIMATH_TOKENS {
            let string = entry.spelling;
            match entry.token {
                Token::Symbol(class) if class.is_name() => assert_eq!(
                    entry.unicode, None,
                    "{string:?} is written as letters but renders as something else"
                ),
                Token::OpenBracket
                | Token::CloseBracket
                | Token::OpenCloseBracket
                | Token::Symbol(_)
                | Token::Sign
                | Token::Frac
                | Token::Super
                | Token::Sub
                | Token::Sep => assert!(
                    entry.unicode.is_some() || WRITTEN_AS_TYPED.contains(&string),
                    "{string:?} has no unicode form"
                ),
                _ => {}
            }
        }
    }

    #[test]
    fn unknown_symbol_passes_through() {
        assert_eq!(super::symbol_str("", SkinTone::Default), "");
        assert_eq!(super::symbol_str("x", SkinTone::Default), "x");
        assert_eq!(
            super::symbol_str(":notanemoji:", SkinTone::Default),
            ":notanemoji:"
        );
        assert_eq!(super::symbol_str(":hand:", SkinTone::Default), "✋");
    }

    #[test]
    fn fonts() {
        let mut chars: HashSet<char> = ('A'..='Z')
            .chain('a'..='z')
            .chain('0'..='9')
            .chain('Α'..='Ω')
            .chain('α'..='ω')
            .chain(['∂', 'ϵ', 'ϑ', 'ϰ', 'ϕ', 'ϱ', 'ϖ', '∇'])
            .collect();
        for _ in 0..3 {
            let mut new_chars = HashSet::new();
            for chr in chars {
                for func in [
                    super::bold_map,
                    super::italic_map,
                    super::cal_map,
                    super::frak_map,
                    super::double_map,
                    super::sans_map,
                    super::mono_map,
                ] {
                    let styled = func(chr);
                    // a font it already applied must leave the char alone
                    assert_eq!(func(styled), styled, "{chr:?} restyled to {styled:?}");
                    new_chars.insert(styled);
                }
            }
            chars = new_chars;
        }
    }

    #[test]
    fn bold_lowercase_greek() {
        // the bold small greek block starts at MATHEMATICAL BOLD SMALL ALPHA (U+1D6C2)
        assert_eq!(super::bold_map('α'), '\u{1d6c2}');
        assert_eq!(super::bold_map('β'), '\u{1d6c3}');
        assert_eq!(super::bold_map('ω'), '\u{1d6da}');
    }

    #[test]
    fn bold_fraktur_letterlike() {
        // bold-fraktur of the letterlike ℌ (U+210C) is BOLD FRAKTUR CAPITAL H (U+1D573)
        assert_eq!(super::bold_map('\u{210c}'), '\u{1d573}');
        // a literal left double quotation mark must pass through untouched
        assert_eq!(super::bold_map('\u{201c}'), '\u{201c}');
    }

    #[test]
    fn superscript_mappings() {
        for chr in 'a'..='z' {
            if chr != 'q' {
                assert!(
                    super::superscript_char(chr).is_some(),
                    "missing superscript for '{chr}'"
                );
            }
        }
        for chr in '0'..='9' {
            assert!(super::superscript_char(chr).is_some());
        }
        for chr in ['+', '-', '=', '(', ')'] {
            assert!(super::superscript_char(chr).is_some());
        }
        assert!(super::superscript_char(' ').is_some());
        assert!(super::superscript_char('!').is_none());
    }

    #[test]
    fn subscript_mappings() {
        for chr in [
            'a', 'e', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'r', 's', 't', 'u', 'v', 'x',
        ] {
            assert!(
                super::subscript_char(chr).is_some(),
                "missing subscript for '{chr}'"
            );
        }
        for chr in '0'..='9' {
            assert!(super::subscript_char(chr).is_some());
        }
        for chr in ['+', '-', '=', '(', ')'] {
            assert!(super::subscript_char(chr).is_some());
        }
        assert!(super::subscript_char(' ').is_some());
        assert!(super::subscript_char('z').is_none());
    }

    #[test]
    fn whitespace_steps_narrower_in_a_script() {
        for (typed, narrower) in [
            ('\u{3000}', '\u{2003}'),
            ('\u{2001}', '\u{2002}'),
            ('\u{2003}', '\u{2002}'),
            ('\u{2000}', '\u{2004}'),
            ('\u{2002}', '\u{2004}'),
            ('\u{2007}', '\u{2004}'),
            ('\u{2004}', '\u{2005}'),
            ('\u{0020}', '\u{2009}'),
            ('\u{00a0}', '\u{2009}'),
            ('\u{2005}', '\u{2009}'),
            ('\u{2008}', '\u{2009}'),
            ('\t', '\u{2009}'),
            ('\n', '\u{2009}'),
            ('\u{205f}', '\u{200a}'),
            ('\u{2006}', '\u{200a}'),
            ('\u{2009}', '\u{200a}'),
            ('\u{202f}', '\u{200a}'),
            ('\u{200a}', '\u{200a}'),
        ] {
            assert_eq!(
                super::superscript_char(typed),
                Some(narrower),
                "superscript of {typed:?}"
            );
            assert_eq!(
                super::subscript_char(typed),
                Some(narrower),
                "subscript of {typed:?}"
            );
        }
    }
}
