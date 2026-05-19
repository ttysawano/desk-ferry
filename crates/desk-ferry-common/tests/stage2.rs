use std::collections::BTreeMap;

use desk_ferry_common::{
    config::ClientConfig,
    error::DeskFerryError,
    logging::{protocol_dump_summary, protocol_message_summary, redact_secret},
    mock::{MockClientSession, MockServerSession},
    protocol::{
        decode_json_line, encode_json_line, ActiveHostChanged, AuthChallenge, AuthResponse,
        AuthResult, BoundaryRequest, ButtonState, Edge, HelloMessage, KeyEvent, KeyState,
        MouseButton, MouseButtonEvent, MouseMoveEvent, MouseWarpEvent, ProtocolMessage, ReleaseAll,
        CURRENT_PROTOCOL_VERSION,
    },
    security::{
        auth_response, certificate_fingerprint_sha256, compute_psk_hmac, generate_nonce,
        new_auth_challenge, normalize_fingerprint, verify_fingerprint, verify_psk_hmac, Psk,
    },
    state::ConnectionState,
};

fn psk() -> Psk {
    Psk::from_bytes(b"stage2-test-psk").unwrap()
}

fn wrong_psk() -> Psk {
    Psk::from_bytes(b"wrong-stage2-test-psk").unwrap()
}

fn allowed_clients() -> BTreeMap<String, ClientConfig> {
    BTreeMap::from([(
        "host2".to_string(),
        ClientConfig {
            name: "host2".to_string(),
            allowed: true,
            expected_fingerprint: String::new(),
        },
    )])
}

#[test]
fn auth_succeeds_with_correct_psk() {
    let mut server = MockServerSession::new("host1", allowed_clients(), psk());
    let ProtocolMessage::AuthChallenge(challenge) = server.challenge_message() else {
        panic!("challenge expected");
    };
    let response = auth_response(&psk(), &challenge, "host2").unwrap();
    let result = server.authenticate(&response);

    assert!(matches!(
        result,
        ProtocolMessage::AuthResult(AuthResult { success: true, .. })
    ));
    assert!(server.is_authenticated());
}

#[test]
fn auth_fails_with_wrong_psk() {
    let mut server = MockServerSession::new("host1", allowed_clients(), psk());
    let ProtocolMessage::AuthChallenge(challenge) = server.challenge_message() else {
        panic!("challenge expected");
    };
    let response = auth_response(&wrong_psk(), &challenge, "host2").unwrap();
    let result = server.authenticate(&response);

    assert!(matches!(
        result,
        ProtocolMessage::AuthResult(AuthResult { success: false, .. })
    ));
    assert!(!server.is_authenticated());
}

#[test]
fn auth_fails_for_unallowed_host_name() {
    let mut server = MockServerSession::new("host1", allowed_clients(), psk());
    let ProtocolMessage::AuthChallenge(challenge) = server.challenge_message() else {
        panic!("challenge expected");
    };
    let response = auth_response(&psk(), &challenge, "host3").unwrap();
    let result = server.authenticate(&response);

    assert!(matches!(
        result,
        ProtocolMessage::AuthResult(AuthResult { success: false, .. })
    ));
}

#[test]
fn input_before_authentication_is_rejected() {
    let state = ConnectionState::new("host1", vec!["host2".to_string()]);
    let event = ProtocolMessage::MouseMove(MouseMoveEvent {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        dx: 1,
        dy: 1,
    });
    let error = state.validate_inbound_message(&event).unwrap_err();
    assert!(matches!(error, DeskFerryError::Authentication(_)));
}

#[test]
fn nonce_changes_each_time() {
    let first = generate_nonce();
    let second = generate_nonce();
    assert_ne!(first, second);
}

#[test]
fn hmac_verification_failure_is_rejected() {
    let nonce = generate_nonce();
    let hmac = compute_psk_hmac(&psk(), &nonce, "host2").unwrap();
    let error = verify_psk_hmac(&wrong_psk(), &nonce, "host2", &hmac).unwrap_err();
    assert!(matches!(error, DeskFerryError::Authentication(_)));
}

#[test]
fn fingerprint_can_be_verified_and_mismatch_is_rejected() {
    let cert = b"fake-der-for-fingerprint-test";
    let fingerprint = certificate_fingerprint_sha256(cert);
    let coloned = fingerprint
        .as_bytes()
        .chunks(2)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join(":");

    verify_fingerprint(cert, &coloned).unwrap();
    assert_eq!(normalize_fingerprint(&coloned), fingerprint);
    assert!(matches!(
        verify_fingerprint(cert, "00"),
        Err(DeskFerryError::SecurityPolicy(_))
    ));
}

#[test]
fn plaintext_normal_mode_is_disabled_by_config_policy() {
    let text = include_str!("../../../examples/server-windows.toml")
        .replace("allow_plaintext = false", "allow_plaintext = true");
    assert!(matches!(
        desk_ferry_common::config::load_config_str(&text),
        Err(DeskFerryError::ConfigValidation(_))
    ));
}

