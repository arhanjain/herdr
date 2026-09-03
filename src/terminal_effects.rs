use base64::Engine;
use std::io::{self, Write};

const BELL_CHUNK: [u8; 64] = [b'\x07'; 64];

/// Host terminal user variable set while an interactive client owns the terminal.
///
/// Lets the outer terminal's own config react to Herdr having the keyboard, e.g.
/// kitty's `map --when-focus-on var:IS_HERDR ctrl+h` to stop claiming a key that
/// Herdr binds itself.
pub(crate) const HOST_USER_VAR_ATTACHED: &str = "IS_HERDR";

pub(crate) fn write_terminal_bells<W: Write>(writer: &mut W, count: u16) -> io::Result<()> {
    let full_chunks = usize::from(count) / BELL_CHUNK.len();
    let remainder = usize::from(count) % BELL_CHUNK.len();
    for _ in 0..full_chunks {
        writer.write_all(&BELL_CHUNK)?;
    }
    writer.write_all(&BELL_CHUNK[..remainder])?;
    writer.flush()
}

pub(crate) fn write_window_title<W: Write>(writer: &mut W, title: Option<&str>) -> io::Result<()> {
    let title = title.unwrap_or("herdr");
    let safe_title = strip_osc_terminators(title);
    write!(writer, "\x1b]0;{safe_title}\x07")?;
    writer.flush()
}

/// Set or delete a terminal user variable (OSC 1337 `SetUserVar`).
///
/// `None` deletes it: terminals treat a payload with no `=value` as unset.
pub(crate) fn write_host_user_var<W: Write>(
    writer: &mut W,
    name: &str,
    value: Option<&str>,
) -> io::Result<()> {
    let safe_name = strip_osc_terminators(name);
    match value {
        Some(value) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(value);
            write!(writer, "\x1b]1337;SetUserVar={safe_name}={encoded}\x07")?;
        }
        None => write!(writer, "\x1b]1337;SetUserVar={safe_name}\x07")?,
    }
    writer.flush()
}

/// Whether the host terminal understands OSC 1337 `SetUserVar`.
///
/// Pure over the env values so it stays testable without touching the process
/// environment, matching `direct_graphics_profile_values` in `crate::client`.
pub(crate) fn host_supports_user_vars(term_program: &str, term: &str, kitty_window: bool) -> bool {
    term_program.eq_ignore_ascii_case("ghostty")
        || term_program.eq_ignore_ascii_case("wezterm")
        || matches!(term, "xterm-ghostty" | "xterm-kitty" | "xterm-wezterm")
        || kitty_window
}

/// `host_supports_user_vars` against the current environment.
pub(crate) fn host_supports_user_vars_from_env() -> bool {
    host_supports_user_vars(
        &std::env::var("TERM_PROGRAM").unwrap_or_default(),
        &std::env::var("TERM").unwrap_or_default(),
        std::env::var_os("KITTY_WINDOW_ID").is_some(),
    )
}

/// Drop bytes that would terminate the OSC string early.
fn strip_osc_terminators(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(*ch, '\u{1b}' | '\u{7}' | '\u{9c}'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_exact_terminal_bell_count() {
        let mut output = Vec::new();

        write_terminal_bells(&mut output, 130).unwrap();

        assert_eq!(output, vec![b'\x07'; 130]);
    }

    #[test]
    fn window_title_strips_terminators_and_defaults_to_herdr() {
        let mut output = Vec::new();
        write_window_title(&mut output, Some("herdr\x1b api\u{7}\u{9c}")).unwrap();
        assert_eq!(output, b"\x1b]0;herdr api\x07");

        output.clear();
        write_window_title(&mut output, None).unwrap();
        assert_eq!(output, b"\x1b]0;herdr\x07");
    }

    #[test]
    fn host_user_var_encodes_value_and_omits_it_to_delete() {
        let mut output = Vec::new();
        write_host_user_var(&mut output, HOST_USER_VAR_ATTACHED, Some("true")).unwrap();
        assert_eq!(output, b"\x1b]1337;SetUserVar=IS_HERDR=dHJ1ZQ==\x07");

        output.clear();
        write_host_user_var(&mut output, HOST_USER_VAR_ATTACHED, None).unwrap();
        assert_eq!(output, b"\x1b]1337;SetUserVar=IS_HERDR\x07");
    }

    #[test]
    fn host_user_var_strips_terminators_from_name() {
        let mut output = Vec::new();
        write_host_user_var(&mut output, "IS\x1b_HERDR\u{7}\u{9c}", None).unwrap();
        assert_eq!(output, b"\x1b]1337;SetUserVar=IS_HERDR\x07");
    }

    #[test]
    fn host_user_var_support_follows_osc_1337_terminal_family() {
        assert!(host_supports_user_vars("", "xterm-kitty", false));
        assert!(host_supports_user_vars("Ghostty", "xterm-256color", false));
        assert!(host_supports_user_vars("WezTerm", "xterm-256color", false));
        assert!(host_supports_user_vars("", "screen", true));
        assert!(!host_supports_user_vars(
            "Apple_Terminal",
            "xterm-256color",
            false
        ));
        assert!(!host_supports_user_vars("", "xterm-256color", false));
    }
}
