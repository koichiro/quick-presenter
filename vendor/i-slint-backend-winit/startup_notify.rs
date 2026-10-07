// SPDX-License-Identifier: GPL-3.0-only

use winit::window::ActivationToken;

pub(crate) fn read_and_clear_token(is_wayland: bool) -> Option<ActivationToken> {
    let token = select_token(
        is_wayland,
        std::env::var("XDG_ACTIVATION_TOKEN").ok(),
        std::env::var("DESKTOP_STARTUP_ID").ok(),
    );
    winit::platform::startup_notify::reset_activation_token_env();
    token
}

fn select_token(
    is_wayland: bool,
    wayland_token: Option<String>,
    x11_token: Option<String>,
) -> Option<ActivationToken> {
    let token = if is_wayland { wayland_token } else { x11_token };
    token
        .filter(|value| !value.is_empty())
        .map(ActivationToken::from_raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_token_for_the_connected_backend() {
        for (is_wayland, expected) in [(true, "wayland"), (false, "x11")] {
            let token = select_token(is_wayland, Some("wayland".into()), Some("x11".into()));
            assert_eq!(token.unwrap().into_raw(), expected);
        }
    }

    #[test]
    fn does_not_use_a_token_from_the_other_backend() {
        assert!(select_token(true, None, Some("x11".into())).is_none());
        assert!(select_token(false, Some("wayland".into()), None).is_none());
    }

    #[test]
    fn ignores_empty_tokens() {
        assert!(select_token(true, Some(String::new()), None).is_none());
        assert!(select_token(false, None, Some(String::new())).is_none());
    }

    #[test]
    fn clears_launcher_environment_before_children_can_inherit_it() {
        const PROBE: &str = "SLINT_STARTUP_NOTIFY_TEST_BACKEND";
        if let Ok(backend) = std::env::var(PROBE) {
            let token = read_and_clear_token(backend == "wayland").unwrap();
            assert_eq!(token.into_raw(), backend);
            assert!(std::env::var_os("XDG_ACTIVATION_TOKEN").is_none());
            assert!(std::env::var_os("DESKTOP_STARTUP_ID").is_none());
            assert!(read_and_clear_token(backend == "wayland").is_none());
            return;
        }

        // Isolate environment mutation from other tests and backend initialization.
        for backend in ["wayland", "x11"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg(format!(
                    "{}::clears_launcher_environment_before_children_can_inherit_it",
                    module_path!().split_once("::").unwrap().1,
                ))
                .env(PROBE, backend)
                .env("XDG_ACTIVATION_TOKEN", "wayland")
                .env("DESKTOP_STARTUP_ID", "x11")
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
    }
}
