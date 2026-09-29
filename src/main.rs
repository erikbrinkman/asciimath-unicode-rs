use asciimath_unicode::{Conf, Layout, SkinTone};
use clap::{Parser, ValueEnum};
use std::io;
use std::io::{Read, Write};
use std::process;

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

#[derive(Debug, Clone, ValueEnum)]
enum LayoutArg {
    /// One line, fractions as super- and subscripts
    InlineScript,
    /// One line, fractions with a slash
    InlinePlain,
    /// Several lines, with stacked fractions and grids
    Block,
}

impl From<LayoutArg> for Layout {
    fn from(inp: LayoutArg) -> Self {
        match inp {
            LayoutArg::InlineScript => Layout::InlineScript,
            LayoutArg::InlinePlain => Layout::InlinePlain,
            LayoutArg::Block => Layout::Block,
        }
    }
}

/// Convert asciimath in stdin to unicode in stdout
#[derive(Debug, Clone, Parser)]
#[command(version, about)]
struct Args {
    /// Keep ( ), [ ] and { } around fractions, scripts and command arguments
    #[arg(long)]
    no_strip_brackets: bool,

    /// Don't render fractions as vulgar fractions
    #[arg(long)]
    no_vulgar_fracs: bool,

    /// Skin tone for emoji
    #[arg(long, value_enum, default_value_t = Tone::Default)]
    skin_tone: Tone,

    /// How to lay out the math
    #[arg(long, value_enum, default_value_t = LayoutArg::InlineScript)]
    layout: LayoutArg,
}

impl From<Args> for Conf {
    fn from(inp: Args) -> Self {
        Conf::default()
            .with_strip_brackets(!inp.no_strip_brackets)
            .with_vulgar_fracs(!inp.no_vulgar_fracs)
            .with_skin_tone(inp.skin_tone.into())
            .with_layout(inp.layout.into())
    }
}

fn main() {
    let conf: Conf = Args::parse().into();
    let mut inp = String::new();
    if let Err(err) = io::stdin().lock().read_to_string(&mut inp) {
        eprintln!("{err}");
        process::exit(1);
    }
    let mut out = io::stdout().lock();
    // a closed pipe (e.g. `| head`) is normal, not an error
    if let Err(err) = write!(out, "{}", conf.parse(&inp))
        .and_then(|()| writeln!(out))
        .and_then(|()| out.flush())
        && err.kind() != io::ErrorKind::BrokenPipe
    {
        eprintln!("{err}");
        process::exit(1);
    }
}
