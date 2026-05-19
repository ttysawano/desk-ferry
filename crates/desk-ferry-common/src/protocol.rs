use serde::{Deserialize, Serialize};

use crate::error::{DeskFerryError, Result};

pub const CURRENT_PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloMessage {
    pub protocol_version: u16,
    pub host_name: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthChallenge {
    pub protocol_version: u16,
    pub nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthResponse {
    pub protocol_version: u16,
    pub host_name: String,
    pub hmac: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthResult {
    pub protocol_version: u16,
    pub success: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseMoveEvent {
    pub protocol_version: u16,
    pub dx: i32,
    pub dy: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MouseWarpEvent {
    pub protocol_version: u16,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseButtonEvent {
    pub protocol_version: u16,
    pub button: MouseButton,
    pub state: ButtonState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEvent {
    pub protocol_version: u16,
    pub key_code: u32,
    pub state: KeyState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryRequest {
    pub protocol_version: u16,
    pub from_host: String,
    pub to_host: String,
    pub from_edge: Edge,
    pub to_edge: Edge,
    pub position_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveHostChanged {
    pub protocol_version: u16,
    pub active_host: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAll {
    pub protocol_version: u16,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolMessage {
    AuthChallenge(AuthChallenge),
    AuthResponse(AuthResponse),
    AuthResult(AuthResult),
    Hello(HelloMessage),
    MouseMove(MouseMoveEvent),
    MouseWarp(MouseWarpEvent),
    MouseButton(MouseButtonEvent),
    Key(KeyEvent),
    BoundaryRequest(BoundaryRequest),
    ActiveHostChanged(ActiveHostChanged),
    ReleaseAll(ReleaseAll),
}

impl ProtocolMessage {
    pub fn protocol_version(&self) -> u16 {
        match self {
            Self::AuthChallenge(message) => message.protocol_version,
            Self::AuthResponse(message) => message.protocol_version,
            Self::AuthResult(message) => message.protocol_version,
            Self::Hello(message) => message.protocol_version,
            Self::MouseMove(message) => message.protocol_version,
            Self::MouseWarp(message) => message.protocol_version,
            Self::MouseButton(message) => message.protocol_version,
            Self::Key(message) => message.protocol_version,
            Self::BoundaryRequest(message) => message.protocol_version,
            Self::ActiveHostChanged(message) => message.protocol_version,
            Self::ReleaseAll(message) => message.protocol_version,
        }
    }
}

pub fn encode_json_line(message: &ProtocolMessage) -> Result<String> {
    let mut line = serde_json::to_string(message)?;
    line.push('\n');
    Ok(line)
}

pub fn decode_json_line(line: &str) -> Result<ProtocolMessage> {
    let trimmed = line.strip_suffix('\n').unwrap_or(line);
    let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);
    if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(DeskFerryError::InvalidJsonLine);
    }

    let message: ProtocolMessage = serde_json::from_str(trimmed)?;
    let actual = message.protocol_version();
    if actual != CURRENT_PROTOCOL_VERSION {
        return Err(DeskFerryError::ProtocolVersionMismatch {
            expected: CURRENT_PROTOCOL_VERSION,
            actual,
        });
    }
    Ok(message)
}

pub fn is_input_event(message: &ProtocolMessage) -> bool {
    matches!(
        message,
        ProtocolMessage::MouseMove(_)
            | ProtocolMessage::MouseWarp(_)
            | ProtocolMessage::MouseButton(_)
            | ProtocolMessage::Key(_)
            | ProtocolMessage::BoundaryRequest(_)
    )
}
