use asciimath_unicode::{Conf, SkinTone};
use clap::{Parser, ValueEnum};
use std::io;
use std::io::{Read, Write};

#[derive(Debug, Clone, ValueEnum)]
enum Tone {
    Default,
    Light,
    MediumLight,
    Medium,
    MediumDark,
    Dark,
}

impl From<Tone> for SkinTone {
    fn from(inp: Tone) -> Self {
        match inp {
            Tone::Default => SkinTone::Default,
            Tone::Light => SkinTone::Light,
            Tone::MediumLight => SkinTone::MediumLight,
            Tone::Medium => SkinTone::Medium,
            Tone::MediumDark => SkinTone::MediumDark,
            Tone::Dark => SkinTone::Dark,
        }
    }
}

/// Convert asciimath in stdin to unicode in stdout
#[derive(Debug, Clone, Parser)]
#[command(version, about)]
struct Args {
    /// Don't strip unnecessary parenthesis in some contexts
    #[arg(long)]
    no_strip_brackets: bool,

    /// Don't render fractions as vulgar fractions
    #[arg(long)]
    no_vulgar_fracs: bool,

    /// Don't render fractions using super- and sub-scripts
    #[arg(long)]
    no_script_fracs: bool,

    /// Skin tone for emoji
    #[arg(long, value_enum, default_value_t = Tone::Default)]
    skin_tone: Tone,

    /// Render as multi-line 2D block (stacked fractions, vertical scripts, matrix grids)
    #[arg(long)]
    block: bool,
}

impl From<Args> for Conf {
    fn from(inp: Args) -> Self {
        Conf {
            strip_brackets: !inp.no_strip_brackets,
            vulgar_fracs: !inp.no_vulgar_fracs,
            script_fracs: !inp.no_script_fracs,
            skin_tone: inp.skin_tone.into(),
            block: inp.block,
        }
    }
}

fn main() {
    let conf: Conf = Args::parse().into();
    let mut inp = String::new();
    io::stdin().lock().read_to_string(&mut inp).unwrap();
    let mut out = io::stdout().lock();
    write!(out, "{}", conf.parse(&inp)).unwrap();
    writeln!(out).unwrap();
}
