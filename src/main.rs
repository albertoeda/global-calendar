#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use std::path::PathBuf;
use team_clock::app::TeamClockApp;

const USAGE: &str = "\
team-clock — see the local time of everyone in a global team on a world map

USAGE:
    team-clock [PATH/TO/team.toml]

Without a path, looks for ./team.toml, then team.toml in the platform config dir.
Press ? in the app for keyboard shortcuts.";

fn main() -> eframe::Result {
    let mut path = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("team-clock {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => path = Some(PathBuf::from(arg)),
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Team Clock")
            .with_app_id("team-clock")
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([720.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Team Clock",
        options,
        Box::new(move |cc| Ok(Box::new(TeamClockApp::new(cc, path)))),
    )
}
