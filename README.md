Asciimath Unicode
=================

[![crates.io](https://img.shields.io/crates/v/asciimath-unicode)](https://crates.io/crates/asciimath-unicode)
[![docs](https://docs.rs/asciimath-unicode/badge.svg)](https://docs.rs/asciimath-unicode)
[![license](https://img.shields.io/github/license/erikbrinkman/asciimath-unicode-rs)](LICENSE)
[![tests](https://github.com/erikbrinkman/asciimath-unicode-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/erikbrinkman/asciimath-unicode-rs/actions/workflows/rust.yml)

Render asciimath to unicode.

To convert asciimath quickly, you can use `parse_unicode` to get an `Asciimath`
value that implements `Display`.  If you want more control, see the options
exposed through `Conf` which can `parse` input into `Asciimath` as well.

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

Whether the mark replaces a bracket pair with nothing in it or sits between the brackets follows
`strip_brackets`: `abs()` renders `|□|` by default and `|(□)|` with the brackets kept.

`keep_spaces` keeps only the whitespace between two neighboring parts, and where none was typed the
usual spacing still applies, so `sinx` renders `sin x` either way.  A `+` or `-` that applies to the
part after it rather than joining two parts is written against that part, so `1 - - x` renders
`1 - -x`.  A one-line layout writes a kept run as typed, tabs and newlines included, while `Block`
writes it as plain spaces as wide as the run printed, since a tab or newline would throw off the
lines it stacks, and drops what was typed inside a grid.  A space that lands in a script is written
one step narrower, so `x^(a b)` renders `xᵃ ᵇ` with a thin space rather than a full one.

`spaced_operators` puts a space on either side of an operator that joins two parts of the math,
like `+`, `=`, `xx` or `in`, so `a+b=c` renders `a + b = c`.  It says nothing about the spaces that
keep words and function names legible, so `sinx` renders `sin x` either way, and a sign on the part
after it is still written against that part.  Whitespace typed with `keep_spaces` on wins where it
was typed, and where none was typed this decides, so `a  +b` renders `a  + b`.  Such a space steps
one width narrower when it lands in a script, and a fixed-width font draws it no narrower than a
full cell, so `Block` leaves these spaces out inside a raised or lowered script: `x^(a+b)` renders
`xᵃ⁺ᵇ` there and `xᵃ ⁺ ᵇ` in a one-line layout.  The spaces a script gets for any other reason are written there
either way.
