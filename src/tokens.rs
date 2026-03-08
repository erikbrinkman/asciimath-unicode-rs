//! Definitions of the relevant tokens and conversions between them

use asciimath_parser::prefix_map::QpTriePrefixMap;
use asciimath_parser::tree::Expression;
use asciimath_parser::{Token, Tokenizer};
use emojis::SkinTone;
use std::borrow::Cow;
use std::sync::LazyLock;

macro_rules! tokens {
    ($($type:ident => $($str:expr),+;)+) => {
        [
            $(
                $(
                    ($str, Token::$type),
                )+
            )+
        ]
    };
}

const UNICODE_TOKENS: [(&str, Token); 399] = tokens!(
    Frac => "/";
    Super => "^";
    Sub => "_";
    Sep => ",";
    Function => "sin", "cos", "tan", "sinh", "cosh", "tanh", "cot", "sec", "csc", "arcsin",
        "arccos", "arctan", "coth", "sech", "csch", "exp", "log", "ln", "det", "gcd", "lcm", "Sin",
        "Cos", "Tan", "Arcsin", "Arccos", "Arctan", "Sinh", "Cosh", "Tanh", "Cot", "Sec", "Csc",
        "Log", "Ln", "f", "g";
    Unary => "sqrt", "abs", "norm", "floor", "ceil", "Abs", "hat", "bar", "overline", "vec", "dot",
        "ddot", "overarc", "overparen", "ul", "underline", "ubrace", "underbrace", "obrace",
        "overbrace", "text", "mbox", "cancel", "tilde";
    // font commands
    Unary => "bb", "mathbf", "sf", "mathsf", "bbb", "mathbb", "cc", "mathcal", "tt", "mathtt",
        "fr", "mathfrak", "it", "mathit";
    Binary => "frac", "root", "stackrel", "overset", "underset", "color", "id", "class";
    // greek symbols
    Symbol => "alpha", "Alpha", "beta", "Beta", "chi", "Chi", "delta", "Delta", "epsi", "Epsi",
        "epsilon", "Epsilon", "varepsilon", "eta", "Eta", "gamma", "Gamma", "iota", "Iota",
        "kappa", "Kappa","varkappa", "lambda", "Lambda", "lamda", "Lamda", "mu", "Mu", "nu", "Nu",
        "omega", "Omega", "phi", "varphi", "Phi", "pi", "Pi", "varpi", "psi", "Psi", "rho", "Rho",
        "varrho", "sigma", "Sigma", "tau", "Tau", "theta", "vartheta", "Theta","Vartheta",
        "upsilon", "Upsilon", "xi", "Xi", "zeta", "Zeta";
    // operations
    Symbol => "*", "cdot", "**", "ast", "***", "star", "//", "\\\\", "backslash", "setminus", "xx",
        "times", "|><", "ltimes", "><|", "rtimes", "|><|", "bowtie", "-:", "div", "divide", "@",
        "circ", "o+", "oplus", "ox", "otimes", "o.", "odot", "sum", "prod", "^^", "wedge", "land",
        "^^^", "bigwedge", "vv", "vee", "lor", "vvv", "bigvee", "nn", "cap", "nnn", "bigcap", "uu",
        "cup", "uuu", "bigcup", "ominus", "oslash";
    // relations
    Symbol => "=", "!=", "ne", "<", "lt", "<=", "le", "lt=", "leq", ">", "gt", "mlt", "ll", ">=", "ge",
        "gt=", "geq", "mgt", "gg", "-<", "prec", "-lt", ">-", "succ", "-<=", "preceq", ">-=",
        "succeq", "in", "!in", "notin", "sub", "subset", "sup", "supset", "sube", "subseteq",
        "supe", "supseteq",
        "!sub", "nsub", "!sup", "nsup", "!sube", "nsubseteq", "!supe", "nsupseteq",
        "-=", "equiv", "~=", "cong", "~~", "approx", "~", "sim",
        "prop", "propto";
    // logical
    Symbol => "not", "neg", "=>", "implies", "<=>", "iff", "AA", "forall", "EE", "exists", "!EE",
        "notexists", "_|_", "bot", "TT", "top", "|--", "vdash", "|==", "models";
    Symbol => "and", "or", "if";
    // misc
    Symbol => ":|:", "int", "oint", "iint", "iiint", "oiint", "oiiint",
        "del", "partial", "grad", "nabla", "+-", "pm", "-+", "mp",
        "O/", "emptyset", "oo", "infty", "aleph", "...", "ldots", ":.", "therefore", ":'",
        "because", "/_", "angle", "/_\\", "triangle", "'", "prime", "\\ ", "frown",
        "quad", "qquad", "cdots", "vdots", "ddots", "diamond", "square", "CC", "NN", "QQ", "RR",
        "ZZ", "ell";
    // arrows
    Symbol => "uarr", "uparrow", "uArr", "Uparrow", "darr", "downarrow", "dArr", "Downarrow",
        "rarr", "rightarrow", "->", "to", ">->",
        "rightarrowtail", "->>", "twoheadrightarrow", ">->>", "twoheadrightarrowtail", "|->",
        "mapsto", "larr", "leftarrow", "<-", "harr", "leftrightarrow", "<->", "rArr", "Rightarrow",
        "==>", "lArr", "Leftarrow","<==",  "hArr", "Leftrightarrow", "<==>";
    // brackets
    OpenBracket => "(", "[", "{", "|:", "(:", "<<", "langle", "left(", "left[", "{:", "|__",
        "lfloor", "|~", "lceiling";
    // right solution
    CloseBracket => ")", "]", "}", ":|", ":)", ">>", "rangle", "right)", "right]", ":}",
        "__|", "rfloor", "~|", "rceiling";
    OpenCloseBracket => "|";
    // defined identifiers
    Ident => "dx", "dy", "dz", "dt";
    // underover
    Ident => "lim", "Lim", "dim", "mod", "lub", "glb", "min", "max";
    // Misc
    Ident => ":=";
);

