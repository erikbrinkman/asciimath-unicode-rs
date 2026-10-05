//! Integration tests for the `asciimath-unicode` binary.
#![cfg(feature = "binary")]

use std::io::{Read, Write};
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

/// Run the binary with `args` and no input, returning its exit code.
fn exit_code(args: &[&str]) -> Option<i32> {
    let output = Command::new(env!("CARGO_BIN_EXE_asciimath-unicode"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run binary");
    output.status.code()
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
fn inline_plain_layout() {
    assert_eq!(
        run(&["--no-vulgar-fracs", "--layout", "inline-plain"], "1/2"),
        "1/2\n"
    );
}

#[test]
fn block_no_vulgar_fracs_stacks() {
    assert_eq!(
        run(&["--layout", "block", "--no-vulgar-fracs"], "1/2"),
        "1\n─\n2\n"
    );
}

#[test]
fn no_strip_brackets() {
    assert_eq!(run(&["--no-strip-brackets"], "sqrt(x)"), "√(x)\n");
}

#[test]
fn keep_spaces() {
    assert_eq!(run(&[], "a + b"), "a+b\n");
    assert_eq!(run(&["--keep-spaces"], "a + b"), "a + b\n");
    assert_eq!(run(&["--keep-spaces"], "x / y   z"), "x/y   z\n");
    assert_eq!(run(&["--keep-spaces"], "a\tb"), "a\tb\n");
    assert_eq!(
        run(
            &["--keep-spaces", "--layout", "inline-plain"],
            "1 / 2  x ^ 2"
        ),
        "½  x²\n"
    );
    assert_eq!(
        run(&["--keep-spaces", "--no-strip-brackets"], "1/(- x)"),
        "¹⁄₍₋ₓ₎\n"
    );
    assert_eq!(
        run(&["--keep-spaces", "--placeholders"], "a  obrace() b"),
        "a  □ b\n"
    );
    assert_eq!(
        run(&["--keep-spaces", "--layout", "block"], "a\tb"),
        "a b\n"
    );
    assert_eq!(
        run(&["--keep-spaces", "--layout", "block"], "sqrt(a\nb)/c"),
        "√(a b)\n──────\n   c\n"
    );
    assert_eq!(
        run(&["--keep-spaces", "--layout", "block"], "[[a  b,c],[d,e]]"),
        "⎡ab  c⎤\n⎣ d  e⎦\n"
    );
}

#[test]
fn placeholders() {
    assert_eq!(run(&[], "x^"), "x\n");
    assert_eq!(run(&["--placeholders"], "x^"), "x⸋\n");
    assert_eq!(run(&["--placeholders"], "1/"), "1/□\n");
    assert_eq!(
        run(&["--placeholders", "--placeholder", "?"], "sqrt"),
        "√?\n"
    );
    assert_eq!(
        run(&["--placeholders", "--placeholder-sub", "."], "x_"),
        "x.\n"
    );
    assert_eq!(
        run(&["--placeholders", "--placeholder-sup", "!"], "x^"),
        "x!\n"
    );
    assert_eq!(run(&["--placeholders"], ")"), ")\n");
    assert_eq!(
        run(&["--placeholders", "--no-strip-brackets"], "abs()"),
        "|(□)|\n"
    );
    assert_eq!(
        run(&["--placeholders", "--layout", "block"], "(: :)"),
        "⟨□⟩\n"
    );
}

#[test]
fn placeholder_marks_need_placeholders() {
    assert_eq!(exit_code(&["--placeholder", "?"]), Some(2));
    assert_eq!(exit_code(&["--placeholder-sub", "."]), Some(2));
    assert_eq!(exit_code(&["--placeholder-sup", "!"]), Some(2));
}

#[test]
fn block_mode() {
    assert_eq!(run(&["--layout", "block"], "x/y"), "x\n─\ny\n");
}

#[test]
fn closed_stdout_pipe_is_not_an_error() {
    // a reader that closes the pipe early must not make the binary fail
    let mut child = Command::new(env!("CARGO_BIN_EXE_asciimath-unicode"))
        .args(["--layout", "block"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn binary");
    // a large block expression produces multi-line output larger than the pipe buffer
    let input = "sum_(i=1)^n ".repeat(20_000);
    child
        .stdin
        .take()
        .expect("stdin not piped")
        .write_all(input.as_bytes())
        .expect("failed to write stdin");
    // read a single byte, then drop the read end to close the pipe
    let mut stdout = child.stdout.take().expect("stdout not piped");
    let mut one = [0u8; 1];
    let _ = stdout.read(&mut one);
    drop(stdout);
    let status = child.wait().expect("failed to wait on binary");
    assert!(status.success(), "broken pipe should exit successfully");
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

#[test]
fn non_utf8_input_fails_cleanly() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_asciimath-unicode"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn binary");
    child
        .stdin
        .take()
        .expect("stdin not piped")
        .write_all(&[0xff, 0xfe])
        .expect("failed to write stdin");
    let output = child.wait_with_output().expect("failed to wait on binary");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr was not utf-8");
    assert!(!stderr.is_empty());
    assert!(!stderr.contains("panicked"), "binary panicked: {stderr}");
}
