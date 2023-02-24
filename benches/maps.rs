#![feature(test)]

extern crate test;

use lazy_static::lazy_static;
use rand::distributions::Alphanumeric;
use rand::Rng;
use std::collections::HashMap;
use std::hint;
use test::Bencher;

lazy_static! {
    static ref SUBS: HashMap<char, char> = HashMap::from_iter([
        ('a', 'ₐ'),
        ('e', 'ₑ'),
        ('h', 'ₕ'),
        ('i', 'ᵢ'),
        ('k', 'ₖ'),
        ('l', 'ₗ'),
        ('m', 'ₘ'),
        ('n', 'ₙ'),
        ('o', 'ₒ'),
        ('p', 'ₚ'),
        ('r', 'ᵣ'),
        ('s', 'ₛ'),
        ('t', 'ₜ'),
        ('u', 'ᵤ'),
        ('v', 'ᵥ'),
        ('x', 'ₓ'),
        ('0', '₀'),
        ('1', '₁'),
        ('2', '₂'),
        ('3', '₃'),
        ('4', '₄'),
        ('5', '₅'),
        ('6', '₆'),
        ('7', '₇'),
        ('8', '₈'),
        ('9', '₉'),
        ('+', '₊'),
        ('-', '₋'),
        ('=', '₌'),
        ('(', '₍'),
        (')', '₎'),
        ('β', 'ᵦ'),
        ('γ', 'ᵧ'),
        ('ρ', 'ᵨ'),
        ('φ', 'ᵩ'),
        ('ϕ', 'ᵩ'),
        ('χ', 'ᵪ'),
    ]);
}

fn convert_hash(inp: char) -> Option<char> {
    SUBS.get(&inp).copied()
}

fn convert_match(inp: char) -> Option<char> {
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
        'ϕ' => Some('ᵩ'),
        'χ' => Some('ᵪ'),
        _ => None,
    }
}

lazy_static! {
    static ref RANDOM: String = {
        let mut rng = rand::thread_rng();
        String::from_utf8((0..1000).map(move |_| rng.sample(Alphanumeric)).collect()).unwrap()
    };
}

#[bench]
fn hasher(bench: &mut Bencher) {
    let string: &str = &RANDOM;
    bench.iter(|| {
        for chr in string.chars() {
            hint::black_box(convert_hash(chr));
        }
    });
}

#[bench]
fn matcher(bench: &mut Bencher) {
    let string: &str = &RANDOM;
    bench.iter(|| {
        for chr in string.chars() {
            hint::black_box(convert_match(chr));
        }
    });
}
