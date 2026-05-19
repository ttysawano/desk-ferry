use std::collections::BTreeSet;

use desk_ferry_common::{
    config::AppConfig,
    protocol::{ActiveHostChanged, BoundaryRequest, ProtocolMessage, CURRENT_PROTOCOL_VERSION},
    DeskFerryError, Result,
};

use crate::{
    display::DisplayTopology,
    input::{
        EmergencyHotkey, InputActionKind, InputEngine, InputMode, InputProcessResult, RawInputEvent,
    },
    transition::{BoundaryEngine, CursorMove},
};

#[derive(Debug, Clone, PartialEq)]
pub struct ServerEventResult {
    pub log_events: Vec<&'static str>,
    pub messages: Vec<ProtocolMessage>,
    pub suppress_input: bool,
}

impl ServerEventResult {
    fn empty() -> Self {
        Self {
            log_events: Vec::new(),
            messages: Vec::new(),
            suppress_input: false,
        }
    }
}

#[derive(Debug)]
pub struct WindowsServerIntegration {
    server_host: String,
    authenticated_clients: BTreeSet<String>,
    input_engine: InputEngine,
    boundary_engine: BoundaryEngine,
}

impl WindowsServerIntegration {
    pub fn new(
        config: &AppConfig,
        topology: DisplayTopology,
        mode: InputMode,
        emergency_hotkey: EmergencyHotkey,
    ) -> Result<Self> {
        Ok(Self {
            server_host: config.host.name.clone(),
            authenticated_clients: BTreeSet::new(),
            input_engine: InputEngine::new_with_hotkey(
                config.host.name.clone(),
                mode,
                emergency_hotkey,
            ),
            boundary_engine: BoundaryEngine::new(config, topology)?,
        })
    }

    pub fn active_host(&self) -> &str {
        self.input_engine.active_host()
    }

    pub fn is_client_registered(&self, host: &str) -> bool {
        self.authenticated_clients.contains(host)
    }

    pub fn register_authenticated_client(&mut self, host: impl Into<String>) -> ServerEventResult {
        self.authenticated_clients.insert(host.into());
        ServerEventResult {
            log_events: vec!["client authenticated"],
            messages: vec![self.active_host_changed()],
            suppress_input: false,
        }
    }

    pub fn handle_local_input(
        &mut self,
        event: RawInputEvent,
        now_ms: u64,
    ) -> Result<ServerEventResult> {
        if matches!(event, RawInputEvent::Key { .. }) {
            let result = self.input_engine.handle_event(event);
            return Ok(Self::from_input_result(result));
        }

        if let RawInputEvent::MouseMove { x, y, dx, dy } = event {
            if self.active_host() == self.server_host {
                if let Some(transition) = self.boundary_engine.evaluate_move(CursorMove {
                    x,
                    y,
                    dx,
                    dy,
                    now_ms,
                })? {
                    self.input_engine
                        .set_active_host(transition.neighbor.target_host.clone());
                    return Ok(ServerEventResult {
                        log_events: vec!["boundary transition detected", "active_host changed"],
                        messages: transition.messages,
                        suppress_input: false,
                    });
                }
            }
            let result = self
                .input_engine
                .handle_event(RawInputEvent::MouseMove { x, y, dx, dy });
            return Ok(Self::from_input_result(result));
        }

        let result = self.input_engine.handle_event(event);
        Ok(Self::from_input_result(result))
    }

    pub fn handle_client_message(&mut self, message: ProtocolMessage) -> Result<ServerEventResult> {
        match message {
            ProtocolMessage::BoundaryRequest(request) => self.handle_boundary_request(request),
            _ => Ok(ServerEventResult::empty()),
        }
    }

    pub fn handle_disconnect(&mut self) -> ServerEventResult {
        self.authenticated_clients.clear();
        let result = self.input_engine.handle_event(RawInputEvent::Disconnect);
        Self::from_input_result(result)
    }

    fn handle_boundary_request(&mut self, request: BoundaryRequest) -> Result<ServerEventResult> {
        if !self.authenticated_clients.contains(&request.from_host) {
            return Err(DeskFerryError::Authentication(
                "boundary request from unauthenticated client".to_string(),
            ));
        }
        if request.to_host != self.server_host {
            return Err(DeskFerryError::Authentication(
                "boundary request target is not this server".to_string(),
            ));
        }

        self.input_engine.set_active_host(self.server_host.clone());
        Ok(ServerEventResult {
            log_events: vec!["active_host changed"],
            messages: vec![self.active_host_changed()],
            suppress_input: false,
        })
    }

    fn from_input_result(result: InputProcessResult) -> ServerEventResult {
        let mut log_events = Vec::new();
        let mut messages = Vec::new();

        for action in result.actions {
            match action.kind {
                InputActionKind::MouseMoveDetected
                | InputActionKind::MouseButtonDetected
                | InputActionKind::MouseWheelDetected => {
                    if action.message.is_some() {
                        log_events.push("mouse event forwarded");
                    }
                }
                InputActionKind::KeyDetected => {
                    if action.message.is_some() {
                        log_events.push("key event forwarded");
                    }
                }
                InputActionKind::EmergencyHotkeyDetected => {
                    log_events.push("emergency hotkey detected");
                }
                InputActionKind::ActiveHostChanged => {
                    log_events.push("active_host changed");
                }
                InputActionKind::ReleaseAllGenerated => {
                    log_events.push("release_all generated");
                }
                InputActionKind::ConnectionDisconnected => {
                    log_events.push("client disconnected");
                }
            }

            if let Some(message) = action.message {
                messages.push(message);
            }
        }

        ServerEventResult {
            log_events,
            messages,
            suppress_input: result.suppress_input,
        }
    }

