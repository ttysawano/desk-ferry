use std::{collections::BTreeMap, fs, path::Path, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{
    error::{DeskFerryError, Result},
    protocol::Edge,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostRole {
    Server,
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostOs {
    Windows,
    LinuxX11,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecurityMode {
    Tls,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostConfig {
    pub name: String,
    pub role: HostRole,
    pub os: HostOs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub bind_address: Option<String>,
    pub port: Option<u16>,
    pub reconnect_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub mode: SecurityMode,
    pub psk_file: String,
    pub allow_plaintext: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionConfig {
    pub entry_margin_px: u32,
    pub cooldown_ms: u64,
    pub require_double_push: bool,
    pub edge_activation_delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputConfig {
    pub emergency_hotkey: Option<String>,
    pub keyboard_mode: String,
    pub ime_sync: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsDisplayConfig {
    pub alias: String,
    pub match_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsConfig {
    pub use_virtual_screen: bool,
    #[serde(default)]
    pub displays: Vec<WindowsDisplayConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub allow_multi_display: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientConfig {
    pub name: String,
    pub allowed: bool,
    #[serde(default)]
    pub expected_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NeighborMap {
    #[serde(default)]
    pub left: String,
    #[serde(default)]
    pub right: String,
    #[serde(default)]
    pub up: String,
    #[serde(default)]
    pub down: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub host: HostConfig,
    pub security: SecurityConfig,
    pub transition: TransitionConfig,
    pub input: InputConfig,
    #[serde(default)]
    pub network: Option<NetworkConfig>,
    #[serde(default)]
    pub server: Option<ServerConfig>,
    #[serde(default)]
    pub display: Option<DisplayConfig>,
    #[serde(default)]
    pub windows: Option<WindowsConfig>,
    #[serde(default)]
    pub neighbors: NeighborMap,
    #[serde(default)]
    pub display_neighbors: BTreeMap<String, NeighborMap>,
    #[serde(default)]
    pub clients: BTreeMap<String, ClientConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeighborSpec {
    pub target_host: String,
    pub target_edge: Edge,
}

impl FromStr for NeighborSpec {
    type Err = DeskFerryError;

    fn from_str(value: &str) -> Result<Self> {
        let (target_host, target_edge) = value
            .split_once(':')
            .ok_or_else(|| DeskFerryError::InvalidNeighborSpec(value.to_string()))?;

        if target_host.trim().is_empty() || target_host != target_host.trim() {
            return Err(DeskFerryError::InvalidNeighborSpec(value.to_string()));
        }

        let target_edge = match target_edge {
            "left" => Edge::Left,
            "right" => Edge::Right,
            "up" => Edge::Up,
            "down" => Edge::Down,
            _ => return Err(DeskFerryError::InvalidNeighborSpec(value.to_string())),
        };

        Ok(Self {
            target_host: target_host.to_string(),
            target_edge,
        })
    }
}

pub fn parse_optional_neighbor(value: &str) -> Result<Option<NeighborSpec>> {
    if value.is_empty() {
        Ok(None)
    } else {
        value.parse().map(Some)
    }
}

pub fn load_config_file(path: impl AsRef<Path>) -> Result<AppConfig> {
    let text = fs::read_to_string(path)
        .map_err(|error| DeskFerryError::ConfigValidation(error.to_string()))?;
    load_config_str(&text)
}

pub fn load_config_str(text: &str) -> Result<AppConfig> {
    let config: AppConfig = toml::from_str(text)?;
    validate_config(&config)?;
    Ok(config)
}

pub fn validate_config(config: &AppConfig) -> Result<()> {
    if config.host.name.trim().is_empty() {
        return invalid("host.name must not be empty");
    }
    if config.security.psk_file.trim().is_empty() {
        return invalid("security.psk_file must not be empty");
    }
    if config.security.allow_plaintext {
        return invalid("security.allow_plaintext must be false");
    }
    if config.input.keyboard_mode != "physical_key" {
        return invalid("input.keyboard_mode must be physical_key");
    }
    if config.input.ime_sync {
        return invalid("input.ime_sync must be false in Stage 1");
    }

    validate_neighbor_map(&config.neighbors)?;
    for neighbors in config.display_neighbors.values() {
        validate_neighbor_map(neighbors)?;
    }

    match (&config.host.role, &config.host.os) {
        (HostRole::Server, HostOs::Windows) => validate_windows_server(config),
        (HostRole::Client, HostOs::LinuxX11) => validate_linux_x11_client(config),
        (HostRole::Server, _) => invalid("server role is only supported with os = windows"),
        (HostRole::Client, _) => invalid("client role is only supported with os = linux-x11"),
    }
}

fn validate_windows_server(config: &AppConfig) -> Result<()> {
    let network = config
        .network
        .as_ref()
        .ok_or_else(|| DeskFerryError::ConfigValidation("server network is required".into()))?;
    if network.port.unwrap_or(0) == 0 {
        return invalid("server network.port must be set");
    }
    if network
        .bind_address
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return invalid("server network.bind_address must be set");
    }
    let windows = config
        .windows
        .as_ref()
        .ok_or_else(|| DeskFerryError::ConfigValidation("windows section is required".into()))?;
    if windows.displays.is_empty() {
        return invalid("windows.displays must not be empty");
    }
    Ok(())
}

fn validate_linux_x11_client(config: &AppConfig) -> Result<()> {
    let server = config
        .server
        .as_ref()
        .ok_or_else(|| DeskFerryError::ConfigValidation("server section is required".into()))?;
    if server.host.trim().is_empty() {
        return invalid("server.host must not be empty");
    }
    if server.port == 0 {
        return invalid("server.port must be set");
    }
    let display = config
        .display
        .as_ref()
        .ok_or_else(|| DeskFerryError::ConfigValidation("display section is required".into()))?;
    if display.allow_multi_display {
        return invalid("client display.allow_multi_display must be false");
    }
    Ok(())
}

fn validate_neighbor_map(neighbors: &NeighborMap) -> Result<()> {
    parse_optional_neighbor(&neighbors.left)?;
    parse_optional_neighbor(&neighbors.right)?;
    parse_optional_neighbor(&neighbors.up)?;
    parse_optional_neighbor(&neighbors.down)?;
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(DeskFerryError::ConfigValidation(message.into()))
}
