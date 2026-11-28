use liuli::prelude::*;
use std::{hint::black_box, time::Instant};
struct Label;
impl App for Label {
    type Model = ();
    type Msg = ();
    fn update(_: &mut (), _: (), _: InstantMs) -> Change {
        Change::Unchanged
    }
    fn on_action(_: &(), _: UiAction) -> Option<()> {
        None
    }
    fn view(_: &()) -> impl Scene {
        layout(Key(1), NodeProps::new(), move |ui| {
            ui.label(Key(2), NodeProps::new().text(TextRef::Static("A")))
        })
    }
}
struct Sink(usize);
impl DisplaySink for Sink {
    type Error = ();
    fn flush_rect(&mut self, rect: Rect, _: &[u16]) -> Result<(), ()> {
        self.0 += (rect.w * rect.h) as usize;
        Ok(())
    }
}
fn main() {
    let font = Lf4Font::from_bytes(include_bytes!(
        "../../../../apps/liuli-widget-demo/assets/heading.lf4"
    ))
    .unwrap();
    let mut storage = Box::new(Storage::ZERO);
    let mut e = Engine::<Label>::new_in_place_with_provider(
        &mut storage,
        (),
        FontSet::single(font),
        Rect::new(0, 0, 64, 32),
        InstantMs(0),
    )
    .unwrap();
    let mut sink = Sink(0);
    let mut scratch = [0u16; 512];
    e.present(&mut sink, &mut scratch).unwrap();
    let initial = sink.0;
    let mut time = 0;
    let mut batches = [0u128; 9];
    let mut changes = 0;
    let mut deadlines = 0;
    for batch in &mut batches {
        let start = Instant::now();
        for _ in 0..100_000 {
            time += 1;
            changes += usize::from(black_box(e.tick(InstantMs(time)).unwrap()));
            deadlines += usize::from(black_box(e.wakeup(InstantMs(time))) != NextWakeup::OnEvent);
        }
        *batch = start.elapsed().as_nanos() / 100_000;
    }
    e.present(&mut sink, &mut scratch).unwrap();
    batches.sort_unstable();
    println!(
        "ticks={} changes={} scheduled={} initial_px={} added_px={} median_ns_per_tick_and_query={} sorted={:?}",
        time,
        changes,
        deadlines,
        initial,
        sink.0 - initial,
        batches[4],
        batches
    );
}
