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

```rs
let res = asciimath_unicode::parse_unicode("1/2").to_string();
assert_eq!(res, "½");
```

```rs
use asciimath_unicode::Conf;
let conf = Conf {
    vulgar_fracs: false,
    ..Default::default()
};
let res = conf.parse("1/2").to_string();
assert_eq!(res, "¹⁄₂");
```
