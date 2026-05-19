use std::collections::BTreeMap;

use desk_ferry_common::{
    config::{parse_optional_neighbor, AppConfig, NeighborMap, NeighborSpec},
    protocol::{
        ActiveHostChanged, BoundaryRequest, Edge, MouseWarpEvent, ProtocolMessage,
        CURRENT_PROTOCOL_VERSION,
    },
    DeskFerryError, Result,
};

use crate::display::{ConfiguredDisplay, DisplayTopology, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryTransition {
    pub display_alias: String,
    pub from_edge: Edge,
    pub neighbor: NeighborSpec,
    pub position_ratio: f64,
    pub messages: Vec<ProtocolMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorMove {
    pub x: i32,
    pub y: i32,
    pub dx: i32,
    pub dy: i32,
    pub now_ms: u64,
}

#[derive(Debug, Clone)]
pub struct BoundaryEngine {
    host_name: String,
    topology: DisplayTopology,
    neighbors: BTreeMap<String, DisplayNeighborMap>,
    entry_margin_px: u32,
    cooldown_ms: u64,
    last_transition_ms: Option<u64>,
}

impl BoundaryEngine {
    pub fn new(config: &AppConfig, topology: DisplayTopology) -> Result<Self> {
        Ok(Self {
            host_name: config.host.name.clone(),
            neighbors: resolve_display_neighbors(config, &topology)?,
            topology,
            entry_margin_px: config.transition.entry_margin_px,
            cooldown_ms: config.transition.cooldown_ms,
            last_transition_ms: None,
        })
    }

    pub fn evaluate_move(&mut self, movement: CursorMove) -> Result<Option<BoundaryTransition>> {
        if self.is_in_cooldown(movement.now_ms) {
            return Ok(None);
        }

        let Some(display) = self.topology.display_at(movement.x, movement.y) else {
            return Ok(None);
        };
        let Some(edge) = outward_edge(display.monitor.rect, &movement) else {
            return Ok(None);
        };
        let Some(neighbor) = self
            .neighbors
            .get(&display.alias)
            .and_then(|neighbors| neighbors.get(edge))
            .cloned()
        else {
            return Ok(None);
        };

        let position_ratio = position_ratio(display.monitor.rect, edge, movement.x, movement.y);
        let messages = transition_messages(
            &self.host_name,
            edge,
            &neighbor,
            position_ratio,
            self.entry_margin_px,
        );
        self.last_transition_ms = Some(movement.now_ms);

        Ok(Some(BoundaryTransition {
            display_alias: display.alias.clone(),
            from_edge: edge,
            neighbor,
            position_ratio,
            messages,
        }))
    }

    fn is_in_cooldown(&self, now_ms: u64) -> bool {
        self.last_transition_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < self.cooldown_ms)
    }
}

#[derive(Debug, Clone, Default)]
struct DisplayNeighborMap {
    left: Option<NeighborSpec>,
    right: Option<NeighborSpec>,
    up: Option<NeighborSpec>,
    down: Option<NeighborSpec>,
}

impl DisplayNeighborMap {
    fn get(&self, edge: Edge) -> Option<&NeighborSpec> {
        match edge {
            Edge::Left => self.left.as_ref(),
            Edge::Right => self.right.as_ref(),
            Edge::Up => self.up.as_ref(),
            Edge::Down => self.down.as_ref(),
        }
    }
}

fn resolve_display_neighbors(
    config: &AppConfig,
    topology: &DisplayTopology,
) -> Result<BTreeMap<String, DisplayNeighborMap>> {
    let mut resolved = BTreeMap::new();

    for alias in topology.displays.keys() {
        let map = config
            .display_neighbors
            .get(alias)
            .unwrap_or(&config.neighbors);
        resolved.insert(alias.clone(), parse_neighbor_map(map)?);
    }

    for (alias, neighbors) in &resolved {
        for neighbor in [
            neighbors.left.as_ref(),
            neighbors.right.as_ref(),
            neighbors.up.as_ref(),
            neighbors.down.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            let allowed = config
                .clients
                .get(&neighbor.target_host)
                .is_some_and(|client| client.allowed);
            if !allowed {
                return Err(DeskFerryError::ConfigValidation(format!(
                    "display alias '{alias}' references unavailable target host '{}'",
                    neighbor.target_host
                )));
            }
        }
    }

    Ok(resolved)
}

fn parse_neighbor_map(map: &NeighborMap) -> Result<DisplayNeighborMap> {
    Ok(DisplayNeighborMap {
        left: parse_optional_neighbor(&map.left)?,
        right: parse_optional_neighbor(&map.right)?,
        up: parse_optional_neighbor(&map.up)?,
        down: parse_optional_neighbor(&map.down)?,
    })
}

pub fn outward_edge(rect: Rect, movement: &CursorMove) -> Option<Edge> {
    if movement.x <= rect.left && movement.dx < 0 {
        Some(Edge::Left)
    } else if movement.x >= rect.right - 1 && movement.dx > 0 {
        Some(Edge::Right)
    } else if movement.y <= rect.top && movement.dy < 0 {
        Some(Edge::Up)
    } else if movement.y >= rect.bottom - 1 && movement.dy > 0 {
        Some(Edge::Down)
    } else {
        None
    }
}

pub fn position_ratio(rect: Rect, edge: Edge, x: i32, y: i32) -> f64 {
    let (offset, span) = match edge {
        Edge::Left | Edge::Right => (y - rect.top, rect.height() - 1),
        Edge::Up | Edge::Down => (x - rect.left, rect.width() - 1),
    };
    if span <= 0 {
        return 0.0;
    }
    (offset as f64 / span as f64).clamp(0.0, 1.0)
}

pub fn entry_margin_coordinate(target_edge: Edge, entry_margin_px: u32) -> (f64, f64) {
    let margin = entry_margin_px as f64;
    match target_edge {
        Edge::Left => (margin, f64::NAN),
        Edge::Right => (-margin, f64::NAN),
        Edge::Up => (f64::NAN, margin),
        Edge::Down => (f64::NAN, -margin),
    }
}

fn transition_messages(
    from_host: &str,
    from_edge: Edge,
    neighbor: &NeighborSpec,
    position_ratio: f64,
    entry_margin_px: u32,
) -> Vec<ProtocolMessage> {
    let (x, y) = match neighbor.target_edge {
        Edge::Left => (entry_margin_px as f64, position_ratio),
        Edge::Right => (-(entry_margin_px as f64), position_ratio),
        Edge::Up => (position_ratio, entry_margin_px as f64),
        Edge::Down => (position_ratio, -(entry_margin_px as f64)),
    };

    vec![
        ProtocolMessage::BoundaryRequest(BoundaryRequest {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            from_host: from_host.to_string(),
            to_host: neighbor.target_host.clone(),
            from_edge,
            to_edge: neighbor.target_edge,
            position_ratio,
        }),
        ProtocolMessage::ActiveHostChanged(ActiveHostChanged {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            active_host: neighbor.target_host.clone(),
        }),
        ProtocolMessage::MouseWarp(MouseWarpEvent {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            x,
            y,
        }),
    ]
}

pub fn format_display_mapping(display: &ConfiguredDisplay) -> String {
    let rect = display.monitor.rect;
    format!(
        "alias={} match_name={} monitor={} rect=({}, {})-({}, {}) size={}x{} primary={}",
        display.alias,
        display.match_name,
        display.monitor.name,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        rect.width(),
        rect.height(),
        display.monitor.primary
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{resolve_display_topology, MonitorInfo};
    use desk_ferry_common::{config::load_config_str, protocol::ProtocolMessage};

    fn monitor(name: &str, left: i32, top: i32, right: i32, bottom: i32) -> MonitorInfo {
        MonitorInfo {
            name: name.to_string(),
            rect: Rect::new(left, top, right, bottom),
            primary: left == 0 && top == 0,
        }
    }

    fn config() -> AppConfig {
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
entry_margin_px = 3
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

[display_neighbors.left]
left = "host2:right"
right = ""
up = ""
down = ""

[display_neighbors.main]
left = ""
right = "host2:left"
up = "host2:down"
down = "host2:up"

[clients.host2]
name = "host2"
allowed = true
expected_fingerprint = ""
"#,
        )
        .expect("valid config")
    }

    fn engine() -> BoundaryEngine {
        let config = config();
        let topology = resolve_display_topology(
            &config,
            vec![
                monitor("\\\\.\\DISPLAY1", -1280, 0, 0, 1024),
                monitor("\\\\.\\DISPLAY2", 0, -200, 1920, 880),
            ],
        )
        .expect("topology");
        BoundaryEngine::new(&config, topology).expect("engine")
    }

    #[test]
    fn display_neighbors_are_resolved_per_alias() {
        let mut engine = engine();
        let transition = engine
            .evaluate_move(CursorMove {
                x: 1919,
                y: 340,
                dx: 1,
                dy: 0,
                now_ms: 1_000,
            })
            .expect("evaluated")
            .expect("transition");

        assert_eq!(transition.display_alias, "main");
        assert_eq!(transition.from_edge, Edge::Right);
        assert_eq!(transition.neighbor.target_host, "host2");
        assert_eq!(transition.neighbor.target_edge, Edge::Left);
    }

    #[test]
    fn right_left_up_and_down_edges_require_outward_motion() {
        let rect = Rect::new(0, 0, 100, 50);

        assert_eq!(
            outward_edge(
                rect,
                &CursorMove {
                    x: 99,
                    y: 20,
                    dx: 1,
                    dy: 0,
                    now_ms: 0,
                },
            ),
            Some(Edge::Right)
        );
        assert_eq!(
            outward_edge(
                rect,
                &CursorMove {
                    x: 0,
                    y: 20,
                    dx: -1,
                    dy: 0,
                    now_ms: 0,
                },
            ),
            Some(Edge::Left)
        );
        assert_eq!(
            outward_edge(
                rect,
                &CursorMove {
                    x: 20,
                    y: 0,
                    dx: 0,
                    dy: -1,
                    now_ms: 0,
                },
            ),
            Some(Edge::Up)
        );
        assert_eq!(
            outward_edge(
                rect,
                &CursorMove {
                    x: 20,
                    y: 49,
                    dx: 0,
                    dy: 1,
                    now_ms: 0,
                },
            ),
            Some(Edge::Down)
        );
    }

    #[test]
    fn being_on_edge_without_outward_motion_does_not_transition() {
        let rect = Rect::new(0, 0, 100, 50);

        assert_eq!(
            outward_edge(
                rect,
                &CursorMove {
                    x: 99,
                    y: 20,
                    dx: 0,
                    dy: 0,
                    now_ms: 0,
                },
            ),
            None
        );
        assert_eq!(
            outward_edge(
                rect,
                &CursorMove {
                    x: 0,
                    y: 20,
                    dx: 1,
                    dy: 0,
                    now_ms: 0,
                },
            ),
            None
        );
    }

    #[test]
    fn pos_ratio_is_calculated_along_the_cross_axis() {
        let rect = Rect::new(-100, -50, 101, 51);

        assert_eq!(position_ratio(rect, Edge::Left, -100, -50), 0.0);
        assert_eq!(position_ratio(rect, Edge::Right, 100, 50), 1.0);
        assert!((position_ratio(rect, Edge::Up, 0, -50) - 0.5).abs() < 0.0001);
        assert!((position_ratio(rect, Edge::Down, 0, 50) - 0.5).abs() < 0.0001);
    }

    #[test]
    fn transition_messages_include_boundary_active_host_and_mouse_warp() {
        let mut engine = engine();
        let transition = engine
            .evaluate_move(CursorMove {
                x: 1919,
                y: 340,
                dx: 4,
                dy: 0,
                now_ms: 1_000,
            })
            .expect("evaluated")
            .expect("transition");

        assert_eq!(transition.messages.len(), 3);
        assert!(matches!(
            transition.messages[0],
            ProtocolMessage::BoundaryRequest(_)
        ));
        assert!(matches!(
            transition.messages[1],
            ProtocolMessage::ActiveHostChanged(_)
        ));
        match &transition.messages[2] {
            ProtocolMessage::MouseWarp(warp) => {
                assert_eq!(warp.x, 3.0);
                assert!((warp.y - transition.position_ratio).abs() < 0.0001);
            }
            other => panic!("expected mouse_warp, got {other:?}"),
        }
    }

    #[test]
    fn cooldown_suppresses_immediate_retransition() {
        let mut engine = engine();
        let first = engine
            .evaluate_move(CursorMove {
                x: 1919,
                y: 340,
                dx: 1,
                dy: 0,
                now_ms: 1_000,
            })
            .expect("evaluated");
        let second = engine
            .evaluate_move(CursorMove {
                x: 1919,
                y: 340,
                dx: 1,
                dy: 0,
                now_ms: 1_100,
            })
            .expect("evaluated");
        let third = engine
            .evaluate_move(CursorMove {
                x: 1919,
                y: 340,
                dx: 1,
                dy: 0,
                now_ms: 1_151,
            })
            .expect("evaluated");

        assert!(first.is_some());
        assert!(second.is_none());
        assert!(third.is_some());
    }

    #[test]
    fn unavailable_target_host_is_rejected() {
        let mut config = config();
        config.clients.get_mut("host2").unwrap().allowed = false;
        let topology = resolve_display_topology(
            &config,
            vec![
                monitor("\\\\.\\DISPLAY1", -1280, 0, 0, 1024),
                monitor("\\\\.\\DISPLAY2", 0, -200, 1920, 880),
            ],
        )
        .expect("topology");
        let error = BoundaryEngine::new(&config, topology).expect_err("target host must fail");

        assert!(error.to_string().contains("unavailable target host"));
    }
}
