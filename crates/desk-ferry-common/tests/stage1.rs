use desk_ferry_common::{
    config::{load_config_str, parse_optional_neighbor},
    error::DeskFerryError,
    logging::protocol_message_summary,
    protocol::{
        decode_json_line, encode_json_line, ActiveHostChanged, BoundaryRequest, ButtonState, Edge,
        HelloMessage, KeyEvent, KeyState, MouseButton, MouseButtonEvent, MouseMoveEvent,
        MouseWarpEvent, ProtocolMessage, ReleaseAll, CURRENT_PROTOCOL_VERSION,
    },
};

const SERVER_CONFIG: &str = include_str!("../../../examples/server-windows.toml");
const CLIENT_CONFIG: &str = include_str!("../../../examples/client-linux-x11.toml");

#[test]
fn config_loading_accepts_stage1_examples() {
    let server = load_config_str(SERVER_CONFIG).expect("server config should load");
    assert_eq!(server.host.name, "host1");

    let client = load_config_str(CLIENT_CONFIG).expect("client config should load");
    assert_eq!(client.host.name, "host2");
}

#[test]
fn invalid_config_is_rejected() {
    let invalid = SERVER_CONFIG.replace("allow_plaintext = false", "allow_plaintext = true");
    let error = load_config_str(&invalid).expect_err("plaintext should be rejected");
    assert!(matches!(error, DeskFerryError::ConfigValidation(_)));
}

#[test]
fn neighbor_parsing_accepts_empty_and_valid_values() {
    assert!(parse_optional_neighbor("").unwrap().is_none());

    let spec = parse_optional_neighbor("host2:left")
        .unwrap()
        .expect("neighbor should be present");
    assert_eq!(spec.target_host, "host2");
    assert_eq!(spec.target_edge, Edge::Left);
}

#[test]
fn neighbor_parsing_rejects_invalid_values() {
    let error = parse_optional_neighbor("host2:diagonal").expect_err("invalid edge");
    assert!(matches!(error, DeskFerryError::InvalidNeighborSpec(_)));
}

#[test]
fn protocol_json_lines_round_trips_required_messages() {
    let messages = vec![
        ProtocolMessage::Hello(HelloMessage {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            host_name: "host1".to_string(),
            role: "server".to_string(),
        }),
        ProtocolMessage::MouseMove(MouseMoveEvent {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            dx: 10,
            dy: -4,
        }),
        ProtocolMessage::MouseWarp(MouseWarpEvent {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            x: 0.25,
            y: 0.75,
        }),
        ProtocolMessage::MouseButton(MouseButtonEvent {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            button: MouseButton::Left,
            state: ButtonState::Pressed,
        }),
        ProtocolMessage::Key(KeyEvent {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            key_code: 30,
            state: KeyState::Released,
        }),
        ProtocolMessage::BoundaryRequest(BoundaryRequest {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            from_host: "host1".to_string(),
            to_host: "host2".to_string(),
            from_edge: Edge::Right,
            to_edge: Edge::Left,
            position_ratio: 0.5,
        }),
        ProtocolMessage::ActiveHostChanged(ActiveHostChanged {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            active_host: "host2".to_string(),
        }),
        ProtocolMessage::ReleaseAll(ReleaseAll {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            reason: "disconnect".to_string(),
        }),
    ];

    for message in messages {
        let line = encode_json_line(&message).expect("message should encode");
        assert!(line.ends_with('\n'));
        let decoded = decode_json_line(&line).expect("message should decode");
        assert_eq!(decoded, message);
    }
}

#[test]
fn unknown_message_is_rejected() {
    let line = r#"{"type":"clipboard","protocol_version":1}"#;
    let error = decode_json_line(line).expect_err("unknown message should fail");
    assert!(matches!(error, DeskFerryError::JsonParse(_)));
}

#[test]
fn protocol_version_mismatch_is_rejected() {
    let line = r#"{"type":"release_all","protocol_version":999,"reason":"test"}"#;
    let error = decode_json_line(line).expect_err("version mismatch should fail");
    assert!(matches!(
        error,
        DeskFerryError::ProtocolVersionMismatch {
            expected: CURRENT_PROTOCOL_VERSION,
            actual: 999
        }
    ));
}

#[test]
fn safe_logger_does_not_output_key_input_contents() {
    let key = ProtocolMessage::Key(KeyEvent {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        key_code: 30,
        state: KeyState::Pressed,
    });

    let record = protocol_message_summary(&key);
    assert!(record.message.contains("key event"));
    assert!(!record.message.contains("30"));
    assert!(!record.message.contains("Pressed"));
    assert!(!record.message.contains("KEY_A"));
    assert!(!record.message.contains("shift"));
}
