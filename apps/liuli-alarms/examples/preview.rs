use liuli::prelude::*;
use liuli_alarms::{Alarms, Model};
use liuli_sim::{preview::PreviewServer, session::AppSession};
fn main() -> Result<(), String> {
    let file = std::env::args_os()
        .nth(1)
        .ok_or("usage: preview <font.lf4>")?;
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    let fonts = FontSet::single(Lf4Font::from_bytes(&bytes).map_err(|e| format!("{e:?}"))?);
    let mut storage = Storage::new();
    let mut app = AppSession::<Alarms>::new(
        &mut storage,
        Model::new(536),
        fonts,
        Size::new(240, 536),
        InstantMs(0),
        true,
    )?;
    if let Some(path) = std::env::args_os()
        .skip_while(|arg| arg != "--replay")
        .nth(1)
    {
        let trace = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        return liuli_sim::preview::replay_trace(&mut app, &trace);
    }
    let server = PreviewServer::bind(7878)?;
    println!("http://{}", server.addr());
    server.serve_app(&mut app)
}
