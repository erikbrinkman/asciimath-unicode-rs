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
let conf = Conf {
    vulgar_fracs: false,
    ..Default::default()
};
let res = conf.parse("1/2").to_string();
assert_eq!(res, "¹⁄₂");
```

```rust
use asciimath_unicode::Conf;
let conf = Conf {
    block: true,
    ..Default::default()
};
let res = conf.parse("x/y").to_string();
assert_eq!(res, "x\n─\ny");
```

## Configuration

| Field            |       Type |   Default | Description                                                                       |
|------------------|------------|-----------|-----------------------------------------------------------------------------------|
| `strip_brackets` |     `bool` |    `true` | Strip unnecessary parentheses in some contexts                                    |
| `vulgar_fracs`   |     `bool` |    `true` | Render fractions as vulgar fractions (e.g. ½)                                     |
| `script_fracs`   |     `bool` |    `true` | Render fractions using super/subscripts (e.g. ¹⁄₂)                                |
| `skin_tone`      | `SkinTone` | `Default` | Default skin tone for emojis                                                      |
| `block`          |     `bool` |   `false` | Multi-line 2D block rendering (stacked fractions, vertical scripts, matrix grids) |
