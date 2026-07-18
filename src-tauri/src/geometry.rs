//! Pure window-geometry resolver (CPE-598, epic CPE-580).
//!
//! Turns parsed CLI geometry args + the monitor work-area list + the default rect into a **final,
//! on-screen window rect** — the headlessly-testable brain of the launch-geometry feature. All the
//! foolproofing lives here: precedence (`CLI flag > default`), off-screen clamping, preset resolution,
//! `--monitor` selection, and logical/physical conversion. The live `apply` step (CPE-600) just consumes
//! the [`Resolved`] rect; this module has no Tauri deps so it can be exhaustively unit-tested.
//!
//! **Contract:** all rects are **logical pixels** (stable across DPI). `--physical` means the *inputs*
//! were physical, so they're converted to logical here using the target monitor's scale.

/// A window rectangle in **logical** pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A monitor's usable **work area** (logical px; excludes taskbar/dock) + its DPI `scale`.
#[derive(Debug, Clone, Copy)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// A named position; resolves down to plain x/y within the target monitor (not a special case).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

impl Preset {
    /// Parse a `--position` value; `None` if unrecognised (the caller reports it).
    pub fn parse(s: &str) -> Option<Preset> {
        match s.trim().to_ascii_lowercase().replace(['-', '_'], "").as_str() {
            "topleft" | "tl" => Some(Preset::TopLeft),
            "topright" | "tr" => Some(Preset::TopRight),
            "bottomleft" | "bl" => Some(Preset::BottomLeft),
            "bottomright" | "br" => Some(Preset::BottomRight),
            "center" | "centre" | "middle" => Some(Preset::Center),
            _ => None,
        }
    }
}

/// The parsed geometry knobs. Every field is independently optional (orthogonal); unspecified fields fall
/// back to the default. Convenience flags (`position`/`monitor`/`maximized`/`fullscreen`) resolve down to
/// the same four scalars.
#[derive(Debug, Default, Clone)]
pub struct GeometryArgs {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub position: Option<Preset>,
    pub monitor: Option<usize>,
    pub maximized: bool,
    pub fullscreen: bool,
    /// The x/y/width/height inputs are physical pixels (default: logical).
    pub physical: bool,
}

/// The resolved, ready-to-apply window state.
#[derive(Debug, PartialEq, Eq)]
pub struct Resolved {
    pub rect: Rect,
    pub maximized: bool,
    pub fullscreen: bool,
    /// Non-fatal adjustments (e.g. clamped onto the work area) for the caller to log.
    pub warnings: Vec<String>,
}

/// A geometry request that can't be honoured at all.
#[derive(Debug, PartialEq, Eq)]
pub enum GeometryError {
    /// A zero (or, after conversion, zero) width/height was requested.
    ZeroSize,
    /// No monitors were reported, so nothing can be positioned/clamped.
    NoMonitors,
}

impl std::fmt::Display for GeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeometryError::ZeroSize => write!(f, "window width/height must be greater than zero"),
            GeometryError::NoMonitors => write!(f, "no monitors available to position the window"),
        }
    }
}

/// Build [`GeometryArgs`] from a `flag name → value` lookup (the shape `tauri-plugin-cli` hands us:
/// a JSON string for value flags, a bool for switches). Kept pure + testable, independent of the plugin.
/// A **present-but-unparseable** numeric flag is a hard error (the caller exits non-zero) — never a
/// silently-ignored value. Absent flags stay `None`.
pub fn parse_args(get: &dyn Fn(&str) -> Option<serde_json::Value>) -> Result<GeometryArgs, String> {
    fn num<T: std::str::FromStr>(get: &dyn Fn(&str) -> Option<serde_json::Value>, key: &str) -> Result<Option<T>, String> {
        match get(key) {
            None | Some(serde_json::Value::Null) | Some(serde_json::Value::Bool(false)) => Ok(None),
            Some(serde_json::Value::String(s)) => {
                s.parse::<T>().map(Some).map_err(|_| format!("--{key}: '{s}' is not a valid number"))
            }
            Some(serde_json::Value::Number(n)) => {
                n.to_string().parse::<T>().map(Some).map_err(|_| format!("--{key}: '{n}' is out of range"))
            }
            Some(other) => Err(format!("--{key}: unexpected value {other}")),
        }
    }
    let flag = |key: &str| matches!(get(key), Some(serde_json::Value::Bool(true)));
    let position = match get("position") {
        Some(serde_json::Value::String(s)) => {
            Some(Preset::parse(&s).ok_or_else(|| format!("--position: '{s}' is not a known preset"))?)
        }
        _ => None,
    };
    Ok(GeometryArgs {
        x: num(get, "x")?,
        y: num(get, "y")?,
        width: num(get, "width")?,
        height: num(get, "height")?,
        position,
        monitor: num(get, "monitor")?,
        maximized: flag("maximized"),
        fullscreen: flag("fullscreen"),
        physical: flag("physical"),
    })
}

