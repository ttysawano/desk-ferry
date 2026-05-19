use std::env;

use desk_ferry_common::{config::load_config_file, Result};
use desk_ferry_server_windows::{
    display::resolve_display_topology, platform::enumerate_monitors,
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

    println!("desk-ferry-server-windows: Stage 3A backend skeleton ready");
    println!("Use --config <path> --list-displays to inspect Windows monitor layout.");
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    config_path: String,
    list_displays: bool,
}

impl Args {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut config_path = "examples/server-windows.toml".to_string();
        let mut list_displays = false;
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
                "--help" | "-h" => {
                    println!(
                        "usage: desk-ferry-server-windows [--config <path>] [--list-displays]"
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

        Ok(Self {
            config_path,
            list_displays,
        })
    }
}
