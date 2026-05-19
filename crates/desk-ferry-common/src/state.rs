use std::collections::BTreeSet;

use crate::{
    error::{DeskFerryError, Result},
    protocol::{is_input_event, ProtocolMessage, ReleaseAll, CURRENT_PROTOCOL_VERSION},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionState {
    server_host: String,
    allowed_hosts: BTreeSet<String>,
    active_host: String,
    authenticated_host: Option<String>,
}

impl ConnectionState {
    pub fn new(
        server_host: impl Into<String>,
        allowed_hosts: impl IntoIterator<Item = String>,
    ) -> Self {
        let server_host = server_host.into();
        Self {
            active_host: server_host.clone(),
            server_host,
            allowed_hosts: allowed_hosts.into_iter().collect(),
            authenticated_host: None,
        }
    }

    pub fn active_host(&self) -> &str {
        &self.active_host
    }

    pub fn authenticated_host(&self) -> Option<&str> {
        self.authenticated_host.as_deref()
    }

    pub fn is_authenticated(&self) -> bool {
        self.authenticated_host.is_some()
    }

    pub fn authenticate(&mut self, host_name: &str) -> Result<()> {
        if !self.allowed_hosts.contains(host_name) {
            return Err(DeskFerryError::Authentication(
                "host is not allowed".to_string(),
            ));
        }
        self.authenticated_host = Some(host_name.to_string());
        Ok(())
    }

    pub fn switch_active_host(&mut self, host_name: &str) -> Result<()> {
        if host_name != self.server_host && !self.allowed_hosts.contains(host_name) {
            return Err(DeskFerryError::Authentication(
                "active host is not allowed".to_string(),
            ));
        }
        self.active_host = host_name.to_string();
        Ok(())
    }

    pub fn handle_disconnect(&mut self) -> ReleaseAll {
        self.authenticated_host = None;
        self.active_host = self.server_host.clone();
        ReleaseAll {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            reason: "disconnect".to_string(),
        }
    }

    pub fn validate_inbound_message(&self, message: &ProtocolMessage) -> Result<()> {
        if is_input_event(message) && !self.is_authenticated() {
            return Err(DeskFerryError::Authentication(
                "input event received before authentication".to_string(),
            ));
        }
        Ok(())
    }
}