fn preset_position(p: Preset, wa: &WorkArea, w: u32, h: u32) -> (i32, i32) {
    let right = wa.x + (wa.width.saturating_sub(w)) as i32;
    let bottom = wa.y + (wa.height.saturating_sub(h)) as i32;
    let cx = wa.x + (wa.width.saturating_sub(w) / 2) as i32;
    let cy = wa.y + (wa.height.saturating_sub(h) / 2) as i32;
    match p {
        Preset::TopLeft => (wa.x, wa.y),
        Preset::TopRight => (right, wa.y),
        Preset::BottomLeft => (wa.x, bottom),
        Preset::BottomRight => (right, bottom),
        Preset::Center => (cx, cy),
    }
}

/// Resolve the final window rect. Precedence per field: **explicit CLI value > preset > default**. The
/// result is always fully within the target monitor's work area (size clamped to fit, position clamped so
/// the whole window — and thus its title bar — is grabbable). Errors only on a zero size or no monitors.
pub fn resolve(args: &GeometryArgs, monitors: &[WorkArea], default: Rect) -> Result<Resolved, GeometryError> {
    let wa = match args.monitor {
        Some(n) => monitors.get(n).or_else(|| monitors.first()),
        None => monitors.first(),
    }
    .ok_or(GeometryError::NoMonitors)?;

    let mut warnings = Vec::new();
    if let Some(n) = args.monitor {
        if monitors.get(n).is_none() {
            warnings.push(format!("monitor {n} not found; using the primary monitor"));
        }
    }

    // `--physical` means the inputs are physical px; convert to the logical contract.
    let li = |v: i32| if args.physical { (v as f64 / wa.scale).round() as i32 } else { v };
    let lu = |v: u32| if args.physical { (v as f64 / wa.scale).round() as u32 } else { v };

    // Size: explicit > default.
    let want_w = args.width.map(lu).unwrap_or(default.width);
    let want_h = args.height.map(lu).unwrap_or(default.height);
    if want_w == 0 || want_h == 0 {
        return Err(GeometryError::ZeroSize);
    }

    // Clamp size to the monitor work area first (position depends on the final size).
    let width = want_w.min(wa.width.max(1));
    let height = want_h.min(wa.height.max(1));
    if width != want_w || height != want_h {
        warnings.push("size clamped to the monitor work area".into());
    }

    // Position: explicit x/y win per-axis; else a preset within the target monitor; else the default.
    let preset = args.position.map(|p| preset_position(p, wa, width, height));
    let want_x = args.x.map(li).or(preset.map(|(px, _)| px)).unwrap_or(default.x);
    let want_y = args.y.map(li).or(preset.map(|(_, py)| py)).unwrap_or(default.y);

    // Off-screen protection: clamp so the whole window sits inside the work area (never ungrabbable).
    let max_x = wa.x + (wa.width - width) as i32;
    let max_y = wa.y + (wa.height - height) as i32;
    let x = want_x.clamp(wa.x, max_x);
    let y = want_y.clamp(wa.y, max_y);
    if x != want_x || y != want_y {
        warnings.push("position clamped onto the monitor work area".into());
    }

    Ok(Resolved { rect: Rect { x, y, width, height }, maximized: args.maximized, fullscreen: args.fullscreen, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(x: i32, y: i32, w: u32, h: u32) -> WorkArea {
        WorkArea { x, y, width: w, height: h, scale: 1.0 }
    }
    fn dflt() -> Rect {
        Rect { x: 50, y: 50, width: 800, height: 600 }
    }

    #[test]
    fn explicit_rect_inside_the_monitor_passes_through() {
        let args = GeometryArgs { x: Some(100), y: Some(120), width: Some(1200), height: Some(800), ..Default::default() };
        let r = resolve(&args, &[mon(0, 0, 1920, 1080)], dflt()).unwrap();
        assert_eq!(r.rect, Rect { x: 100, y: 120, width: 1200, height: 800 });
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn omitted_fields_fall_back_to_the_default_each_independently() {
        let args = GeometryArgs { width: Some(1000), ..Default::default() }; // only width given
        let r = resolve(&args, &[mon(0, 0, 1920, 1080)], dflt()).unwrap();
        assert_eq!(r.rect, Rect { x: 50, y: 50, width: 1000, height: 600 });
    }

    #[test]
    fn an_offscreen_position_is_clamped_onto_the_work_area() {
        let args = GeometryArgs { x: Some(99999), y: Some(-500), width: Some(400), height: Some(300), ..Default::default() };
        let r = resolve(&args, &[mon(0, 0, 1920, 1080)], dflt()).unwrap();
        assert_eq!(r.rect, Rect { x: 1920 - 400, y: 0, width: 400, height: 300 });
        assert!(r.warnings.iter().any(|w| w.contains("position clamped")));
    }

    #[test]
    fn an_oversized_window_is_clamped_to_the_monitor() {
        let args = GeometryArgs { width: Some(5000), height: Some(5000), x: Some(0), y: Some(0), ..Default::default() };
        let r = resolve(&args, &[mon(0, 0, 1280, 800)], dflt()).unwrap();
        assert_eq!(r.rect, Rect { x: 0, y: 0, width: 1280, height: 800 });
        assert!(r.warnings.iter().any(|w| w.contains("size clamped")));
    }

    #[test]
    fn preset_center_centers_and_explicit_x_wins_over_the_preset() {
        let args = GeometryArgs { position: Some(Preset::Center), width: Some(800), height: Some(600), ..Default::default() };
        let r = resolve(&args, &[mon(0, 0, 1920, 1080)], dflt()).unwrap();
        assert_eq!(r.rect, Rect { x: (1920 - 800) / 2, y: (1080 - 600) / 2, width: 800, height: 600 });

        let args2 = GeometryArgs { x: Some(10), ..args };
        let r2 = resolve(&args2, &[mon(0, 0, 1920, 1080)], dflt()).unwrap();
        assert_eq!(r2.rect.x, 10, "explicit x overrides the preset");
        assert_eq!(r2.rect.y, (1080 - 600) / 2, "y still from the preset");
    }

    #[test]
    fn monitor_selection_positions_relative_to_that_display() {
        let monitors = [mon(0, 0, 1920, 1080), mon(1920, 0, 1280, 1024)];
        let args = GeometryArgs { monitor: Some(1), position: Some(Preset::TopLeft), width: Some(400), height: Some(300), ..Default::default() };
        let r = resolve(&args, &monitors, dflt()).unwrap();
        assert_eq!(r.rect, Rect { x: 1920, y: 0, width: 400, height: 300 });
    }

    #[test]
    fn physical_inputs_convert_to_logical_by_the_monitor_scale() {
        let args = GeometryArgs { width: Some(2400), height: Some(1600), x: Some(200), y: Some(100), physical: true, ..Default::default() };
        let r = resolve(&args, &[WorkArea { x: 0, y: 0, width: 1920, height: 1080, scale: 2.0 }], dflt()).unwrap();
        // 2400x1600 physical / 2.0 = 1200x800 logical at (100,50).
        assert_eq!(r.rect, Rect { x: 100, y: 50, width: 1200, height: 800 });
    }

    #[test]
    fn zero_size_and_no_monitors_are_errors() {
        let z = GeometryArgs { width: Some(0), height: Some(100), ..Default::default() };
        assert_eq!(resolve(&z, &[mon(0, 0, 800, 600)], dflt()), Err(GeometryError::ZeroSize));
        let any = GeometryArgs::default();
        assert_eq!(resolve(&any, &[], dflt()), Err(GeometryError::NoMonitors));
    }

    #[test]
    fn maximized_and_fullscreen_pass_through() {
        let args = GeometryArgs { maximized: true, fullscreen: true, ..Default::default() };
        let r = resolve(&args, &[mon(0, 0, 1920, 1080)], dflt()).unwrap();
        assert!(r.maximized && r.fullscreen);
    }

    #[test]
    fn parse_args_reads_values_flags_and_rejects_junk() {
        use serde_json::json;
        let m = |pairs: Vec<(&str, serde_json::Value)>| {
            let map: std::collections::HashMap<String, serde_json::Value> =
                pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
            move |k: &str| map.get(k).cloned()
        };
        // Numeric strings + switches parse; an unknown flag stays None.
        let g = m(vec![("x", json!("100")), ("width", json!("1200")), ("maximized", json!(true)), ("physical", json!(false))]);
        let a = parse_args(&g).unwrap();
        assert_eq!(a.x, Some(100));
        assert_eq!(a.width, Some(1200));
        assert!(a.maximized && !a.physical);
        assert_eq!(a.y, None, "absent flag stays None");
        // Present-but-junk numeric → hard error.
        let bad = m(vec![("width", json!("abc"))]);
        assert!(parse_args(&bad).is_err());
        // A bad preset name errors too.
        let badp = m(vec![("position", json!("sideways"))]);
        assert!(parse_args(&badp).is_err());
        let goodp = m(vec![("position", json!("center"))]);
        assert_eq!(parse_args(&goodp).unwrap().position, Some(Preset::Center));
    }

    #[test]
    fn preset_parse_accepts_common_spellings() {
        assert_eq!(Preset::parse("center"), Some(Preset::Center));
        assert_eq!(Preset::parse("top-right"), Some(Preset::TopRight));
        assert_eq!(Preset::parse("BR"), Some(Preset::BottomRight));
        assert_eq!(Preset::parse("nonsense"), None);
    }
}
