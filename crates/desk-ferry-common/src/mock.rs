use std::collections::BTreeMap;

use crate::{
    config::ClientConfig,
    error::{DeskFerryError, Result},
    protocol::{
        ActiveHostChanged, AuthChallenge, AuthResponse, HelloMessage, ProtocolMessage, ReleaseAll,
        CURRENT_PROTOCOL_VERSION,
    },
    security::{
        auth_response, auth_result_failure, auth_result_success, new_auth_challenge,
        verify_auth_response, Psk,
    },
    state::ConnectionState,
};

#[derive(Debug, Clone)]
pub struct MockServerSession {
    psk: Psk,
    clients: BTreeMap<String, ClientConfig>,
    challenge: AuthChallenge,
    state: ConnectionState,
}

impl MockServerSession {
    pub fn new(
        server_host: impl Into<String>,
        clients: BTreeMap<String, ClientConfig>,
        psk: Psk,
    ) -> Self {
        let allowed = clients
            .iter()
            .filter(|(_, client)| client.allowed)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        Self {
            psk,
            clients,
            challenge: new_auth_challenge(),
            state: ConnectionState::new(server_host, allowed),
        }
    }

    pub fn challenge_message(&self) -> ProtocolMessage {
        ProtocolMessage::AuthChallenge(self.challenge.clone())
    }

    pub fn authenticate(&mut self, response: &AuthResponse) -> ProtocolMessage {
        let result = self
            .clients
            .get(&response.host_name)
            .filter(|client| client.allowed)
            .ok_or_else(|| DeskFerryError::Authentication("host is not allowed".to_string()))
            .and_then(|_| verify_auth_response(&self.psk, &self.challenge, response))
            .and_then(|_| self.state.authenticate(&response.host_name));

        match result {
            Ok(()) => ProtocolMessage::AuthResult(auth_result_success()),
            Err(_) => ProtocolMessage::AuthResult(auth_result_failure()),
        }
    }

    pub fn accept_authenticated_message(&mut self, message: &ProtocolMessage) -> Result<()> {
        self.state.validate_inbound_message(message)?;
        match message {
            ProtocolMessage::Hello(hello) => self.accept_hello(hello),
            ProtocolMessage::BoundaryRequest(request) => {
                self.state.switch_active_host(&request.to_host)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn active_host_changed(&self) -> ProtocolMessage {
        ProtocolMessage::ActiveHostChanged(ActiveHostChanged {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            active_host: self.state.active_host().to_string(),
        })
    }

    pub fn handle_disconnect(&mut self) -> ProtocolMessage {
        ProtocolMessage::ReleaseAll(self.state.handle_disconnect())
    }

    pub fn is_authenticated(&self) -> bool {
        self.state.is_authenticated()
    }

    pub fn active_host(&self) -> &str {
        self.state.active_host()
    }

    fn accept_hello(&self, hello: &HelloMessage) -> Result<()> {
        if hello.protocol_version != CURRENT_PROTOCOL_VERSION {
            return Err(DeskFerryError::ProtocolVersionMismatch {
                expected: CURRENT_PROTOCOL_VERSION,
                actual: hello.protocol_version,
            });
        }
        if self.state.authenticated_host() != Some(hello.host_name.as_str()) {
            return Err(DeskFerryError::Authentication(
                "hello host does not match authenticated host".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MockClientSession {
    host_name: String,
    psk: Psk,
    authenticated: bool,
}

impl MockClientSession {
    pub fn new(host_name: impl Into<String>, psk: Psk) -> Self {
        Self {
            host_name: host_name.into(),
            psk,
            authenticated: false,
        }
    }

    pub fn respond_to_challenge(&self, challenge: &AuthChallenge) -> Result<ProtocolMessage> {
        Ok(ProtocolMessage::AuthResponse(auth_response(
            &self.psk,
            challenge,
            &self.host_name,
        )?))
    }

    pub fn accept_auth_result(&mut self, message: &ProtocolMessage) -> Result<()> {
        match message {
            ProtocolMessage::AuthResult(result) if result.success => {
                self.authenticated = true;
                Ok(())
            }
            ProtocolMessage::AuthResult(_) => Err(DeskFerryError::Authentication(
                "server rejected authentication".to_string(),
            )),
            _ => Err(DeskFerryError::Authentication(
                "expected auth result".to_string(),
            )),
        }
    }

    pub fn hello(&self) -> Result<ProtocolMessage> {
        if !self.authenticated {
            return Err(DeskFerryError::Authentication(
                "cannot send hello before authentication".to_string(),
            ));
        }
        Ok(ProtocolMessage::Hello(HelloMessage {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            host_name: self.host_name.clone(),
            role: "client".to_string(),
        }))
    }

    pub fn receive_message(&self, message: &ProtocolMessage) -> Result<()> {
        if !self.authenticated {
            return Err(DeskFerryError::Authentication(
                "cannot receive input before authentication".to_string(),
            ));
        }
        match message {
            ProtocolMessage::MouseMove(_)
            | ProtocolMessage::MouseWarp(_)
            | ProtocolMessage::MouseButton(_)
            | ProtocolMessage::Key(_)
            | ProtocolMessage::BoundaryRequest(_)
            | ProtocolMessage::ActiveHostChanged(_)
            | ProtocolMessage::ReleaseAll(_) => Ok(()),
            _ => Err(DeskFerryError::Authentication(
                "unexpected post-authentication message".to_string(),
            )),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }
}

pub fn release_all_message(reason: impl Into<String>) -> ProtocolMessage {
    ProtocolMessage::ReleaseAll(ReleaseAll {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        reason: reason.into(),
    })
}
