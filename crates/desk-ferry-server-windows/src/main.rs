use std::{
    cell::RefCell,
    env, fs,
    io::ErrorKind,
    net::TcpListener,
    rc::Rc,
    time::{Duration, Instant},
};

use desk_ferry_common::{
    config::load_config_file,
    logging::protocol_message_summary,
    mock::MockServerSession,
    protocol::ProtocolMessage,
    security::Psk,
    transport::{
        accept_tls, is_graceful_disconnect_error, read_message, send_message,
        server_config_from_pem,
    },
    DeskFerryError, Result,
};
use desk_ferry_server_windows::{
    display::resolve_display_topology,
    input::{safe_action_summary, EmergencyHotkey, InputMode},
    platform::enumerate_monitors,
    platform::{run_input_hooks, run_input_hooks_with_tick, HookInputResult, InputHookConfig},
    server::{ServerEventResult, WindowsServerIntegration},
    transition::format_display_mapping,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("desk-ferry-server-windows: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse(env::args().skip(1))?;
    if args.list_displays {
        list_displays(&args.config_path)?;
        return Ok(());
    }
    if args.input_dry_run {
        input_dry_run(&args)?;
        return Ok(());
    }
    if args.serve_input_dry_run {
        serve_input_dry_run(&args)?;
        return Ok(());
    }

    println!("desk-ferry-server-windows: Stage 3C backend skeleton ready");
    println!("Use --config <path> --list-displays to inspect Windows monitor layout.");
    println!("Use --config <path> --input-dry-run to inspect input hook events safely.");
    println!("Use --config <path> --serve-input-dry-run to accept a secure mock-client.");
    Ok(())
}

fn list_displays(config_path: &str) -> Result<()> {
    let config = load_config_file(config_path)?;
    let monitors = enumerate_monitors()?;

    println!("desk-ferry-server-windows: detected Windows displays");
    for monitor in &monitors {
        let rect = monitor.rect;
        println!(
            "monitor={} rect=({}, {})-({}, {}) size={}x{} primary={}",
            monitor.name,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            rect.width(),
            rect.height(),
            monitor.primary
        );
    }

    let topology = resolve_display_topology(&config, monitors)?;
    let rect = topology.virtual_screen;
    println!(
        "virtual_screen=({}, {})-({}, {}) size={}x{}",
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        rect.width(),
        rect.height()
    );
    println!("configured display aliases:");
    for display in topology.displays.values() {
        println!("{}", format_display_mapping(display));
    }

    Ok(())
}

fn input_dry_run(args: &Args) -> Result<()> {
    let config = load_config_file(&args.config_path)?;
    let duration = args.duration_ms.map(Duration::from_millis);
    let mode = if args.input_suppress {
        InputMode::Suppress
    } else {
        InputMode::DryRun
    };

    println!("desk-ferry-server-windows: input dry-run started");
    println!("input mode: {:?}", mode);
    println!("suppression active: {}", mode == InputMode::Suppress);
    println!("safe logs only: event type and state summaries");
    run_input_hooks(
        InputHookConfig {
            server_host: config.host.name,
            initial_active_host: args.active_host.clone(),
            mode,
            duration,
            emergency_hotkey: emergency_hotkey(
                &config.input.emergency_hotkey,
                &args.emergency_hotkey,
            )?,
        },
        |hook_result| {
            println!("suppress_input={}", hook_result.result.suppress_input);
            for action in &hook_result.result.actions {
                println!("{}", safe_action_summary(action));
                if let Some(message) = &action.message {
                    println!(
                        "send candidate: {}",
                        protocol_message_summary(message).message
                    );
                }
            }
        },
    )?;
    println!("desk-ferry-server-windows: input dry-run stopped");
    Ok(())
}

fn serve_input_dry_run(args: &Args) -> Result<()> {
    let config = load_config_file(&args.config_path)?;
    let emergency_hotkey =
        emergency_hotkey(&config.input.emergency_hotkey, &args.emergency_hotkey)?;
    let mode = if args.input_suppress {
        InputMode::Suppress
    } else {
        InputMode::DryRun
    };
    let duration = args.duration_ms.map(Duration::from_millis);
    let cert_file = config.security.cert_file.as_deref().ok_or_else(|| {
        DeskFerryError::ConfigValidation(
            "security.cert_file is required for server-windows dry-run".to_string(),
        )
    })?;
    let key_file = config.security.key_file.as_deref().ok_or_else(|| {
        DeskFerryError::ConfigValidation(
            "security.key_file is required for server-windows dry-run".to_string(),
        )
    })?;
    let network = config.network.as_ref().ok_or_else(|| {
        DeskFerryError::ConfigValidation("server network is required".to_string())
    })?;
    let address = format!(
        "{}:{}",
        network.bind_address.as_deref().unwrap_or("127.0.0.1"),
        network.port.unwrap_or(24800)
    );

    let psk = Psk::from_bytes(fs::read(&config.security.psk_file)?)?;
    let tls_config = server_config_from_pem(cert_file, key_file)?;
    let monitors = enumerate_monitors()?;
    let topology = resolve_display_topology(&config, monitors)?;
    let mut server_state =
        WindowsServerIntegration::new(&config, topology, mode, emergency_hotkey.clone())?;

    let listener = TcpListener::bind(&address)?;
    println!("TLS listen started");
    let (tcp, _) = listener.accept()?;
    let mut stream = accept_tls(tcp, tls_config)?;

    let mut auth_session =
        MockServerSession::new(config.host.name.clone(), config.clients.clone(), psk);
    send_message(&mut stream, &auth_session.challenge_message())?;
    let response = read_message(&mut stream)?;
    let ProtocolMessage::AuthResponse(response) = response else {
        return Err(DeskFerryError::Authentication(
            "expected auth response".to_string(),
        ));
    };
    let result = auth_session.authenticate(&response);
    send_message(&mut stream, &result)?;
    if !auth_session.is_authenticated() {
        return Err(DeskFerryError::Authentication(
            "client authentication failed".to_string(),
        ));
    }

    let hello = read_message(&mut stream)?;
    auth_session.accept_authenticated_message(&hello)?;
    let ProtocolMessage::Hello(hello) = hello else {
        return Err(DeskFerryError::Authentication("expected hello".to_string()));
    };
    stream
        .sock
        .set_read_timeout(Some(Duration::from_millis(10)))?;
    let result = server_state.register_authenticated_client(hello.host_name);
    send_server_result(&mut stream, result)?;

    println!("client authenticated");
    println!("input mode: {:?}", mode);
    println!("suppression active: {}", mode == InputMode::Suppress);

    let runtime = Rc::new(RefCell::new(ServerRuntime {
        stream,
        server_state,
        started: Instant::now(),
        duration,
        disconnected: false,
    }));
    let hook_runtime = Rc::clone(&runtime);
    let tick_runtime = Rc::clone(&runtime);

    run_input_hooks_with_tick(
        InputHookConfig {
            server_host: config.host.name,
            initial_active_host: None,
            mode,
            duration,
            emergency_hotkey,
        },
        move |hook_result| {
            if let Err(error) = hook_runtime.borrow_mut().handle_hook_result(hook_result) {
                eprintln!("desk-ferry-server-windows: {error}");
            }
        },
        move || tick_runtime.borrow_mut().tick(),
    )?;

    if !runtime.borrow().disconnected {
        let result = runtime.borrow_mut().server_state.handle_disconnect();
        let _ = runtime.borrow_mut().send_server_result(result);
    }

    Ok(())
}

struct ServerRuntime {
    stream: desk_ferry_common::transport::TlsServerStream,
    server_state: WindowsServerIntegration,
    started: Instant,
    duration: Option<Duration>,
    disconnected: bool,
}

impl ServerRuntime {
    fn handle_hook_result(&mut self, hook_result: HookInputResult) -> Result<()> {
        if self.disconnected {
            return Ok(());
        }
        let now_ms = self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let result = self
            .server_state
            .handle_local_input(hook_result.event, now_ms)?;
        self.send_server_result(result)
    }

    fn tick(&mut self) -> Result<()> {
        if self.disconnected {
            return Ok(());
        }
        if self
            .duration
            .is_some_and(|duration| self.started.elapsed() >= duration)
        {
            return Ok(());
        }

        match read_message(&mut self.stream) {
            Ok(message) => {
                let result = self.server_state.handle_client_message(message)?;
                self.send_server_result(result)?;
            }
            Err(DeskFerryError::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) if is_graceful_disconnect_error(&error) => {
                let result = self.server_state.handle_disconnect();
                self.send_server_result(result)?;
                self.disconnected = true;
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    fn send_server_result(&mut self, result: ServerEventResult) -> Result<()> {
        match send_server_result(&mut self.stream, result) {
            Ok(()) => Ok(()),
            Err(error) if is_graceful_disconnect_error(&error) => {
                self.disconnected = true;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

fn send_server_result(
    stream: &mut desk_ferry_common::transport::TlsServerStream,
    result: ServerEventResult,
) -> Result<()> {
    for event in result.log_events {
        println!("{event}");
    }
    for message in result.messages {
        println!("{}", protocol_message_summary(&message).message);
        send_message(stream, &message)?;
    }
    Ok(())
}

fn emergency_hotkey(
    config_value: &Option<String>,
    override_value: &Option<String>,
) -> Result<EmergencyHotkey> {
    override_value
        .as_deref()
        .or(config_value.as_deref())
        .unwrap_or("Ctrl+Alt+Shift+Esc")
        .parse()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    config_path: String,
    list_displays: bool,
    input_dry_run: bool,
    serve_input_dry_run: bool,
    input_suppress: bool,
    duration_ms: Option<u64>,
    active_host: Option<String>,
    emergency_hotkey: Option<String>,
}

impl Args {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut config_path = "examples/server-windows.toml".to_string();
        let mut list_displays = false;
        let mut input_dry_run = false;
        let mut serve_input_dry_run = false;
        let mut input_suppress = false;
        let mut duration_ms = None;
        let mut active_host = None;
        let mut emergency_hotkey = None;
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--config" => {
                    config_path = args.next().ok_or_else(|| {
                        desk_ferry_common::DeskFerryError::ConfigValidation(
                            "--config requires a path".to_string(),
                        )
                    })?;
                }
                "--list-displays" => list_displays = true,
                "--input-dry-run" => input_dry_run = true,
                "--serve-input-dry-run" => serve_input_dry_run = true,
                "--input-suppress" => input_suppress = true,
                "--duration-ms" => {
                    let value = args.next().ok_or_else(|| {
                        desk_ferry_common::DeskFerryError::ConfigValidation(
                            "--duration-ms requires a value".to_string(),
                        )
                    })?;
                    duration_ms = Some(value.parse::<u64>().map_err(|_| {
                        desk_ferry_common::DeskFerryError::ConfigValidation(
                            "--duration-ms must be a positive integer".to_string(),
                        )
                    })?);
                }
                "--active-host" => {
                    active_host = Some(args.next().ok_or_else(|| {
                        desk_ferry_common::DeskFerryError::ConfigValidation(
                            "--active-host requires a host name".to_string(),
                        )
                    })?);
                }
                "--emergency-hotkey" => {
                    emergency_hotkey = Some(args.next().ok_or_else(|| {
                        desk_ferry_common::DeskFerryError::ConfigValidation(
                            "--emergency-hotkey requires a value".to_string(),
                        )
                    })?);
                }
                "--help" | "-h" => {
                    println!(
                        "usage: desk-ferry-server-windows [--config <path>] [--list-displays] [--input-dry-run] [--serve-input-dry-run] [--input-suppress] [--active-host <host>] [--emergency-hotkey <combo>] [--duration-ms <ms>]"
                    );
                    std::process::exit(0);
                }
                _ => {
                    return Err(desk_ferry_common::DeskFerryError::ConfigValidation(
                        format!("unknown argument '{arg}'"),
                    ));
                }
            }
        }

        if input_suppress && !(input_dry_run || serve_input_dry_run) {
            return Err(desk_ferry_common::DeskFerryError::ConfigValidation(
                "--input-suppress requires an input dry-run mode in Stage 3C".to_string(),
            ));
        }

        Ok(Self {
            config_path,
            list_displays,
            input_dry_run,
            serve_input_dry_run,
            input_suppress,
            duration_ms,
            active_host,
            emergency_hotkey,
        })
    }
}