pub static TOKEN_MAP: LazyLock<QpTriePrefixMap<Cow<'static, str>, Token>> = LazyLock::new(|| {
    UNICODE_TOKENS
        .into_iter()
        .map(|(name, tok)| (Cow::Borrowed(name), tok))
        .chain(emojis::iter().flat_map(|emoji| {
            emoji
                .shortcodes()
                .map(|code| (Cow::Owned(format!(":{code}:")), Token::Symbol))
        }))
        .collect()
});

pub fn superscript_char(inp: char) -> Option<char> {
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
        c if c.is_whitespace() => Some(c),
        _ => None,
    }
}

pub fn subscript_char(inp: char) -> Option<char> {
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
        c if c.is_whitespace() => Some(c),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
pub fn symbol_str(inp: &str, skin_tone: SkinTone) -> &str {
    match inp {
        "/" | "//" => "/",
        "^" => "^",
        "_" => "_",
        "," => ",",
        // greek
        "alpha" => "α",
        "Alpha" => "Α",
        "beta" => "β",
        "Beta" => "Β",
        "chi" => "χ",
        "Chi" => "Χ",
        "delta" => "δ",
        "Delta" => "Δ",
        "epsi" | "epsilon" => "ε",
        "Epsi" | "Epsilon" => "Ε",
        "varepsilon" => "ϵ",
        "eta" => "η",
        "Eta" => "Η",
        "gamma" => "γ",
        "Gamma" => "Γ",
        "iota" => "ι",
        "Iota" => "Ι",
        "kappa" => "κ",
        "Kappa" => "Κ",
        "varkappa" => "ϰ",
        "lambda" | "lamda" => "λ",
        "Lambda" | "Lamda" => "Λ",
        "mu" => "μ",
        "Mu" => "Μ",
        "nu" => "ν",
        "Nu" => "Ν",
        "omega" => "ω",
        "Omega" => "Ω",
        "phi" => "φ",
        "varphi" => "ϕ",
        "Phi" => "Φ",
        "pi" => "π",
        "Pi" => "Π",
        "varpi" => "ϖ",
        "psi" => "ψ",
        "Psi" => "Ψ",
        "rho" => "ρ",
        "Rho" => "Ρ",
        "varrho" => "ϱ",
        "sigma" => "σ",
        "Sigma" => "Σ",
        "tau" => "τ",
        "Tau" => "Τ",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "Theta" => "Θ",
        "Vartheta" => "ϴ",
        "upsilon" => "υ",
        "Upsilon" => "Υ",
        "xi" => "ξ",
        "Xi" => "Ξ",
        "zeta" => "ζ",
        "Zeta" => "Ζ",
        // operations
        "*" | "cdot" => "⋅",
        "**" | "ast" => "∗",
        "***" | "star" => "⋆",
        "\\\\" | "backslash" | "setminus" => "\\",
        "xx" | "times" => "×",
        "|><" | "ltimes" => "⋉",
        "><|" | "rtimes" => "⋊",
        "|><|" | "bowtie" => "⋈",
        "-:" | "div" | "divide" => "÷",
        "@" | "circ" => "∘",
        "o+" | "oplus" => "⊕",
        "ox" | "otimes" => "⊗",
        "o." | "odot" => "⊙",
        "sum" => "∑",
        "prod" => "∏",
        "^^" | "wedge" | "land" => "∧",
        "^^^" | "bigwedge" => "⋀",
        "vv" | "vee" | "lor" => "∨",
        "vvv" | "bigvee" => "⋁",
        "nn" | "cap" => "∩",
        "nnn" | "bigcap" => "⋂",
        "uu" | "cup" => "∪",
        "uuu" | "bigcup" => "⋃",
        "ominus" => "⊖",
        "oslash" => "⊘",
        // relations
        "=" => "=",
        "!=" | "ne" => "≠",
        "lt" | "<" => "<",
        "<=" | "le" | "lt=" | "leq" => "≤",
        "gt" | ">" => ">",
        "mlt" | "ll" => "≪",
        ">=" | "ge" | "gt=" | "geq" => "≥",
        "mgt" | "gg" => "≫",
        "-<" | "prec" | "-lt" => "≺",
        ">-" | "succ" => "≻",
        "-<=" | "preceq" => "⪯",
        ">-=" | "succeq" => "⪰",
        "in" => "∈",
        "!in" | "notin" => "∉",
        "sub" | "subset" => "⊂",
        "sup" | "supset" => "⊃",
        "sube" | "subseteq" => "⊆",
        "supe" | "supseteq" => "⊇",
        "!sub" | "nsub" => "⊄",
        "!sup" | "nsup" => "⊅",
        "!sube" | "nsubseteq" => "⊈",
        "!supe" | "nsupseteq" => "⊉",
        "-=" | "equiv" => "≡",
        "~=" | "cong" => "≅",
        "~~" | "approx" => "≈",
        "~" | "sim" => "~",
        "prop" | "propto" => "∝",
        // logical
        "not" | "neg" => "¬",
        "=>" | "implies" | "rArr" | "Rightarrow" | "==>" => "⇒",
        "<=>" | "iff" | "hArr" | "Leftrightarrow" | "<==>" => "⇔",
        "AA" | "forall" => "∀",
        "EE" | "exists" => "∃",
        "!EE" | "notexists" => "∄",
        "_|_" | "bot" => "⊥",
        "TT" | "top" => "⊤",
        "|--" | "vdash" => "⊢",
        "|==" | "models" => "⊨",
        "and" => " and ",
        "or" => " or ",
        "if" => " if ",
        // misc
        ":|:" | "|" => "|",
        "int" => "∫",
        "oint" => "∮",
        "iint" => "∬",
        "iiint" => "∭",
        "oiint" => "∯",
        "oiiint" => "∰",
        "del" | "partial" => "∂",
        "grad" | "nabla" => "∇",
        "+-" | "pm" => "±",
        "-+" | "mp" => "∓",
        "O/" | "emptyset" => "∅",
        "oo" | "infty" => "∞",
        "aleph" => "ℵ",
        "..." | "ldots" => "…",
        ":." | "therefore" => "∴",
        ":'" | "because" => "∵",
        "/_" | "angle" => "∠",
        "/_\\" | "triangle" => "△",
        "'" | "prime" => "'",
        "\\ " | "quad" | "qquad" => " ",
        "frown" => "⌢",
        "cdots" => "⋯",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "diamond" => "⋄",
        "square" => "□",
        "CC" => "ℂ",
        "NN" => "ℕ",
        "QQ" => "ℚ",
        "RR" => "ℝ",
        "ZZ" => "ℤ",
        "ell" => "ℓ",
        // arrows
        "uarr" | "uparrow" => "↑",
        "uArr" | "Uparrow" => "⇑",
        "darr" | "downarrow" => "↓",
        "dArr" | "Downarrow" => "⇓",
        "rarr" | "rightarrow" | "->" | "to" => "→",
        ">->" | "rightarrowtail" => "↣",
        "->>" | "twoheadrightarrow" => "↠",
        ">->>" | "twoheadrightarrowtail" => "⤖",
        "|->" | "mapsto" => "↦",
        "larr" | "leftarrow" | "<-" => "←",
        "harr" | "leftrightarrow" | "<->" => "↔",
        "lArr" | "Leftarrow" | "<==" => "⇐",
        // emoji
        chr => {
            let emoji = emojis::get_by_shortcode(&chr[1..chr.len() - 1]).unwrap();
            emoji.with_skin_tone(skin_tone).unwrap_or(emoji).as_str()
        }
    }
}

pub fn left_bracket_str(inp: &str) -> &str {
    match inp {
        "(" | "left(" => "(",
        "[" | "left[" => "[",
        "{" => "{",
        "{:" | "" => "",
        "(:" | "langle" | "<<" => "⟨",
        "|__" | "lfloor" => "⌊",
        "|~" | "lceiling" => "⌈",
        "|:" | "|" => "|",
        _ => unreachable!("matches all valid left bracket strs"),
    }
}

pub fn right_bracket_str(inp: &str) -> &str {
    match inp {
        ")" | "right)" => ")",
        "]" | "right]" => "]",
        "}" => "}",
        ":}" | "" => "",
        ":)" | "rangle" | ">>" => "⟩",
        "__|" | "rfloor" => "⌋",
        "~|" | "rceiling" => "⌉",
        ":|" | "|" => "|",
        _ => unreachable!("matches all valid right bracket strs"),
    }
}

#[inline]
fn map_range(inp: char, from: char, to: char) -> char {
    char::from_u32((inp as u32) - (from as u32) + (to as u32)).unwrap()
}

pub fn bold_map(inp: char) -> char {
    match inp {
        // regular
        c @ 'A'..='Z' => map_range(c, 'A', '\u{1d400}'),
        c @ 'a'..='z' => map_range(c, 'a', '\u{1d41a}'),
        c @ '0'..='9' => map_range(c, '0', '\u{1d7ce}'),
        c @ 'Α'..='Ω' => map_range(c, '\u{0391}', '\u{1d6a8}'),
        'ϴ' => '\u{1d6b9}',
        c @ 'α'..='ω' => map_range(c, '\u{03b1}', '\u{1d6da}'),
        '∂' => '𝛛',
        'ϵ' => '𝛜',
        'ϑ' => '𝛝',
        'ϰ' => '𝛞',
        'ϕ' => '𝛟',
        'ϱ' => '𝛠',
        'ϖ' => '𝛡',
        '∇' => '𝛁',
        // italic
        'ℎ' => '\u{1d489}',
        c @ '\u{1d434}'..='\u{1d467}' => map_range(c, '\u{1d434}', '\u{1d468}'),
        c @ '\u{1d6e2}'..='\u{1d71b}' => map_range(c, '\u{1d6e2}', '\u{1d71c}'),
        // cal
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
        c @ '\u{1d49c}'..='\u{1d4cf}' => map_range(c, '\u{1d49c}', '\u{1d4d0}'),
        // frak
        '\u{212d}' => '\u{1d56e}',
        '\u{201c}' => '\u{1d573}',
        '\u{2111}' => '\u{1d574}',
        '\u{211c}' => '\u{1d57d}',
        '\u{2128}' => '\u{1d585}',
        c @ '\u{1d504}'..='\u{1d537}' => map_range(c, '\u{1d504}', '\u{1d56c}'),
        // sans
        c @ '\u{1d5a0}'..='\u{1d5d3}' => map_range(c, '\u{1d5a0}', '\u{1d5d4}'),
        c @ '\u{1d7e2}'..='\u{1d7eb}' => map_range(c, '\u{1d7e2}', '\u{1d7ec}'),
        // italic sans
        c @ '\u{1d608}'..='\u{1d63b}' => map_range(c, '\u{1d608}', '\u{1d63c}'),
        // rest
        c => c,
    }
}

pub fn italic_map(inp: char) -> char {
    match inp {
        // letterlike
        'h' => 'ℎ',
        // regular
        c @ 'A'..='Z' => map_range(c, 'A', '\u{1d434}'),
        c @ 'a'..='z' => map_range(c, 'a', '\u{1d44e}'),
        c @ 'Α'..='Ω' => map_range(c, '\u{0391}', '\u{1d6e2}'),
        'ϴ' => '\u{1d6f3}',
        c @ 'α'..='ω' => map_range(c, '\u{03b1}', '\u{1d6fc}'),
        '∂' => '𝜕',
        'ϵ' => '𝜖',
        'ϑ' => '𝜗',
        'ϰ' => '𝜘',
        'ϕ' => '𝜙',
        'ϱ' => '𝜚',
        'ϖ' => '𝜛',
        '∇' => '𝛻',
        // bold
        c @ '\u{1d400}'..='\u{1d433}' => map_range(c, '\u{1d400}', '\u{1d468}'),
        c @ '\u{1d6a8}'..='\u{1d6e1}' => map_range(c, '\u{1d6a8}', '\u{1d71c}'),
        // sans
        c @ '\u{1d5a0}'..='\u{1d5d3}' => map_range(c, '\u{1d5a0}', '\u{1d608}'),
        // bold sans
        c @ '\u{1d5d4}'..='\u{1d607}' => map_range(c, '\u{1d5d4}', '\u{1d63c}'),
        c @ '\u{1d756}'..='\u{1d78f}' => map_range(c, '\u{1d756}', '\u{1d790}'),
        // extra
        '\u{1d53b}' => '\u{2145}',
        '\u{1d555}' => '\u{2146}',
        '\u{1d556}' => '\u{2147}',
        '\u{1d55a}' => '\u{2148}',
        '\u{1d55b}' => '\u{2149}',
        // rest
        c => c,
    }
}

pub fn cal_map(inp: char) -> char {
    match inp {
        // letterlike
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
        // regular
        c @ 'A'..='Z' => map_range(c, 'A', '\u{1d49c}'),
        c @ 'a'..='z' => map_range(c, 'a', '\u{1d4b6}'),
        // bold
        c @ '\u{1d400}'..='\u{1d433}' => map_range(c, '\u{1d400}', '\u{1d4d0}'),
        // rest
        c => c,
    }
}

pub fn frak_map(inp: char) -> char {
    match inp {
        // letterlike
        'C' => '\u{212d}',
        'H' => '\u{201c}',
        'I' => '\u{2111}',
        'R' => '\u{211c}',
        'Z' => '\u{2128}',
        // regular
        c @ 'A'..='Z' => map_range(c, 'A', '\u{1d504}'),
        c @ 'a'..='z' => map_range(c, 'a', '\u{1d51e}'),
        // bold
        c @ '\u{1d400}'..='\u{1d433}' => map_range(c, '\u{1d400}', '\u{1d56c}'),
        // rest
        c => c,
    }
}

pub fn double_map(inp: char) -> char {
    match inp {
        // letterlike
        'C' => '\u{2102}',
        'H' => '\u{210d}',
        'N' => '\u{2115}',
        'P' => '\u{2119}',
        'Q' => '\u{211a}',
        'R' => '\u{211d}',
        'Z' => '\u{2124}',
        // regular
        c @ 'A'..='Z' => map_range(c, 'A', '\u{1d538}'),
        c @ 'a'..='z' => map_range(c, 'a', '\u{1d552}'),
        c @ '0'..='9' => map_range(c, '0', '\u{1d7d8}'),
        // extras
        'π' => '\u{213c}',
        'γ' => '\u{213d}',
        'Π' => '\u{213e}',
        'Γ' => '\u{213f}',
        '∑' => '\u{2140}',
        '\u{1d437}' => '\u{2145}',
        '\u{1d451}' => '\u{2146}',
        '\u{1d452}' => '\u{2147}',
        '\u{1d456}' => '\u{2148}',
        '\u{1d457}' => '\u{2149}',
        // rest
        c => c,
    }
}

pub fn sans_map(inp: char) -> char {
    match inp {
        // regular
        c @ 'A'..='Z' => map_range(c, 'A', '\u{1d5a0}'),
        c @ 'a'..='z' => map_range(c, 'a', '\u{1d5ba}'),
        c @ '0'..='9' => map_range(c, '0', '\u{1d7e2}'),
        // bold
        c @ '\u{1d400}'..='\u{1d433}' => map_range(c, '\u{1d400}', '\u{1d5d4}'),
        c @ '\u{1d7ce}'..='\u{1d7d7}' => map_range(c, '\u{1d7ce}', '\u{1d7ec}'),
        c @ '\u{1d6a8}'..='\u{1d6e1}' => map_range(c, '\u{1d6a8}', '\u{1d756}'),
        // italic
        'ℎ' => '\u{1d629}',
        c @ '\u{1d434}'..='\u{1d467}' => map_range(c, '\u{1d434}', '\u{1d608}'),
        // bold italic
        c @ '\u{1d468}'..='\u{1d49b}' => map_range(c, '\u{1d468}', '\u{1d63c}'),
        c @ '\u{1d71c}'..='\u{1d755}' => map_range(c, '\u{1d71c}', '\u{1d790}'),
        // rest
        c => c,
    }
}

pub fn mono_map(inp: char) -> char {
    match inp {
        // regular
        c @ 'A'..='Z' => map_range(c, 'A', '\u{1d670}'),
        c @ 'a'..='z' => map_range(c, 'a', '\u{1d68a}'),
        c @ '0'..='9' => map_range(c, '0', '\u{1d7f6}'),
        // rest
        c => c,
    }
}

pub fn parse(inp: &str) -> Expression<'_> {
    asciimath_parser::parse_tokens(Tokenizer::with_tokens(inp, &*TOKEN_MAP, true))
}

