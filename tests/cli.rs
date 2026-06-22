//! Integration tests for the `asciimath-unicode` binary.
#![cfg(feature = "binary")]

use std::io::Write;
use std::process::{Command, Stdio};

/// Run the binary with `args`, feeding `input` on stdin, and return its stdout.
fn run(args: &[&str], input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_asciimath-unicode"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn binary");
    child
        .stdin
        .take()
        .expect("stdin not piped")
        .write_all(input.as_bytes())
        .expect("failed to write stdin");
    let output = child.wait_with_output().expect("failed to wait on binary");
    assert!(output.status.success(), "binary exited with failure");
    String::from_utf8(output.stdout).expect("stdout was not utf-8")
}

#[test]
fn converts_with_defaults() {
    assert_eq!(run(&[], "1/2"), "½\n");
}

#[test]
fn no_vulgar_fracs() {
    assert_eq!(run(&["--no-vulgar-fracs"], "1/2"), "¹⁄₂\n");
}

#[test]
fn no_script_fracs() {
    assert_eq!(
        run(&["--no-vulgar-fracs", "--no-script-fracs"], "1/2"),
        "1/2\n"
    );
}

#[test]
fn block_no_script_fracs_stacks() {
    assert_eq!(
        run(
            &["--block", "--no-vulgar-fracs", "--no-script-fracs"],
            "1/2"
        ),
        "1\n─\n2\n"
    );
}

#[test]
fn no_strip_brackets() {
    assert_eq!(run(&["--no-strip-brackets"], "sqrt(x)"), "√(x)\n");
}

#[test]
fn block_mode() {
    assert_eq!(run(&["--block"], "x/y"), "x\n─\ny\n");
}

#[test]
fn skin_tone() {
    // every `--skin-tone` variant maps onto a distinct emoji rendering
    assert_eq!(run(&[], ":hand:"), "✋\n");
    assert_eq!(run(&["--skin-tone", "default"], ":hand:"), "✋\n");
    assert_eq!(run(&["--skin-tone", "light"], ":hand:"), "✋🏻\n");
    assert_eq!(run(&["--skin-tone", "medium-light"], ":hand:"), "✋🏼\n");
    assert_eq!(run(&["--skin-tone", "medium"], ":hand:"), "✋🏽\n");
    assert_eq!(run(&["--skin-tone", "medium-dark"], ":hand:"), "✋🏾\n");
    assert_eq!(run(&["--skin-tone", "dark"], ":hand:"), "✋🏿\n");
}
