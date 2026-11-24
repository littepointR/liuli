use device_ui::Device;
use liuli::prelude::*;
use liuli_sim::{preview::PreviewServer, session::AppSession};
fn main() -> Result<(), String> {
    let font =
        Lf4Font::from_bytes(include_bytes!("../assets/body.lf4")).map_err(|e| format!("{e:?}"))?;
    let mut storage = Storage::new();
    let mut session = AppSession::<Device>::new(
        &mut storage,
        false,
        FontSet::single(font),
        Size::new(240, 536),
        InstantMs(0),
        true,
    )?;
    if std::env::args().any(|arg| arg == "--headless") {
        session.step(InstantMs(1), Some(RawInput::Activate), true)?;
        println!("frame {}", session.snapshot().frame_id);
        return Ok(());
    }
    if let Some(path) = std::env::args_os()
        .skip_while(|arg| arg != "--replay")
        .nth(1)
    {
        let trace = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        return liuli_sim::preview::replay_trace(&mut session, &trace);
    }
    let server = PreviewServer::bind(7878)?;
    println!("http://{}", server.addr());
    server.serve_app(&mut session)
}
