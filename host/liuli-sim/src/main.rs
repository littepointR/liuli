use std::path::PathBuf;

use liuli_sim::preview::PreviewServer;
use liuli_sim::replay::{ImageFormat, run_replay_file};
use liuli_sim::sim::SimScenario;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");

    let res = match cmd {
        "replay" => run_replay_cli(&args[2..]),
        "preview" => run_preview_cli(&args[2..]),
        _ => {
            eprintln!(
                "Usage:\n  liuli-sim replay <events-file> [--out <dir>] [--format png|ppm]\n  liuli-sim preview [--port <port>] [--scenario terminal|settings]"
            );
            std::process::exit(1);
        }
    };

    if let Err(err) = res {
        eprintln!("liuli-sim error: {err}");
        std::process::exit(1);
    }
}

fn run_replay_cli(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("missing events file path".into());
    }
    let file = PathBuf::from(&args[0]);
    let mut out_dir: Option<PathBuf> = None;
    let mut format = ImageFormat::Png;

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                let dir = args.get(i).ok_or("--out requires directory")?;
                out_dir = Some(PathBuf::from(dir));
            }
            "--format" => {
                i += 1;
                let fmt = args.get(i).ok_or("--format requires png or ppm")?;
                format = match fmt.as_str() {
                    "png" => ImageFormat::Png,
                    "ppm" => ImageFormat::Ppm,
                    other => return Err(format!("unknown format '{other}'")),
                };
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
        i += 1;
    }

    let report = run_replay_file(&file, out_dir.as_deref(), format)?;
    println!(
        "Replay OK: {} shots, {} frames presented, final t={} ms, wakeup={:?}",
        report.shots.len(),
        report.frames_presented,
        report.final_now.0,
        report.final_wakeup
    );
    Ok(())
}

fn run_preview_cli(args: &[String]) -> Result<(), String> {
    let mut port = 7878u16;
    let mut scenario = SimScenario::Terminal;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                let p = args.get(i).ok_or("--port requires number")?;
                port = p.parse().map_err(|e| format!("bad port '{p}': {e}"))?;
            }
            "--scenario" => {
                i += 1;
                let s = args
                    .get(i)
                    .ok_or("--scenario requires terminal or settings")?;
                scenario = match s.as_str() {
                    "terminal" => SimScenario::Terminal,
                    "settings" => SimScenario::Settings,
                    other => return Err(format!("unknown scenario '{other}'")),
                };
            }
            other => return Err(format!("unknown preview flag '{other}'")),
        }
        i += 1;
    }

    let server = PreviewServer::bind_scenario(port, scenario)?;
    println!(
        "liuli preview ({:?}) listening on http://{}/",
        server.scenario(),
        server.addr()
    );
    server.serve()
}
