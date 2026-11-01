Asciimath Unicode
=================

[![crates.io](https://img.shields.io/crates/v/asciimath-unicode)](https://crates.io/crates/asciimath-unicode)
[![docs](https://docs.rs/asciimath-unicode/badge.svg)](https://docs.rs/asciimath-unicode)
[![license](https://img.shields.io/github/license/erikbrinkman/asciimath-unicode-rs)](https://github.com/erikbrinkman/asciimath-unicode-rs/blob/main/LICENSE)
[![tests](https://github.com/erikbrinkman/asciimath-unicode-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/erikbrinkman/asciimath-unicode-rs/actions/workflows/rust.yml)

Render asciimath to unicode.

To convert asciimath quickly, you can use [`parse_unicode`] to get an
[`Asciimath`] value that implements [`Display`].  If you want more control, see
the options exposed through [`Conf`] which can [`parse`] input into
[`Asciimath`] as well.

All of the input is read as math, so prose run through this comes out mangled:
`it is` renders as `𝑖s`.  Pick the math out of prose first.

# Usage

## Binary

This crate provides a simple cli for converting asciimath to unicode:

```bash
cargo install asciimath-unicode --features binary
```

```bash
asciimath-unicode -h
```

## Library

```bash
cargo add asciimath-unicode
```

```rust
let res = asciimath_unicode::parse_unicode("1/2").to_string();
assert_eq!(res, "½");
```

```rust
use asciimath_unicode::Conf;
let conf = Conf::default().with_vulgar_fracs(false);
let res = conf.parse("1/2").to_string();
assert_eq!(res, "¹⁄₂");
```

```rust
use asciimath_unicode::{Conf, Layout};
let conf = Conf::default().with_layout(Layout::Block);
let res = conf.parse("x/y").to_string();
assert_eq!(res, "x\n─\ny");
```

Every spelling in [`ASCIIMATH_TOKENS`] is accepted, along with the Unicode forms listed there (all
in [`asciimath_spellings`]), plus any emoji shortcode `:name:` from the
[`emojis`](https://docs.rs/emojis) crate:

```rust
use asciimath_unicode::ASCIIMATH_TOKENS;
let alpha = ASCIIMATH_TOKENS.iter().find(|entry| entry.spelling == "alpha");
assert_eq!(alpha.unwrap().unicode, Some("α"));
```

## Configuration

`Conf` starts from `default()`; set fields directly or with the matching
`with_*` method.

| Field              | Type                   | Default        | Description                                                           |
|--------------------|------------------------|----------------|-----------------------------------------------------------------------|
| `strip_brackets`   |                 `bool` |         `true` | Drop ( ), [ ] and { } around fractions, scripts and command arguments |
| `vulgar_fracs`     |                 `bool` |         `true` | Render fractions as vulgar fractions (e.g. ½)                         |
| `skin_tone`        |             `SkinTone` |      `Default` | Default skin tone for emojis                                          |
| `layout`           |               `Layout` | `InlineScript` | How to lay out the math: `InlineScript`, `InlinePlain`, or `Block`    |
| `placeholders`     | `Option<Placeholders>` |         `None` | What stands in for the parts that aren't there yet                    |
| `keep_spaces`      |                 `bool` |        `false` | Write the whitespace typed between parts of the math back out         |
| `spaced_operators` |                 `bool` |        `false` | Put a space on either side of operators like `+` and `=`              |

`Placeholders` holds the marks themselves: `char` for an argument, fraction part, bracket pair or
matrix cell with nothing in it, `sub` for a subscript and `sup` for a superscript, defaulting to
`□`, `▫` and `⸋`.

`placeholders` is for showing math while it is still being typed, so that `x^` or `sqrt` doesn't
look the same as `x` or nothing.  Brackets or a matrix cell with nothing in them count as a part
being typed too, while a function name like `sin` or `f` reads fine alone and gets no placeholder.
Whether the mark replaces a bracket pair with nothing in it or sits between the brackets follows
`strip_brackets`.  A script that can't be raised or lowered is written after a literal `^` or `_`,
and a script that isn't there yet then takes `char` rather than `sup` or `sub`, so `x_y^` gives
`x_y^□` because `y` has no subscript form:

```rust
use asciimath_unicode::{Conf, Placeholders};
let conf = Conf::default().with_placeholders(Some(Placeholders::default()));
assert_eq!(conf.parse("sqrt").to_string(), "√□");
assert_eq!(conf.parse("x^").to_string(), "x⸋");
assert_eq!(conf.parse("abs()").to_string(), "|□|");
assert_eq!(conf.with_strip_brackets(false).parse("abs()").to_string(), "|(□)|");
assert_eq!(conf.parse("x_y^").to_string(), "x_y^□");
assert_eq!(conf.parse("x_x^").to_string(), "xₓ⸋");

let marks = Placeholders::default().with_char('?').with_sup('!');
let conf = conf.with_placeholders(Some(marks));
assert_eq!(conf.parse("sqrt").to_string(), "√?");
assert_eq!(conf.parse("x^").to_string(), "x!");
```

`keep_spaces` keeps only the whitespace between two neighboring parts.  Whitespace within one part,
like around the `/` of a fraction or before a function's argument, is still dropped, and where none
was typed the usual spacing still applies, so `sinx` renders `sin x` either way.  A `+` or `-` that
applies to the part after it rather than joining two parts is written against that part, so
whitespace typed after it is dropped too.  A one-line layout writes a kept run as typed, tabs and
newlines included, while `Block` writes it as plain spaces as wide as the run printed, since a tab
or newline would throw off the lines it stacks, and drops what was typed inside a grid.  A space
that lands in a script is written one step narrower, so a script with a space in it doesn't read as
finished math:

```rust
use asciimath_unicode::{Conf, Layout};
let conf = Conf::default().with_keep_spaces(true);
assert_eq!(conf.parse("a + b").to_string(), "a + b");
assert_eq!(conf.parse("a+b").to_string(), "a+b");
assert_eq!(conf.parse("1 / 2  x ^ 2").to_string(), "½  x²");
assert_eq!(conf.parse("1 - - x").to_string(), "1 - -x");
assert_eq!(conf.parse("a\tb").to_string(), "a\tb");
assert_eq!(conf.parse("sinx").to_string(), "sin x");
assert_eq!(conf.parse("x^(a b)").to_string(), "xᵃ\u{2009}ᵇ");

let block = conf.with_layout(Layout::Block);
assert_eq!(block.parse("a\tb").to_string(), "a b");
assert_eq!(block.parse("[[a  b, c], [d, e]]").to_string(), "⎡ab  c⎤\n⎣ d  e⎦");
assert_eq!(conf.parse("[[a  b, c], [d, e]]").to_string(), "[[a  b,c],[d,e]]");
```

`spaced_operators` puts a space on either side of an operator that joins two parts of the math,
like `+`, `=`, `xx` or `in`; with it off they are written against their operands.  It says nothing
about the spaces that keep words and function names legible, as in `sin x` or `a and b`, which are
written either way, and a sign on the part after it is still written against that part.
Whitespace typed with `keep_spaces` on wins where it was typed, and where none was typed this
decides.  Such a space would step one width narrower in a script, and a fixed-width font draws
it no narrower than a full cell, so these spaces are left out inside a raised or lowered script
rather than show it as padded as full-size math.  The spaces a script gets for any other reason
are written there either way:

```rust
use asciimath_unicode::{Conf, Layout};
let conf = Conf::default().with_spaced_operators(true);
assert_eq!(conf.parse("a+b=c").to_string(), "a + b = c");
assert_eq!(conf.parse("1 - - x").to_string(), "1 - -x");
assert_eq!(conf.with_spaced_operators(false).parse("sinx+1").to_string(), "sin x+1");
assert_eq!(conf.parse("x^(a+b)").to_string(), "xᵃ⁺ᵇ");
assert_eq!(conf.parse("x^sin x").to_string(), "xˢⁱⁿ\u{2009}ˣ");
let block = conf.with_layout(Layout::Block);
assert_eq!(block.parse("x^(a+b)").to_string(), "xᵃ⁺ᵇ");

let kept = conf.with_keep_spaces(true);
assert_eq!(kept.parse("a  +b").to_string(), "a  + b");
```

[`parse_unicode`]: https://docs.rs/asciimath-unicode/latest/asciimath_unicode/fn.parse_unicode.html
[`Asciimath`]: https://docs.rs/asciimath-unicode/latest/asciimath_unicode/struct.Asciimath.html
[`Display`]: https://doc.rust-lang.org/std/fmt/trait.Display.html
[`Conf`]: https://docs.rs/asciimath-unicode/latest/asciimath_unicode/struct.Conf.html
[`parse`]: https://docs.rs/asciimath-unicode/latest/asciimath_unicode/struct.Conf.html#method.parse
[`ASCIIMATH_TOKENS`]: https://docs.rs/asciimath-unicode/latest/asciimath_unicode/static.ASCIIMATH_TOKENS.html
[`asciimath_spellings`]: https://docs.rs/asciimath-unicode/latest/asciimath_unicode/fn.asciimath_spellings.html