#[test]
fn auth_messages_round_trip() {
    let messages = vec![
        ProtocolMessage::AuthChallenge(AuthChallenge {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            nonce: "abcdef".to_string(),
        }),
        ProtocolMessage::AuthResponse(AuthResponse {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            host_name: "host2".to_string(),
            hmac: "00ff".to_string(),
        }),
        ProtocolMessage::AuthResult(AuthResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            success: true,
            reason: "authenticated".to_string(),
        }),
    ];

    for message in messages {
        let line = encode_json_line(&message).unwrap();
        assert_eq!(decode_json_line(&line).unwrap(), message);
    }
}

#[test]
fn post_auth_protocol_messages_round_trip() {
    let messages = vec![
        ProtocolMessage::Hello(HelloMessage {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            host_name: "host2".to_string(),
            role: "client".to_string(),
        }),
        ProtocolMessage::MouseMove(MouseMoveEvent {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            dx: 5,
            dy: -3,
        }),
        ProtocolMessage::MouseWarp(MouseWarpEvent {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            x: 0.5,
            y: 0.25,
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
            from_host: "host2".to_string(),
            to_host: "host1".to_string(),
            from_edge: Edge::Left,
            to_edge: Edge::Right,
            position_ratio: 0.4,
        }),
        ProtocolMessage::ActiveHostChanged(ActiveHostChanged {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            active_host: "host1".to_string(),
        }),
        ProtocolMessage::ReleaseAll(ReleaseAll {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            reason: "disconnect".to_string(),
        }),
    ];

    for message in messages {
        let line = encode_json_line(&message).unwrap();
        assert_eq!(decode_json_line(&line).unwrap(), message);
    }
}

#[test]
fn protocol_version_mismatch_still_rejected() {
    let line = r#"{"type":"auth_result","protocol_version":999,"success":true,"reason":"x"}"#;
    assert!(matches!(
        decode_json_line(line),
        Err(DeskFerryError::ProtocolVersionMismatch { .. })
    ));
}

#[test]
fn authenticated_state_and_active_host_are_managed() {
    let mut server = MockServerSession::new("host1", allowed_clients(), psk());
    let ProtocolMessage::AuthChallenge(challenge) = server.challenge_message() else {
        panic!("challenge expected");
    };
    let response = auth_response(&psk(), &challenge, "host2").unwrap();
    server.authenticate(&response);
    assert!(server.is_authenticated());

    let boundary = ProtocolMessage::BoundaryRequest(BoundaryRequest {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        from_host: "host1".to_string(),
        to_host: "host2".to_string(),
        from_edge: Edge::Right,
        to_edge: Edge::Left,
        position_ratio: 0.5,
    });
    server.accept_authenticated_message(&boundary).unwrap();
    assert_eq!(server.active_host(), "host2");

    let release = server.handle_disconnect();
    assert_eq!(server.active_host(), "host1");
    assert!(!server.is_authenticated());
    assert!(matches!(release, ProtocolMessage::ReleaseAll(_)));
}

#[test]
fn mock_client_authentication_flow_allows_release_all() {
    let mut client = MockClientSession::new("host2", psk());
    let challenge = new_auth_challenge();
    let response = client.respond_to_challenge(&challenge).unwrap();
    assert!(matches!(response, ProtocolMessage::AuthResponse(_)));
    client
        .accept_auth_result(&ProtocolMessage::AuthResult(AuthResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            success: true,
            reason: "authenticated".to_string(),
        }))
        .unwrap();
    assert!(client.is_authenticated());
    client.hello().unwrap();
    client
        .receive_message(&ProtocolMessage::MouseMove(MouseMoveEvent {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            dx: 1,
            dy: 0,
        }))
        .unwrap();
    client
        .receive_message(&ProtocolMessage::ReleaseAll(ReleaseAll {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            reason: "disconnect".to_string(),
        }))
        .unwrap();
}

#[test]
fn safe_logs_do_not_expose_psk_hmac_or_key_details() {
    let secret = "actual-test-psk";
    let hmac = compute_psk_hmac(&psk(), b"nonce", "host2").unwrap();
    assert_eq!(redact_secret(secret), "<redacted>");

    let auth = ProtocolMessage::AuthResponse(AuthResponse {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        host_name: "host2".to_string(),
        hmac: hmac.clone(),
    });
    let auth_log = protocol_message_summary(&auth).message;
    assert!(!auth_log.contains(secret));
    assert!(!auth_log.contains(&hmac));

    let key = ProtocolMessage::Key(KeyEvent {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        key_code: 30,
        state: KeyState::Pressed,
    });
    let key_log = protocol_message_summary(&key).message;
    assert!(!key_log.contains("30"));
    assert!(!key_log.contains("Pressed"));
    assert!(!key_log.contains("KEY_A"));
    assert!(!key_log.contains("shift"));

    let dump = protocol_dump_summary(&encode_json_line(&key).unwrap())
        .unwrap()
        .message;
    assert!(dump.contains("key event"));
    assert!(!dump.contains("30"));
}
