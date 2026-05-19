use crate::{
    error::Result,
    protocol::{decode_json_line, ProtocolMessage, CURRENT_PROTOCOL_VERSION},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafeLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeLogRecord {
    pub level: SafeLogLevel,
    pub message: String,
}

impl SafeLogRecord {
    pub fn new(level: SafeLogLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
        }
    }
}

pub fn protocol_message_summary(message: &ProtocolMessage) -> SafeLogRecord {
    let summary = match message {
        ProtocolMessage::AuthChallenge(_) => "auth challenge",
        ProtocolMessage::AuthResponse(_) => "auth response",
        ProtocolMessage::AuthResult(message) if message.success => "auth result success",
        ProtocolMessage::AuthResult(_) => "auth result failure",
        ProtocolMessage::Hello(_) => "protocol hello",
        ProtocolMessage::MouseMove(_) => "mouse move event",
        ProtocolMessage::MouseWarp(_) => "mouse warp event",
        ProtocolMessage::MouseButton(_) => "mouse button event",
        ProtocolMessage::Key(_) => "key event",
        ProtocolMessage::BoundaryRequest(_) => "boundary request",
        ProtocolMessage::ActiveHostChanged(_) => "active host changed",
        ProtocolMessage::ReleaseAll(_) => "release all",
    };

    SafeLogRecord::new(
        SafeLogLevel::Info,
        format!("{summary}; protocol_version={CURRENT_PROTOCOL_VERSION}"),
    )
}

pub fn redact_secret(value: &str) -> String {
    if value.is_empty() {
        "<empty>".to_string()
    } else {
        "<redacted>".to_string()
    }
}

pub fn protocol_dump_summary(line: &str) -> Result<SafeLogRecord> {
    let message = decode_json_line(line)?;
    Ok(protocol_message_summary(&message))
}
