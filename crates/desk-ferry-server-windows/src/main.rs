use std::{env, time::Duration};

use desk_ferry_common::{config::load_config_file, logging::protocol_message_summary, Result};
use desk_ferry_server_windows::{
    display::resolve_display_topology,
    input::{safe_action_summary, InputMode},
    platform::enumerate_monitors,
    platform::{run_input_hooks, InputHookConfig},
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

    println!("desk-ferry-server-windows: Stage 3B backend skeleton ready");
    println!("Use --config <path> --list-displays to inspect Windows monitor layout.");
    println!("Use --config <path> --input-dry-run to inspect input hook events safely.");
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
        },
        |result| {
            println!("suppress_input={}", result.suppress_input);
            for action in &result.actions {
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    config_path: String,
    list_displays: bool,
    input_dry_run: bool,
    input_suppress: bool,
    duration_ms: Option<u64>,
    active_host: Option<String>,
}

impl Args {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut config_path = "examples/server-windows.toml".to_string();
        let mut list_displays = false;
        let mut input_dry_run = false;
        let mut input_suppress = false;
        let mut duration_ms = None;
        let mut active_host = None;
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
                "--help" | "-h" => {
                    println!(
                        "usage: desk-ferry-server-windows [--config <path>] [--list-displays] [--input-dry-run] [--input-suppress] [--active-host <host>] [--duration-ms <ms>]"
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

        if input_suppress && !input_dry_run {
            return Err(desk_ferry_common::DeskFerryError::ConfigValidation(
                "--input-suppress requires --input-dry-run in Stage 3B".to_string(),
            ));
        }

        Ok(Self {
            config_path,
            list_displays,
            input_dry_run,
            input_suppress,
            duration_ms,
            active_host,
        })
    }
}
