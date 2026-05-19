use std::collections::BTreeSet;

use desk_ferry_common::protocol::{
    ActiveHostChanged, ButtonState, KeyEvent, KeyState, MouseButton, MouseButtonEvent,
    MouseMoveEvent, ProtocolMessage, ReleaseAll, CURRENT_PROTOCOL_VERSION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    DryRun,
    Suppress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawButtonState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKeyState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawInputEvent {
    MouseMove {
        dx: i32,
        dy: i32,
    },
    MouseButton {
        button: MouseButton,
        state: RawButtonState,
    },
    MouseWheel {
        delta: i32,
    },
    Key {
        key_code: u32,
        state: RawKeyState,
    },
    Disconnect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputActionKind {
    MouseMoveDetected,
    MouseButtonDetected,
    MouseWheelDetected,
    KeyDetected,
    EmergencyHotkeyDetected,
    ActiveHostChanged,
    ReleaseAllGenerated,
    ConnectionDisconnected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputAction {
    pub kind: InputActionKind,
    pub message: Option<ProtocolMessage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputProcessResult {
    pub suppress_input: bool,
    pub actions: Vec<InputAction>,
}

#[derive(Debug, Clone)]
pub struct InputEngine {
    mode: InputMode,
    server_host: String,
    active_host: String,
    pressed_keys: BTreeSet<u32>,
    pressed_mouse_buttons: Vec<MouseButton>,
}

const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_MENU: u32 = 0x12;
const VK_ESCAPE: u32 = 0x1B;
const VK_LSHIFT: u32 = 0xA0;
const VK_RSHIFT: u32 = 0xA1;
const VK_LCONTROL: u32 = 0xA2;
const VK_RCONTROL: u32 = 0xA3;
const VK_LMENU: u32 = 0xA4;
const VK_RMENU: u32 = 0xA5;

impl InputEngine {
    pub fn new(server_host: impl Into<String>, mode: InputMode) -> Self {
        let server_host = server_host.into();
        Self {
            mode,
            active_host: server_host.clone(),
            server_host,
            pressed_keys: BTreeSet::new(),
            pressed_mouse_buttons: Vec::new(),
        }
    }

    pub fn mode(&self) -> InputMode {
        self.mode
    }

    pub fn active_host(&self) -> &str {
        &self.active_host
    }

    pub fn set_active_host(&mut self, host: impl Into<String>) -> InputProcessResult {
        self.active_host = host.into();
        InputProcessResult {
            suppress_input: false,
            actions: vec![InputAction {
                kind: InputActionKind::ActiveHostChanged,
                message: Some(ProtocolMessage::ActiveHostChanged(ActiveHostChanged {
                    protocol_version: CURRENT_PROTOCOL_VERSION,
                    active_host: self.active_host.clone(),
                })),
            }],
        }
    }

    pub fn handle_event(&mut self, event: RawInputEvent) -> InputProcessResult {
        match event {
            RawInputEvent::MouseMove { dx, dy } => self.handle_mouse_move(dx, dy),
            RawInputEvent::MouseButton { button, state } => self.handle_mouse_button(button, state),
            RawInputEvent::MouseWheel { delta } => self.handle_mouse_wheel(delta),
            RawInputEvent::Key { key_code, state } => self.handle_key(key_code, state),
            RawInputEvent::Disconnect => self.handle_disconnect(),
        }
    }

    pub fn pressed_key_count(&self) -> usize {
        self.pressed_keys.len()
    }

    pub fn pressed_mouse_button_count(&self) -> usize {
        self.pressed_mouse_buttons.len()
    }

    pub fn should_suppress_for_active_host(&self) -> bool {
        self.mode == InputMode::Suppress && self.active_host != self.server_host
    }

    fn handle_mouse_move(&mut self, dx: i32, dy: i32) -> InputProcessResult {
        let mut actions = vec![InputAction {
            kind: InputActionKind::MouseMoveDetected,
            message: None,
        }];
        if self.active_host != self.server_host {
            actions[0].message = Some(ProtocolMessage::MouseMove(MouseMoveEvent {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                dx,
                dy,
            }));
        }
        InputProcessResult {
            suppress_input: self.should_suppress_for_active_host(),
            actions,
        }
    }

    fn handle_mouse_button(
        &mut self,
        button: MouseButton,
        state: RawButtonState,
    ) -> InputProcessResult {
        match state {
            RawButtonState::Pressed => {
                if !self.pressed_mouse_buttons.contains(&button) {
                    self.pressed_mouse_buttons.push(button);
                }
            }
            RawButtonState::Released => {
                self.pressed_mouse_buttons
                    .retain(|pressed| *pressed != button);
            }
        }

        let mut actions = vec![InputAction {
            kind: InputActionKind::MouseButtonDetected,
            message: None,
        }];
        if self.active_host != self.server_host {
            actions[0].message = Some(ProtocolMessage::MouseButton(MouseButtonEvent {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                button,
                state: match state {
                    RawButtonState::Pressed => ButtonState::Pressed,
                    RawButtonState::Released => ButtonState::Released,
                },
            }));
        }
        InputProcessResult {
            suppress_input: self.should_suppress_for_active_host(),
            actions,
        }
    }

    fn handle_mouse_wheel(&mut self, _delta: i32) -> InputProcessResult {
        InputProcessResult {
            suppress_input: self.should_suppress_for_active_host(),
            actions: vec![InputAction {
                kind: InputActionKind::MouseWheelDetected,
                message: None,
            }],
        }
    }

    fn handle_key(&mut self, key_code: u32, state: RawKeyState) -> InputProcessResult {
        match state {
            RawKeyState::Pressed => {
                self.pressed_keys.insert(key_code);
            }
            RawKeyState::Released => {
                self.pressed_keys.remove(&key_code);
            }
        }

        if self.is_emergency_hotkey(key_code, state) {
            return self.handle_emergency();
        }

        let mut actions = vec![InputAction {
            kind: InputActionKind::KeyDetected,
            message: None,
        }];
        if self.active_host != self.server_host {
            actions[0].message = Some(ProtocolMessage::Key(KeyEvent {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                key_code,
                state: match state {
                    RawKeyState::Pressed => KeyState::Pressed,
                    RawKeyState::Released => KeyState::Released,
                },
            }));
        }

        InputProcessResult {
            suppress_input: self.should_suppress_for_active_host(),
            actions,
        }
    }

    fn handle_emergency(&mut self) -> InputProcessResult {
        self.active_host = self.server_host.clone();
        self.pressed_keys.clear();
        self.pressed_mouse_buttons.clear();
        InputProcessResult {
            suppress_input: false,
            actions: vec![
                InputAction {
                    kind: InputActionKind::EmergencyHotkeyDetected,
                    message: None,
                },
                InputAction {
                    kind: InputActionKind::ActiveHostChanged,
                    message: Some(ProtocolMessage::ActiveHostChanged(ActiveHostChanged {
                        protocol_version: CURRENT_PROTOCOL_VERSION,
                        active_host: self.server_host.clone(),
                    })),
                },
                InputAction {
                    kind: InputActionKind::ReleaseAllGenerated,
                    message: Some(release_all("emergency")),
                },
            ],
        }
    }

    fn handle_disconnect(&mut self) -> InputProcessResult {
        self.active_host = self.server_host.clone();
        self.pressed_keys.clear();
        self.pressed_mouse_buttons.clear();
        InputProcessResult {
            suppress_input: false,
            actions: vec![
                InputAction {
                    kind: InputActionKind::ConnectionDisconnected,
                    message: None,
                },
                InputAction {
                    kind: InputActionKind::ActiveHostChanged,
                    message: Some(ProtocolMessage::ActiveHostChanged(ActiveHostChanged {
                        protocol_version: CURRENT_PROTOCOL_VERSION,
                        active_host: self.server_host.clone(),
                    })),
                },
                InputAction {
                    kind: InputActionKind::ReleaseAllGenerated,
                    message: Some(release_all("disconnect")),
                },
            ],
        }
    }

    fn is_emergency_hotkey(&self, key_code: u32, state: RawKeyState) -> bool {
        state == RawKeyState::Pressed
            && key_code == VK_ESCAPE
            && self.any_pressed([VK_CONTROL, VK_LCONTROL, VK_RCONTROL])
            && self.any_pressed([VK_MENU, VK_LMENU, VK_RMENU])
            && self.any_pressed([VK_SHIFT, VK_LSHIFT, VK_RSHIFT])
    }

    fn any_pressed<const N: usize>(&self, keys: [u32; N]) -> bool {
        keys.iter().any(|key| self.pressed_keys.contains(key))
    }
}

pub fn release_all(reason: impl Into<String>) -> ProtocolMessage {
    ProtocolMessage::ReleaseAll(ReleaseAll {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        reason: reason.into(),
    })
}

pub fn safe_action_summary(action: &InputAction) -> &'static str {
    match action.kind {
        InputActionKind::MouseMoveDetected => "mouse move event detected",
        InputActionKind::MouseButtonDetected => "mouse button event detected",
        InputActionKind::MouseWheelDetected => "mouse wheel event detected",
        InputActionKind::KeyDetected => "key event detected",
        InputActionKind::EmergencyHotkeyDetected => "emergency hotkey detected",
        InputActionKind::ActiveHostChanged => "active_host changed",
        InputActionKind::ReleaseAllGenerated => "release_all generated",
        InputActionKind::ConnectionDisconnected => "connection disconnected",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_engine(mode: InputMode) -> InputEngine {
        let mut engine = InputEngine::new("host1", mode);
        engine.set_active_host("host2");
        engine
    }

    #[test]
    fn mouse_event_is_converted_when_client_is_active() {
        let mut engine = client_engine(InputMode::DryRun);
        let result = engine.handle_event(RawInputEvent::MouseMove { dx: 4, dy: -2 });

        assert!(!result.suppress_input);
        assert!(matches!(
            result.actions[0].message,
            Some(ProtocolMessage::MouseMove(_))
        ));
    }

    #[test]
    fn mouse_button_state_is_tracked_and_converted() {
        let mut engine = client_engine(InputMode::DryRun);
        let down = engine.handle_event(RawInputEvent::MouseButton {
            button: MouseButton::Left,
            state: RawButtonState::Pressed,
        });
        let up = engine.handle_event(RawInputEvent::MouseButton {
            button: MouseButton::Left,
            state: RawButtonState::Released,
        });

        assert_eq!(engine.pressed_mouse_button_count(), 0);
        assert!(matches!(
            down.actions[0].message,
            Some(ProtocolMessage::MouseButton(_))
        ));
        assert!(matches!(
            up.actions[0].message,
            Some(ProtocolMessage::MouseButton(_))
        ));
    }

    #[test]
    fn key_event_is_converted_and_state_is_tracked() {
        let mut engine = client_engine(InputMode::DryRun);
        let result = engine.handle_event(RawInputEvent::Key {
            key_code: 65,
            state: RawKeyState::Pressed,
        });

        assert_eq!(engine.pressed_key_count(), 1);
        assert!(matches!(
            result.actions[0].message,
            Some(ProtocolMessage::Key(_))
        ));
    }

    #[test]
    fn mouse_wheel_is_detected_as_internal_event() {
        let mut engine = client_engine(InputMode::DryRun);
        let result = engine.handle_event(RawInputEvent::MouseWheel { delta: 120 });

        assert_eq!(result.actions[0].kind, InputActionKind::MouseWheelDetected);
        assert!(result.actions[0].message.is_none());
    }

    #[test]
    fn server_active_host_does_not_send_input_messages() {
        let mut engine = InputEngine::new("host1", InputMode::DryRun);
        let result = engine.handle_event(RawInputEvent::Key {
            key_code: 65,
            state: RawKeyState::Pressed,
        });

        assert!(result.actions[0].message.is_none());
        assert!(!result.suppress_input);
    }

    #[test]
    fn dry_run_mode_never_suppresses_input() {
        let mut engine = client_engine(InputMode::DryRun);
        let result = engine.handle_event(RawInputEvent::MouseMove { dx: 1, dy: 0 });

        assert!(!result.suppress_input);
    }

    #[test]
    fn suppress_mode_requires_explicit_enablement() {
        let mut dry = client_engine(InputMode::DryRun);
        let mut suppress = client_engine(InputMode::Suppress);

        assert!(
            !dry.handle_event(RawInputEvent::MouseMove { dx: 1, dy: 0 })
                .suppress_input
        );
        assert!(
            suppress
                .handle_event(RawInputEvent::MouseMove { dx: 1, dy: 0 })
                .suppress_input
        );
    }

    #[test]
    fn emergency_hotkey_returns_to_server_and_generates_release_all() {
        let mut engine = client_engine(InputMode::Suppress);
        engine.handle_event(RawInputEvent::Key {
            key_code: VK_CONTROL,
            state: RawKeyState::Pressed,
        });
        engine.handle_event(RawInputEvent::Key {
            key_code: VK_MENU,
            state: RawKeyState::Pressed,
        });
        engine.handle_event(RawInputEvent::Key {
            key_code: VK_SHIFT,
            state: RawKeyState::Pressed,
        });
        let result = engine.handle_event(RawInputEvent::Key {
            key_code: VK_ESCAPE,
            state: RawKeyState::Pressed,
        });

        assert_eq!(engine.active_host(), "host1");
        assert_eq!(engine.pressed_key_count(), 0);
        assert_eq!(engine.pressed_mouse_button_count(), 0);
        assert!(!result.suppress_input);
        assert!(result
            .actions
            .iter()
            .any(|action| action.kind == InputActionKind::EmergencyHotkeyDetected));
        assert!(result
            .actions
            .iter()
            .any(|action| matches!(action.message, Some(ProtocolMessage::ReleaseAll(_)))));
    }

    #[test]
    fn disconnect_returns_to_server_and_generates_release_all() {
        let mut engine = client_engine(InputMode::Suppress);
        engine.handle_event(RawInputEvent::MouseButton {
            button: MouseButton::Right,
            state: RawButtonState::Pressed,
        });

        let result = engine.handle_event(RawInputEvent::Disconnect);

        assert_eq!(engine.active_host(), "host1");
        assert_eq!(engine.pressed_mouse_button_count(), 0);
        assert!(result
            .actions
            .iter()
            .any(|action| matches!(action.message, Some(ProtocolMessage::ReleaseAll(_)))));
    }

    #[test]
    fn safe_log_summary_does_not_include_key_details() {
        let action = InputAction {
            kind: InputActionKind::KeyDetected,
            message: Some(ProtocolMessage::Key(KeyEvent {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                key_code: 65,
                state: KeyState::Pressed,
            })),
        };

        let summary = safe_action_summary(&action);

        assert_eq!(summary, "key event detected");
        assert!(!summary.contains("65"));
    }
}