    fn active_host_changed(&self) -> ProtocolMessage {
        ProtocolMessage::ActiveHostChanged(ActiveHostChanged {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            active_host: self.active_host().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{resolve_display_topology, MonitorInfo, Rect};
    use crate::input::{RawButtonState, RawKeyState};
    use desk_ferry_common::{
        config::load_config_str,
        protocol::{Edge, MouseButton},
    };

    const VK_CONTROL: u32 = 0x11;
    const VK_MENU: u32 = 0x12;
    const VK_SHIFT: u32 = 0x10;
    const VK_F12: u32 = 0x7B;

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
emergency_hotkey = "Ctrl+Alt+Shift+F12"
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

    fn server() -> WindowsServerIntegration {
        let config = config();
        let topology =
            resolve_display_topology(&config, vec![monitor("\\\\.\\DISPLAY1", 0, 0, 1920, 1080)])
                .expect("topology");
        WindowsServerIntegration::new(
            &config,
            topology,
            InputMode::DryRun,
            "Ctrl+Alt+Shift+F12".parse().expect("hotkey"),
        )
        .expect("server")
    }

    #[test]
    fn client_authentication_registers_host_and_keeps_server_active() {
        let mut server = server();
        let result = server.register_authenticated_client("host2");

        assert!(server.is_client_registered("host2"));
        assert_eq!(server.active_host(), "host1");
        assert!(result
            .messages
            .iter()
            .any(|message| matches!(message, ProtocolMessage::ActiveHostChanged(_))));
    }

    #[test]
    fn boundary_transition_switches_active_host_to_client() {
        let mut server = server();
        server.register_authenticated_client("host2");

        let result = server
            .handle_local_input(
                RawInputEvent::MouseMove {
                    x: 1919,
                    y: 540,
                    dx: 1,
                    dy: 0,
                },
                1_000,
            )
            .expect("transition");

        assert_eq!(server.active_host(), "host2");
        assert!(result.log_events.contains(&"boundary transition detected"));
        assert!(result
            .messages
            .iter()
            .any(|message| matches!(message, ProtocolMessage::MouseWarp(_))));
    }

    #[test]
    fn edge_without_outward_motion_does_not_switch_active_host() {
        let mut server = server();
        server.register_authenticated_client("host2");

        let result = server
            .handle_local_input(
                RawInputEvent::MouseMove {
                    x: 1919,
                    y: 540,
                    dx: 0,
                    dy: 0,
                },
                1_000,
            )
            .expect("no transition");

        assert_eq!(server.active_host(), "host1");
        assert!(result.messages.is_empty());
    }

    #[test]
    fn boundary_request_returns_active_host_to_server() {
        let mut server = server();
        server.register_authenticated_client("host2");
        server
            .handle_local_input(
                RawInputEvent::MouseMove {
                    x: 1919,
                    y: 540,
                    dx: 1,
                    dy: 0,
                },
                1_000,
            )
            .expect("transition");

        let result = server
            .handle_client_message(ProtocolMessage::BoundaryRequest(BoundaryRequest {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                from_host: "host2".to_string(),
                to_host: "host1".to_string(),
                from_edge: Edge::Left,
                to_edge: Edge::Right,
                position_ratio: 0.5,
            }))
            .expect("boundary request");

        assert_eq!(server.active_host(), "host1");
        assert!(result
            .messages
            .iter()
            .any(|message| matches!(message, ProtocolMessage::ActiveHostChanged(_))));
    }

    #[test]
    fn input_forwarding_depends_on_active_host() {
        let mut server = server();
        server.register_authenticated_client("host2");
        let local = server
            .handle_local_input(
                RawInputEvent::Key {
                    key_code: 65,
                    state: RawKeyState::Pressed,
                },
                1_000,
            )
            .expect("local key");
        assert!(local.messages.is_empty());

        server
            .handle_local_input(
                RawInputEvent::MouseMove {
                    x: 1919,
                    y: 540,
                    dx: 1,
                    dy: 0,
                },
                1_000,
            )
            .expect("transition");
        let remote = server
            .handle_local_input(
                RawInputEvent::MouseButton {
                    button: MouseButton::Left,
                    state: RawButtonState::Pressed,
                },
                1_200,
            )
            .expect("remote mouse");

        assert!(remote
            .messages
            .iter()
            .any(|message| matches!(message, ProtocolMessage::MouseButton(_))));
    }

    #[test]
    fn emergency_returns_to_server_and_generates_release_all() {
        let mut server = server();
        server.register_authenticated_client("host2");
        server
            .handle_local_input(
                RawInputEvent::MouseMove {
                    x: 1919,
                    y: 540,
                    dx: 1,
                    dy: 0,
                },
                1_000,
            )
            .expect("transition");
        for key_code in [VK_CONTROL, VK_MENU, VK_SHIFT, VK_F12] {
            server
                .handle_local_input(
                    RawInputEvent::Key {
                        key_code,
                        state: RawKeyState::Pressed,
                    },
                    1_100,
                )
                .expect("key");
        }

        assert_eq!(server.active_host(), "host1");
    }

    #[test]
    fn disconnect_returns_to_server_and_generates_release_all() {
        let mut server = server();
        server.register_authenticated_client("host2");
        server
            .handle_local_input(
                RawInputEvent::MouseMove {
                    x: 1919,
                    y: 540,
                    dx: 1,
                    dy: 0,
                },
                1_000,
            )
            .expect("transition");

        let result = server.handle_disconnect();

        assert_eq!(server.active_host(), "host1");
        assert!(result
            .messages
            .iter()
            .any(|message| matches!(message, ProtocolMessage::ReleaseAll(_))));
    }
}
