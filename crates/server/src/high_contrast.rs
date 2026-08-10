//! One-shot OS high-contrast (increased-contrast) accessibility signal read (CPE-1546, epic CPE-1496).
//!
//! [`is_high_contrast_active`] answers a single question — *is the OS's high-contrast / increased-contrast
//! accessibility mode currently ON?* — so the frontend's `ContrastSetting === "system"` (CPE-1544/1545)
//! can resolve to the real OS state instead of the hard-coded `false` default. It is a **one-shot query
//! read at boot**, not a live subscription: a push-based listener (`WM_SETTINGCHANGE` /
//! `NSNotificationCenter` / a D-Bus signal loop) is deliberately deferred (see the ticket) because its
//! live behaviour can only be confirmed by flipping the OS setting while the app runs — an attended
//! cross-OS check, not something this slice lands headless-verified.
//!
//! Per-platform, `#[cfg]`-gated:
//! - **Windows**: `SystemParametersInfoW(SPI_GETHIGHCONTRAST, …)` reads `HCF_HIGHCONTRASTON` from the
//!   returned `HIGHCONTRASTW` (via the `windows` crate, `Win32_UI_Accessibility` +
//!   `Win32_UI_WindowsAndMessaging`).
//! - **macOS**: `NSWorkspace.sharedWorkspace().accessibilityDisplayShouldIncreaseContrast()` (via
//!   `objc2-app-kit`, both getters are safe).
//! - **Linux**: a one-shot, timeout-bounded D-Bus read of the `org.freedesktop.appearance` `contrast`
//!   key over `xdg-desktop-portal`'s `org.freedesktop.portal.Settings` interface (via `zbus`, blocking).
//! - **Any other platform / any error / an unavailable portal**: `false`.
//!
//! **Fail-open contract:** every branch returns `false` on any error and never panics or hangs — the same
//! skip-on-error spirit as `list_dir`, and the reliable path regardless, since the manual `"high"`
//! contrast override (CPE-1544/1545) works no matter what this read returns.

/// Whether the OS's high-contrast / increased-contrast accessibility mode is currently ON.
///
/// A one-shot query — no live subscription (deferred). Fail-open: any error, unsupported platform, or
/// unavailable signal returns `false`; never panics or hangs (Linux is timeout-bounded).
pub fn is_high_contrast_active() -> bool {
    imp::is_high_contrast_active()
}

#[cfg(target_os = "windows")]
mod imp {
    use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };

    pub fn is_high_contrast_active() -> bool {
        // SPI_GETHIGHCONTRAST fills a caller-provided HIGHCONTRASTW in place; `cbSize` must be set to the
        // struct size first (the documented contract). We only READ, so the `fWinIni` update-flags arg is 0.
        let mut hc = HIGHCONTRASTW {
            cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
            ..Default::default()
        };
        // SAFETY: `hc` is a correctly-sized, zero-initialised HIGHCONTRASTW with `cbSize` set exactly as
        // SPI_GETHIGHCONTRAST requires; the pointer is valid for the duration of the call, which only
        // writes into it. No other invariants apply.
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                hc.cbSize,
                Some(std::ptr::from_mut(&mut hc).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };
        // Fail-open: a failed call reports "no high contrast".
        ok.is_ok() && (hc.dwFlags & HCF_HIGHCONTRASTON) == HCF_HIGHCONTRASTON
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2_app_kit::NSWorkspace;

    pub fn is_high_contrast_active() -> bool {
        // Both `sharedWorkspace` and `accessibilityDisplayShouldIncreaseContrast` are safe generated
        // getters in objc2-app-kit; the latter mirrors the OS "Increase contrast" accessibility toggle.
        NSWorkspace::sharedWorkspace().accessibilityDisplayShouldIncreaseContrast()
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::mpsc;
    use std::time::Duration;

    /// The XDG desktop portal `Settings.Read` call can, in principle, block (a stuck/absent portal), so we
    /// bound it: run the read on a throwaway thread and give up after a short timeout, returning `false`.
    /// A one-shot boot read leaking a hung helper thread is harmless (the process outlives it or exits).
    const PORTAL_READ_TIMEOUT: Duration = Duration::from_millis(400);

    pub fn is_high_contrast_active() -> bool {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(read_portal_contrast().unwrap_or(false));
        });
        rx.recv_timeout(PORTAL_READ_TIMEOUT).unwrap_or(false)
    }

    /// One-shot blocking D-Bus read of `org.freedesktop.appearance` / `contrast` over the portal's
    /// `org.freedesktop.portal.Settings` interface. `None` on any error (no bus, no portal, wrong shape).
    fn read_portal_contrast() -> Option<bool> {
        let conn = zbus::blocking::Connection::session().ok()?;
        let proxy = zbus::blocking::Proxy::new(
            &conn,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )
        .ok()?;
        // Settings.Read(namespace, key) -> v. The portal wraps the value in a variant (often doubly), so
        // deserialise into an OwnedValue and unwrap through any nesting in `contrast_from_value`.
        let reply: zbus::zvariant::OwnedValue = proxy
            .call("Read", &("org.freedesktop.appearance", "contrast"))
            .ok()?;
        contrast_from_value(&reply)
    }

    /// Map the portal `contrast` variant to a bool. `0` = no preference (off), `1` = high contrast; any
    /// non-zero integer is treated as "on". Recurses through nested variants (the portal double-wraps).
    /// Anything unexpected → `None` (caller fails open to `false`). Pure — unit-testable without a live bus.
    fn contrast_from_value(v: &zbus::zvariant::Value) -> Option<bool> {
        use zbus::zvariant::Value;
        match v {
            Value::U32(n) => Some(*n != 0),
            Value::I32(n) => Some(*n != 0),
            Value::U8(n) => Some(*n != 0),
            Value::Value(inner) => contrast_from_value(inner),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::contrast_from_value;
        use zbus::zvariant::Value;

        #[test]
        fn contrast_one_is_high() {
            assert_eq!(contrast_from_value(&Value::U32(1)), Some(true));
        }

        #[test]
        fn contrast_zero_is_off() {
            assert_eq!(contrast_from_value(&Value::U32(0)), Some(false));
        }

        #[test]
        fn unwraps_nested_variant() {
            // The portal typically returns the value inside a variant-in-variant.
            let nested = Value::Value(Box::new(Value::U32(1)));
            assert_eq!(contrast_from_value(&nested), Some(true));
        }

        #[test]
        fn unexpected_type_is_none() {
            assert_eq!(contrast_from_value(&Value::from("nope")), None);
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod imp {
    pub fn is_high_contrast_active() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::is_high_contrast_active;

    /// The fail-open contract: the query returns a bool and never panics, on every platform. On a CI host
    /// with no high-contrast set (all three OSes) this reads `false`; the point is that it resolves cleanly.
    #[test]
    fn resolves_without_panicking() {
        let _ = is_high_contrast_active();
    }
}
