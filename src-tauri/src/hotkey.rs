//! What a dictation hotkey event does, for hold-to-talk and press-to-toggle.

// shortcut: a recordable combo, not Fn or Right Option alone: those cannot be registered as global
// shortcuts (they need an event tap and Input Monitoring). Revisit with the Swift bridge if people ask.
pub const DEFAULT: &str = "Alt+Space";

#[derive(Debug, PartialEq)]
pub enum Act {
    Start,
    Stop,
    Nothing,
}

/// Hold: press starts, release stops. Toggle: each press flips between the two, releases do nothing.
pub fn act(toggle: bool, pressed: bool, recording: bool) -> Act {
    match (toggle, pressed, recording) {
        (false, true, _) | (true, true, false) => Act::Start,
        (false, false, _) | (true, true, true) => Act::Stop,
        (true, false, _) => Act::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Act::*;

    #[test]
    fn hold_follows_the_key_and_toggle_follows_the_presses() {
        // (toggle, pressed, recording) -> action
        let table = [
            (false, true, false, Start),
            (false, false, true, Stop),
            (false, false, false, Stop), // release with nothing recording is harmless downstream
            (true, true, false, Start),
            (true, true, true, Stop),
            (true, false, true, Nothing),
            (true, false, false, Nothing),
        ];
        for (toggle, pressed, recording, want) in table {
            assert_eq!(act(toggle, pressed, recording), want, "toggle={toggle} pressed={pressed} recording={recording}");
        }
    }

    #[test]
    fn what_the_settings_recorder_produces_registers() {
        // The UI builds these from KeyboardEvent.code; the plugin must understand every shape.
        for s in [DEFAULT, "Control+Shift+KeyD", "Super+Digit1", "Control+Alt+Super+ArrowLeft", "F5", "Shift+Backquote", "Alt+Comma"] {
            s.parse::<tauri_plugin_global_shortcut::Shortcut>().unwrap_or_else(|e| panic!("{s}: {e}"));
        }
    }
}
