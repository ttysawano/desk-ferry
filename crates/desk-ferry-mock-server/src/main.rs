use std::{env, fs, net::TcpListener};

use desk_ferry_common::{
    config::load_config_file,
    mock::MockServerSession,
    protocol::{MouseMoveEvent, ProtocolMessage, CURRENT_PROTOCOL_VERSION},
    security::Psk,
    transport::{accept_tls, read_message, send_message, server_config_from_pem},
    Result,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("desk-ferry-mock-server: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/server-windows.toml".to_string());
    let config = load_config_file(&config_path)?;
    let psk = Psk::from_bytes(fs::read(&config.security.psk_file)?)?;

    let cert_file = config.security.cert_file.as_deref().ok_or_else(|| {
        desk_ferry_common::DeskFerryError::ConfigValidation(
            "security.cert_file is required for mock-server".to_string(),
        )
    })?;
    let key_file = config.security.key_file.as_deref().ok_or_else(|| {
        desk_ferry_common::DeskFerryError::ConfigValidation(
            "security.key_file is required for mock-server".to_string(),
        )
    })?;
    let tls_config = server_config_from_pem(cert_file, key_file)?;

    let network = config.network.as_ref().ok_or_else(|| {
        desk_ferry_common::DeskFerryError::ConfigValidation(
            "server network is required".to_string(),
        )
    })?;
    let address = format!(
        "{}:{}",
        network.bind_address.as_deref().unwrap_or("127.0.0.1"),
        network.port.unwrap_or(24800)
    );
    let listener = TcpListener::bind(&address)?;
    println!("desk-ferry-mock-server: listening on {address} with TLS");

    for incoming in listener.incoming() {
        let tcp = incoming?;
        let mut stream = accept_tls(tcp, tls_config.clone())?;
        let mut session = MockServerSession::new(
            config.host.name.clone(),
            config.clients.clone(),
            psk.clone(),
        );

        send_message(&mut stream, &session.challenge_message())?;
        let response = read_message(&mut stream)?;
        let ProtocolMessage::AuthResponse(response) = response else {
            send_message(
                &mut stream,
                &ProtocolMessage::AuthResult(desk_ferry_common::security::auth_result_failure()),
            )?;
            continue;
        };
        let result = session.authenticate(&response);
        send_message(&mut stream, &result)?;
        if !session.is_authenticated() {
            continue;
        }

        let hello = read_message(&mut stream)?;
        session.accept_authenticated_message(&hello)?;
        println!(
            "desk-ferry-mock-server: authenticated client; protocol_version={CURRENT_PROTOCOL_VERSION}"
        );

        send_message(&mut stream, &session.active_host_changed())?;
        send_message(
            &mut stream,
            &ProtocolMessage::MouseMove(MouseMoveEvent {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                dx: 1,
                dy: 0,
            }),
        )?;
        loop {
            match read_message(&mut stream) {
                Ok(message) => {
                    session.accept_authenticated_message(&message)?;
                    if matches!(message, ProtocolMessage::BoundaryRequest(_)) {
                        send_message(&mut stream, &session.active_host_changed())?;
                        send_message(&mut stream, &session.handle_disconnect())?;
                        break;
                    }
                }
                Err(_) => {
                    let release_all = session.handle_disconnect();
                    let _ = send_message(&mut stream, &release_all);
                    break;
                }
            }
        }
    }

    Ok(())
}
