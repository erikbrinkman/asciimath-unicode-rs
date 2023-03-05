Asciimath Unicode
=================
[![crates.io](https://img.shields.io/crates/v/asciimath-unicode)](https://crates.io/crates/asciimath-unicode)
[![docs](https://docs.rs/asciimath-unicode/badge.svg)](https://docs.rs/asciimath-unicode)
[![license](https://img.shields.io/github/license/erikbrinkman/asciimath-unicode-rs)](LICENSE)
[![tests](https://github.com/erikbrinkman/asciimath-unicode-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/erikbrinkman/asciimath-unicode-rs/actions/workflows/rust.yml)

Render asciimath to unicode.

To convert asciimath quickly, you can use the `write_unicode` or
`convert_unicode` methods.  If you want more control, see the options exposed
through `InlineRenderer` which can be rendered into `RenderedUnicode`.

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
cargo add asciimath-parser
```

```rs
let res = asciimath_unicode::convert_unicode("1/2");
assert_eq!(res, "½");
```

```rs
use asciimath_unicode::InlineRenderer;
let renderer = InlineRenderer {
    vulgar_fracs: false,
    ..Default::default()
};
let res: String = renderer.render("1/2").collect();
assert_eq!(res, "¹⁄₂");
```
