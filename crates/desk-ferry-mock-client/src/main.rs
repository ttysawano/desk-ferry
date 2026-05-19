use std::{env, fs, net::TcpStream};

use desk_ferry_common::{
    config::load_config_file,
    mock::MockClientSession,
    protocol::{BoundaryRequest, Edge, ProtocolMessage, CURRENT_PROTOCOL_VERSION},
    security::Psk,
    transport::{connect_tls, read_message, send_message},
    Result,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("desk-ferry-mock-client: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/client-linux-x11.toml".to_string());
    let config = load_config_file(&config_path)?;
    let server = config.server.as_ref().ok_or_else(|| {
        desk_ferry_common::DeskFerryError::ConfigValidation(
            "client server section is required".to_string(),
        )
    })?;
    let fingerprint = config
        .security
        .server_fingerprint
        .as_deref()
        .ok_or_else(|| {
            desk_ferry_common::DeskFerryError::ConfigValidation(
                "security.server_fingerprint is required for mock-client".to_string(),
            )
        })?;
    let psk = Psk::from_bytes(fs::read(&config.security.psk_file)?)?;

    let tcp = TcpStream::connect((server.host.as_str(), server.port))?;
    let mut stream = connect_tls(tcp, &server.host, fingerprint)?;
    let mut session = MockClientSession::new(config.host.name.clone(), psk);

    let challenge = read_message(&mut stream)?;
    let ProtocolMessage::AuthChallenge(challenge) = challenge else {
        return Err(desk_ferry_common::DeskFerryError::Authentication(
            "expected auth challenge".to_string(),
        ));
    };
    let response = session.respond_to_challenge(&challenge)?;
    send_message(&mut stream, &response)?;

    let result = read_message(&mut stream)?;
    session.accept_auth_result(&result)?;
    send_message(&mut stream, &session.hello()?)?;
    println!("desk-ferry-mock-client: authenticated; protocol_version={CURRENT_PROTOCOL_VERSION}");

    if let Some(neighbor) =
        desk_ferry_common::config::parse_optional_neighbor(&config.neighbors.left)?
    {
        send_message(
            &mut stream,
            &ProtocolMessage::BoundaryRequest(BoundaryRequest {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                from_host: config.host.name.clone(),
                to_host: neighbor.target_host,
                from_edge: Edge::Left,
                to_edge: neighbor.target_edge,
                position_ratio: 0.5,
            }),
        )?;
    }

    loop {
        let message = read_message(&mut stream)?;
        session.receive_message(&message)?;
        if matches!(message, ProtocolMessage::ReleaseAll(_)) {
            break;
        }
    }

    Ok(())
}
