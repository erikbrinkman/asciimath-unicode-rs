use asciimath_unicode::{Conf, Layout, Placeholders, SkinTone};
use clap::{Parser, ValueEnum};
use std::io;
use std::io::{Read, Write};
use std::process;

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

include!("skin_tones.rs");

macro_rules! skin_tone_args {
    ($($tone:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, ValueEnum)]
        enum SkinToneArg {
            $($tone,)+
        }

        impl From<SkinToneArg> for SkinTone {
            fn from(inp: SkinToneArg) -> Self {
                match inp {
                    $(SkinToneArg::$tone => SkinTone::$tone,)+
                }
            }
        }
    };
}

with_skin_tones!(skin_tone_args);

/// What `-h` says about `--skin-tone`, too short to list every tone
const SKIN_TONE_HELP: &str = "Skin tone for emoji: light, medium-light, medium, medium-dark or dark, \
                              or two different ones joined by -and- for emoji of two people, like \
                              light-and-dark";

/// What `--help` says about `--skin-tone`, with every tone
fn skin_tone_long_help() -> String {
    let names: Vec<String> = SkinToneArg::value_variants()
        .iter()
        .filter_map(ValueEnum::to_possible_value)
        .map(|value| value.get_name().to_owned())
        .collect();
    format!(
        "{SKIN_TONE_HELP}\n\n[possible values: {}]",
        names.join(", ")
    )
}

/// Convert asciimath in stdin to unicode in stdout. All of stdin is read as one expression, line
/// breaks included.
#[derive(Debug, Clone, Parser)]
#[command(version)]
#[allow(clippy::struct_excessive_bools)]
struct Args {
    /// Keep ( ), [ ] and { } around fractions, scripts and command arguments
    #[arg(long)]
    no_strip_brackets: bool,

    /// Don't render fractions as vulgar fractions
    #[arg(long)]
    no_vulgar_fracs: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = SkinToneArg::Default,
        hide_possible_values = true,
        help = SKIN_TONE_HELP,
        long_help = skin_tone_long_help()
    )]
    skin_tone: SkinToneArg,

    /// How to lay out the math
    #[arg(long, value_enum, default_value_t = LayoutArg::InlineScript)]
    layout: LayoutArg,

    /// Show a mark where a part of the math isn't there yet
    #[arg(long)]
    placeholders: bool,

    /// What stands in for a missing argument, fraction part, bracket pair or cell
    #[arg(long, value_name = "CHAR", requires = "placeholders", default_value_t = Placeholders::default().char)]
    placeholder: char,

    /// What stands in for a missing subscript
    #[arg(long, value_name = "CHAR", requires = "placeholders", default_value_t = Placeholders::default().sub)]
    placeholder_sub: char,

    /// What stands in for a missing superscript
    #[arg(long, value_name = "CHAR", requires = "placeholders", default_value_t = Placeholders::default().sup)]
    placeholder_sup: char,

    /// Write the whitespace typed between parts of the math back out
    #[arg(long)]
    keep_spaces: bool,

    /// Put spaces around operators like + and =
    #[arg(long)]
    spaced_operators: bool,
}

impl From<Args> for Conf {
    fn from(inp: Args) -> Self {
        Conf::default()
            .with_strip_brackets(!inp.no_strip_brackets)
            .with_vulgar_fracs(!inp.no_vulgar_fracs)
            .with_skin_tone(inp.skin_tone.into())
            .with_layout(inp.layout.into())
            .with_keep_spaces(inp.keep_spaces)
            .with_spaced_operators(inp.spaced_operators)
            .with_placeholders(inp.placeholders.then(|| {
                Placeholders::default()
                    .with_char(inp.placeholder)
                    .with_sub(inp.placeholder_sub)
                    .with_sup(inp.placeholder_sup)
            }))
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
