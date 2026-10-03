// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The config file, read once at startup.
//!
//! `$XDG_CONFIG_HOME/runebender/config.toml`, or `~/.config/runebender/config.toml`, the file the
//! GPUI shell reads too. Everything in it is optional, and sections this shell does not use are
//! skipped, so both shells and other tools can keep their settings in one file. A file that is
//! missing or malformed is the same as no file: a broken config must not stop a font opening.
//!
//! ```toml
//! [editing]
//! grid = 2          # moved points snap to this many units; 0 turns snapping off; default 1
//! nudge = 2         # an arrow key moves this far; default one grid step
//! shift_nudge = 8   # Shift and an arrow key; default four nudges
//! ```

use std::path::PathBuf;

use serde::Deserialize;

/// What the file can say to this shell. Every field is optional.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct Config {
    /// How points move and snap.
    pub editing: Editing,
}

/// The `[editing]` section.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Editing {
    /// The design grid, in font units; zero turns snapping off.
    pub grid: Option<f64>,
    /// How far an arrow key moves a selection, in font units.
    pub nudge: Option<f64>,
    /// How far Shift and an arrow key move a selection, in font units.
    pub shift_nudge: Option<f64>,
}

/// The nudge distances in effect, set once at startup.
static NUDGE: std::sync::OnceLock<(f64, f64)> = std::sync::OnceLock::new();

/// How far an arrow key moves a selection, and how far with Shift, in font units.
pub(crate) fn nudge() -> (f64, f64) {
    *NUDGE.get_or_init(|| Config::default().nudge())
}

impl Config {
    /// The nudge distances this config gives, with the defaults filled in.
    fn nudge(&self) -> (f64, f64) {
        let positive = |value: Option<f64>| value.filter(|v| v.is_finite() && *v > 0.0);
        let grid = self.grid();
        let step = positive(self.editing.nudge).unwrap_or(if grid > 0.0 { grid } else { 1.0 });
        (
            step,
            positive(self.editing.shift_nudge).unwrap_or(4.0 * step),
        )
    }

    /// The design grid this config gives, with the default filled in.
    fn grid(&self) -> f64 {
        self.editing
            .grid
            .filter(|grid| grid.is_finite() && *grid >= 0.0)
            .unwrap_or(runebender::outline::point_ops::DEFAULT_GRID_SPACING)
    }

    /// Put this config into effect for the whole process. Only the first call sets the nudge.
    pub(crate) fn apply(&self) {
        runebender::outline::point_ops::set_grid_spacing(self.grid());
        let _ = NUDGE.set(self.nudge());
    }
}

/// Where the file lives, whether or not it exists.
pub(crate) fn path() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("runebender/config.toml"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/runebender/config.toml"))
}

/// Parse a config from text.
pub(crate) fn parse(text: &str) -> Result<Config, toml::de::Error> {
    toml::from_str(text)
}

/// The config for this run. Never fails: a missing or bad file yields defaults.
pub(crate) fn load() -> Config {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| parse(&text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_whole_units_and_four_times_with_shift() {
        let config = Config::default();
        assert_eq!(config.grid(), 1.0);
        assert_eq!(config.nudge(), (1.0, 4.0));
    }

    #[test]
    fn a_grid_sets_the_nudge_unless_the_nudge_is_given() {
        let config = parse("[editing]\ngrid = 2\n").unwrap();
        assert_eq!((config.grid(), config.nudge()), (2.0, (2.0, 8.0)));
        let config = parse("[editing]\ngrid = 2\nshift_nudge = 10\n").unwrap();
        assert_eq!(config.nudge(), (2.0, 10.0));
        let config = parse("[editing]\ngrid = 0\n").unwrap();
        assert_eq!(
            (config.grid(), config.nudge()),
            (0.0, (1.0, 4.0)),
            "no snapping"
        );
    }

    #[test]
    fn other_sections_are_skipped_and_bad_values_fall_back() {
        let config =
            parse("theme = \"gray\"\n[quiver]\napi_key = \"x\"\n[editing]\ngrid = 4\n").unwrap();
        assert_eq!(config.grid(), 4.0);
        let config = parse("[editing]\ngrid = -3\nnudge = 0\n").unwrap();
        assert_eq!((config.grid(), config.nudge()), (1.0, (1.0, 4.0)));
        assert!(
            parse("[editing]\ngird = 2\n").is_err(),
            "a misspelled key is reported"
        );
    }
}
