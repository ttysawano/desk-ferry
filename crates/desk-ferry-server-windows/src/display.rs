use std::collections::{BTreeMap, BTreeSet};

use desk_ferry_common::{
    config::{AppConfig, WindowsDisplayConfig},
    DeskFerryError, Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorInfo {
    pub name: String,
    pub rect: Rect,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredDisplay {
    pub alias: String,
    pub match_name: String,
    pub monitor: MonitorInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayTopology {
    pub virtual_screen: Rect,
    pub monitors: Vec<MonitorInfo>,
    pub displays: BTreeMap<String, ConfiguredDisplay>,
}

impl DisplayTopology {
    pub fn display(&self, alias: &str) -> Option<&ConfiguredDisplay> {
        self.displays.get(alias)
    }

    pub fn display_at(&self, x: i32, y: i32) -> Option<&ConfiguredDisplay> {
        self.displays
            .values()
            .find(|display| display.monitor.rect.contains(x, y))
    }
}

pub fn virtual_screen(monitors: &[MonitorInfo]) -> Result<Rect> {
    let mut iter = monitors.iter();
    let first = iter
        .next()
        .ok_or_else(|| DeskFerryError::ConfigValidation("no Windows monitors found".into()))?;
    Ok(iter.fold(first.rect, |rect, monitor| rect.union(&monitor.rect)))
}

pub fn resolve_display_topology(
    config: &AppConfig,
    monitors: Vec<MonitorInfo>,
) -> Result<DisplayTopology> {
    let windows = config
        .windows
        .as_ref()
        .ok_or_else(|| DeskFerryError::ConfigValidation("windows section is required".into()))?;
    let virtual_screen = virtual_screen(&monitors)?;
    let displays = resolve_configured_displays(&windows.displays, &monitors)?;

    for alias in config.display_neighbors.keys() {
        if !displays.contains_key(alias) {
            return Err(DeskFerryError::ConfigValidation(format!(
                "display_neighbors references unknown display alias '{alias}'"
            )));
        }
    }

    Ok(DisplayTopology {
        virtual_screen,
        monitors,
        displays,
    })
}

fn resolve_configured_displays(
    configs: &[WindowsDisplayConfig],
    monitors: &[MonitorInfo],
) -> Result<BTreeMap<String, ConfiguredDisplay>> {
    let mut aliases = BTreeSet::new();
    let mut displays = BTreeMap::new();

    for display in configs {
        if !aliases.insert(display.alias.clone()) {
            return Err(DeskFerryError::ConfigValidation(format!(
                "duplicate display alias '{}'",
                display.alias
            )));
        }

        let monitor = monitors
            .iter()
            .find(|monitor| monitor_matches(&monitor.name, &display.match_name))
            .ok_or_else(|| {
                let available = monitors
                    .iter()
                    .map(|monitor| monitor.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                DeskFerryError::ConfigValidation(format!(
                    "display alias '{}' could not be matched to Windows monitor '{}'; available monitors: {}",
                    display.alias, display.match_name, available
                ))
            })?
            .clone();

        displays.insert(
            display.alias.clone(),
            ConfiguredDisplay {
                alias: display.alias.clone(),
                match_name: display.match_name.clone(),
                monitor,
            },
        );
    }

    Ok(displays)
}

fn monitor_matches(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
        || actual
            .trim_start_matches("\\\\.\\")
            .eq_ignore_ascii_case(expected)
        || actual
            .rsplit('\\')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_ferry_common::config::load_config_str;

    fn monitor(
        name: &str,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        primary: bool,
    ) -> MonitorInfo {
        MonitorInfo {
            name: name.to_string(),
            rect: Rect::new(left, top, right, bottom),
            primary,
        }
    }

    fn server_config() -> AppConfig {
        load_config_str(
            r#"
[host]
name = "host1"
role = "server"
os = "windows"

[network]
bind_address = "127.0.0.1"
port = 24800

[security]
mode = "tls"
psk_file = ".local/stage2-test.psk"
allow_plaintext = false

[transition]
entry_margin_px = 2
cooldown_ms = 150
require_double_push = false
edge_activation_delay_ms = 0

[input]
emergency_hotkey = "Ctrl+Alt+Shift+Esc"
keyboard_mode = "physical_key"
ime_sync = false

[windows]
use_virtual_screen = true

[[windows.displays]]
alias = "left"
match_name = "DISPLAY1"

[[windows.displays]]
alias = "main"
match_name = "DISPLAY2"

[neighbors]
left = ""
right = ""
up = ""
down = ""

[display_neighbors.main]
left = ""
right = "host2:left"
up = ""
down = ""

[clients.host2]
name = "host2"
allowed = true
expected_fingerprint = ""
"#,
        )
        .expect("valid config")
    }

    #[test]
    fn monitor_rectangles_keep_negative_virtual_coordinates() {
        let monitors = vec![
            monitor("\\\\.\\DISPLAY1", -1280, 0, 0, 1024, false),
            monitor("\\\\.\\DISPLAY2", 0, -200, 1920, 880, true),
        ];

        let rect = virtual_screen(&monitors).expect("virtual screen");

        assert_eq!(rect, Rect::new(-1280, -200, 1920, 1024));
        assert_eq!(monitors[0].rect.width(), 1280);
        assert_eq!(monitors[1].rect.height(), 1080);
    }

    #[test]
    fn display_aliases_are_matched_to_real_monitors() {
        let config = server_config();
        let topology = resolve_display_topology(
            &config,
            vec![
                monitor("\\\\.\\DISPLAY1", -1280, 0, 0, 1024, false),
                monitor("\\\\.\\DISPLAY2", 0, 0, 1920, 1080, true),
            ],
        )
        .expect("topology");

        assert_eq!(topology.display("left").unwrap().monitor.rect.left, -1280);
        assert_eq!(topology.display("main").unwrap().monitor.rect.right, 1920);
    }

    #[test]
    fn invalid_display_alias_is_rejected() {
        let config = load_config_str(
            r#"
[host]
name = "host1"
role = "server"
os = "windows"

[network]
bind_address = "127.0.0.1"
port = 24800

[security]
mode = "tls"
psk_file = ".local/stage2-test.psk"
allow_plaintext = false

[transition]
entry_margin_px = 2
cooldown_ms = 150
require_double_push = false
edge_activation_delay_ms = 0

[input]
emergency_hotkey = "Ctrl+Alt+Shift+Esc"
keyboard_mode = "physical_key"
ime_sync = false

[windows]
use_virtual_screen = true

[[windows.displays]]
alias = "main"
match_name = "DISPLAY1"

[neighbors]
left = ""
right = ""
up = ""
down = ""

[display_neighbors.missing]
left = ""
right = "host2:left"
up = ""
down = ""

[clients.host2]
name = "host2"
allowed = true
expected_fingerprint = ""
"#,
        )
        .expect("config parses before live display validation");

        let error = resolve_display_topology(
            &config,
            vec![monitor("\\\\.\\DISPLAY1", 0, 0, 1920, 1080, true)],
        )
        .expect_err("unknown alias must fail");

        assert!(error.to_string().contains("unknown display alias"));
    }

    #[test]
    fn unmatched_monitor_name_is_rejected() {
        let config = server_config();
        let error = resolve_display_topology(
            &config,
            vec![monitor("\\\\.\\DISPLAY1", 0, 0, 1920, 1080, true)],
        )
        .expect_err("missing display must fail");

        assert!(error.to_string().contains("could not be matched"));
    }
}
