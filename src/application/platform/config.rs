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
//! command_nudge = 64 # Command, Shift and an arrow key; default eight Shift nudges
//!
//! [neural]
//! post_opentype = "/Users/me/GH/repos/post-opentype"  # default ~/GH/repos/post-opentype
//! train_host = "kiln"   # ssh host that trains; "" for this machine; default kiln
//! epochs = 800
//! ```

use std::path::PathBuf;

use serde::Deserialize;

/// What the file can say to this shell. Every field is optional.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct Config {
    /// How points move and snap.
    pub editing: Editing,
    /// Training neural fonts from a `.nufo` source.
    pub neural: NeuralConfig,
}

/// The `[neural]` section.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct NeuralConfig {
    /// The post-opentype checkout that holds the training script and tools.
    pub post_opentype: Option<PathBuf>,
    /// The ssh host that trains; empty for this machine only.
    pub train_host: Option<String>,
    /// Epochs of one training run.
    pub epochs: Option<u32>,
}

/// Where training happens and for how long, with the defaults filled in.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Neural {
    pub post_opentype: PathBuf,
    pub train_host: Option<String>,
    pub epochs: u32,
}

/// The training settings in effect, set once at startup.
static NEURAL: std::sync::OnceLock<Neural> = std::sync::OnceLock::new();

/// Where training happens and for how long.
pub(crate) fn neural() -> Neural {
    NEURAL.get_or_init(|| Config::default().neural()).clone()
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
    /// How far Command, Shift and an arrow key move a selection, in font units.
    pub command_nudge: Option<f64>,
}

/// How far the arrow keys move a selection, in font units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Nudge {
    /// An arrow key alone.
    pub step: f64,
    /// Shift and an arrow key.
    pub shift: f64,
    /// Command, Shift and an arrow key, for large moves.
    pub command: f64,
}

impl Nudge {
    /// The distance for one arrow press with these modifiers.
    pub(crate) fn for_modifiers(self, shift: bool, command: bool) -> f64 {
        match (shift, command) {
            (true, true) => self.command,
            (true, false) => self.shift,
            _ => self.step,
        }
    }
}

/// The nudge distances in effect, set once at startup.
static NUDGE: std::sync::OnceLock<Nudge> = std::sync::OnceLock::new();

/// How far the arrow keys move a selection, in font units.
pub(crate) fn nudge() -> Nudge {
    *NUDGE.get_or_init(|| Config::default().nudge())
}

impl Config {
    /// The nudge distances this config gives, with the defaults filled in.
    fn nudge(&self) -> Nudge {
        let positive = |value: Option<f64>| value.filter(|v| v.is_finite() && *v > 0.0);
        let grid = self.grid();
        let step = positive(self.editing.nudge).unwrap_or(if grid > 0.0 { grid } else { 1.0 });
        let shift = positive(self.editing.shift_nudge).unwrap_or(4.0 * step);
        Nudge {
            step,
            shift,
            command: positive(self.editing.command_nudge).unwrap_or(8.0 * shift),
        }
    }

    /// The design grid this config gives, with the default filled in.
    fn grid(&self) -> f64 {
        self.editing
            .grid
            .filter(|grid| grid.is_finite() && *grid >= 0.0)
            .unwrap_or(runebender::outline::point_ops::DEFAULT_GRID_SPACING)
    }

    /// The training settings this config gives, with the defaults filled in: the checkout at
    /// `~/GH/repos/post-opentype`, kiln, 800 epochs.
    fn neural(&self) -> Neural {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        Neural {
            post_opentype: self
                .neural
                .post_opentype
                .clone()
                .unwrap_or_else(|| home.join("GH/repos/post-opentype")),
            train_host: match self.neural.train_host.as_deref() {
                None => Some("kiln".into()),
                Some("") => None,
                Some(host) => Some(host.into()),
            },
            epochs: self.neural.epochs.filter(|e| *e > 0).unwrap_or(800),
        }
    }

    /// Put this config into effect for the whole process. Only the first call sets the nudge.
    pub(crate) fn apply(&self) {
        runebender::outline::point_ops::set_grid_spacing(self.grid());
        let _ = NUDGE.set(self.nudge());
        let _ = NEURAL.set(self.neural());
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
        assert_eq!(config.nudge(), nudge(1.0, 4.0, 32.0));
    }

    fn nudge(step: f64, shift: f64, command: f64) -> Nudge {
        Nudge {
            step,
            shift,
            command,
        }
    }

    #[test]
    fn command_shift_nudges_far_and_can_be_configured() {
        let config = parse("[editing]\ncommand_nudge = 100\n").unwrap();
        assert_eq!(config.nudge(), nudge(1.0, 4.0, 100.0));
        let n = config.nudge();
        assert_eq!(
            [
                n.for_modifiers(false, false),
                n.for_modifiers(true, false),
                n.for_modifiers(true, true),
                n.for_modifiers(false, true),
            ],
            [1.0, 4.0, 100.0, 1.0],
            "Command alone keeps the plain step"
        );
    }

    #[test]
    fn a_grid_sets_the_nudge_unless_the_nudge_is_given() {
        let config = parse("[editing]\ngrid = 2\n").unwrap();
        assert_eq!(
            (config.grid(), config.nudge()),
            (2.0, nudge(2.0, 8.0, 64.0))
        );
        let config = parse("[editing]\ngrid = 2\nshift_nudge = 10\n").unwrap();
        assert_eq!(config.nudge(), nudge(2.0, 10.0, 80.0));
        let config = parse("[editing]\ngrid = 0\n").unwrap();
        assert_eq!(
            (config.grid(), config.nudge()),
            (0.0, nudge(1.0, 4.0, 32.0)),
            "no snapping"
        );
    }

    #[test]
    fn other_sections_are_skipped_and_bad_values_fall_back() {
        let config =
            parse("theme = \"gray\"\n[quiver]\napi_key = \"x\"\n[editing]\ngrid = 4\n").unwrap();
        assert_eq!(config.grid(), 4.0);
        let config = parse("[editing]\ngrid = -3\nnudge = 0\n").unwrap();
        assert_eq!(
            (config.grid(), config.nudge()),
            (1.0, nudge(1.0, 4.0, 32.0))
        );
        assert!(
            parse("[editing]\ngird = 2\n").is_err(),
            "a misspelled key is reported"
        );
    }
}
