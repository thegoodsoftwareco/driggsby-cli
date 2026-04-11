use anyhow::Result;

pub async fn open_browser_url(url: &str) -> Result<bool> {
    if !browser_opening_is_plausible() {
        return Ok(false);
    }

    Ok(open::that_detached(url).is_ok())
}

fn browser_opening_is_plausible() -> bool {
    #[cfg(target_os = "linux")]
    {
        return linux_browser_session_is_configured(|key| std::env::var_os(key));
    }

    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

#[cfg(target_os = "linux")]
fn linux_browser_session_is_configured(
    mut get_var: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> bool {
    [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "WSL_DISTRO_NAME",
        "WSL_INTEROP",
    ]
    .iter()
    .any(|key| get_var(key).is_some_and(|value| !value.is_empty()))
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    mod linux {
        use std::ffi::OsString;

        use super::super::linux_browser_session_is_configured;

        fn lookup<'a>(
            values: &'a [(&'a str, &'a str)],
        ) -> impl FnMut(&str) -> Option<OsString> + 'a {
            |key| {
                values
                    .iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| OsString::from(value))
            }
        }

        #[test]
        fn browser_is_not_plausible_without_graphical_or_browser_environment() {
            assert!(!linux_browser_session_is_configured(lookup(&[])));
        }

        #[test]
        fn browser_is_plausible_with_display() {
            assert!(linux_browser_session_is_configured(lookup(&[(
                "DISPLAY", ":0"
            )])));
        }

        #[test]
        fn browser_is_plausible_with_wsl_bridge() {
            assert!(linux_browser_session_is_configured(lookup(&[(
                "WSL_INTEROP",
                "/run/WSL/1_interop",
            )])));
        }
    }
}
