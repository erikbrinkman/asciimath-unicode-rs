Asciimath Unicode
=================

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
