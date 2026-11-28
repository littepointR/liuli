use liuli::scene::capacity;
use liuli_settings::{SettingsApp, SettingsModel, TerminalApp, TerminalModel};
fn main() {
    let settings = capacity::<SettingsApp>(&SettingsModel::default()).unwrap();
    let terminal = capacity::<TerminalApp>(&TerminalModel::default()).unwrap();
    let d = match std::env::args().nth(1).as_deref() {
        Some("terminal") => terminal,
        Some("settings") => settings,
        None => terminal.alternative(settings),
        Some(name) => panic!("unknown view {name}"),
    };
    println!("liuli-capacity-v1 {} {} {}", d.nodes, d.depth, d.text_slots);
}
