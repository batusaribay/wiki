use std::env;
use std::io::IsTerminal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    enabled: bool,
}

#[derive(Clone, Copy)]
pub enum Role {
    Heading,
}

impl Theme {
    pub fn from_env(mode: ColorMode) -> Self {
        let clicolor = env::var("CLICOLOR").ok();
        let force = env::var("CLICOLOR_FORCE").ok();
        let term = env::var("TERM").ok();
        Self {
            enabled: color_enabled(
                mode,
                env::var_os("NO_COLOR").is_some(),
                clicolor.as_deref(),
                force.as_deref(),
                term.as_deref(),
                std::io::stdout().is_terminal(),
            ),
        }
    }

    pub fn paint(self, role: Role, text: &str) -> String {
        if !self.enabled {
            return text.to_string();
        }
        let code = match role {
            Role::Heading => "1;36",
        };
        format!("\x1b[{code}m{text}\x1b[0m")
    }
}

fn color_enabled(
    mode: ColorMode,
    no_color: bool,
    clicolor: Option<&str>,
    force: Option<&str>,
    term: Option<&str>,
    tty: bool,
) -> bool {
    match mode {
        ColorMode::Never => false,
        ColorMode::Always => true,
        ColorMode::Auto => {
            if no_color {
                return false;
            }
            if force.is_some_and(|value| !value.is_empty() && value != "0") {
                return true;
            }
            if clicolor == Some("0") || term == Some("dumb") {
                return false;
            }
            tty
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_modes_win() {
        assert!(!color_enabled(
            ColorMode::Never,
            false,
            None,
            None,
            None,
            true
        ));
        assert!(color_enabled(
            ColorMode::Always,
            true,
            Some("0"),
            None,
            Some("dumb"),
            false
        ));
    }

    #[test]
    fn auto_respects_the_environment() {
        assert!(color_enabled(
            ColorMode::Auto,
            false,
            None,
            None,
            Some("xterm"),
            true
        ));
        assert!(!color_enabled(
            ColorMode::Auto,
            false,
            None,
            None,
            Some("xterm"),
            false
        ));
        assert!(!color_enabled(
            ColorMode::Auto,
            true,
            None,
            None,
            Some("xterm"),
            true
        ));
        assert!(!color_enabled(
            ColorMode::Auto,
            false,
            Some("0"),
            None,
            Some("xterm"),
            true
        ));
        assert!(!color_enabled(
            ColorMode::Auto,
            false,
            None,
            None,
            Some("dumb"),
            true
        ));
        assert!(color_enabled(
            ColorMode::Auto,
            false,
            Some("0"),
            Some("1"),
            Some("dumb"),
            false
        ));
    }
}
