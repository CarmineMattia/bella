//! Copy message text as data, never as shell code. Native helpers are bounded;
//! OSC 52 is a best-effort fallback whose receipt must not claim confirmed copying.
use crossterm::{Command as _, clipboard::CopyToClipboard};
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub enum Outcome {
    Copied,
    TerminalRequest(String),
}
fn installed(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|p| p.join(name).is_file()))
}
fn send(program: &std::ffi::OsStr, args: &[&str], text: &str) -> std::io::Result<bool> {
    let mut child = Command::new("timeout")
        .args(["--signal=TERM", "--kill-after=1s", "2s"])
        .arg(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let written = child
        .stdin
        .take()
        .expect("piped input")
        .write_all(text.as_bytes());
    let status = child.wait()?;
    Ok(written.is_ok() && status.success())
}
fn terminal_sequence(text: &str) -> Result<String, String> {
    if text.len() > 65536 {
        return Err("Message is too large for terminal clipboard fallback. Install wl-copy, xclip or xsel for native copying.".into());
    }
    let mut sequence = String::new();
    CopyToClipboard::to_clipboard_from(text)
        .write_ansi(&mut sequence)
        .map_err(|_| "Could not encode clipboard text".to_owned())?;
    Ok(sequence)
}
pub fn copy(text: String) -> Result<Outcome, String> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    let x11 = std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty());
    let choices: &[(&str, &[&str], bool)] = &[
        ("wl-copy", &["--type", "text/plain;charset=utf-8"], wayland),
        ("xclip", &["-selection", "clipboard"], x11),
        ("xsel", &["--clipboard", "--input"], x11),
    ];
    if installed("timeout") {
        for (program, args, available) in choices {
            if *available
                && installed(program)
                && send(std::ffi::OsStr::new(program), args, &text).unwrap_or(false)
            {
                return Ok(Outcome::Copied);
            }
        }
    }
    terminal_sequence(&text).map(Outcome::TerminalRequest)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_encodes_text_without_exposing_terminal_controls() {
        let text = "a\n界\x1b]52;c;evil\x07";
        let sequence = terminal_sequence(text).unwrap();
        assert!(sequence.starts_with("\x1b]52;c;"));
        assert!(sequence.ends_with("\x1b\\"));
        assert!(!sequence.contains("evil"));
        assert_eq!(
            terminal_sequence("hello").unwrap(),
            "\x1b]52;c;aGVsbG8=\x1b\\"
        );
        assert!(terminal_sequence(&"x".repeat(65537)).is_err());
    }
    #[test]
    fn native_helper_receives_exact_unicode_multiline_and_shell_characters() -> std::io::Result<()>
    {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("bella-clipboard-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let helper = root.join("helper");
        std::fs::write(&helper, "#!/bin/sh\ncat > \"$1\"\n")?;
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700))?;
        let destination = root.join("copied");
        let text = "hello 界\nsecond line ' ; $(touch do-not-execute) `false`";
        assert!(send(
            helper.as_os_str(),
            &[destination.to_str().unwrap()],
            text
        )?);
        assert_eq!(std::fs::read_to_string(destination)?, text);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