#[cfg(test)]
mod tests {
    use super::{SkinTone, Token, UNICODE_TOKENS};
    use std::collections::HashSet;

    #[test]
    fn mapping() {
        for (string, tok) in UNICODE_TOKENS {
            match tok {
                Token::OpenBracket => {
                    super::left_bracket_str(string);
                }
                Token::CloseBracket => {
                    super::right_bracket_str(string);
                }
                Token::OpenCloseBracket => {
                    super::left_bracket_str(string);
                    super::right_bracket_str(string);
                    super::symbol_str(string, SkinTone::Default);
                }
                Token::Symbol | Token::Frac | Token::Super | Token::Sub | Token::Sep => {
                    super::symbol_str(string, SkinTone::Default);
                }
                _ => {}
            }
        }
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
                    new_chars.insert(func(chr));
                }
            }
            chars = new_chars;
        }
    }

    #[test]
    fn superscript_mappings() {
        for c in 'a'..='z' {
            if c != 'q' {
                assert!(
                    super::superscript_char(c).is_some(),
                    "missing superscript for '{c}'"
                );
            }
        }
        for c in '0'..='9' {
            assert!(super::superscript_char(c).is_some());
        }
        for c in ['+', '-', '=', '(', ')'] {
            assert!(super::superscript_char(c).is_some());
        }
        assert!(super::superscript_char(' ').is_some());
        assert!(super::superscript_char('!').is_none());
    }

    #[test]
    fn subscript_mappings() {
        for c in [
            'a', 'e', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'r', 's', 't', 'u', 'v', 'x',
        ] {
            assert!(
                super::subscript_char(c).is_some(),
                "missing subscript for '{c}'"
            );
        }
        for c in '0'..='9' {
            assert!(super::subscript_char(c).is_some());
        }
        for c in ['+', '-', '=', '(', ')'] {
            assert!(super::subscript_char(c).is_some());
        }
        assert!(super::subscript_char(' ').is_some());
        assert!(super::subscript_char('z').is_none());
    }
}
