use liuli::{
    component::{Bounded, Demand},
    prelude::*,
};

// An external composite starts empty inside an unopened, nested page.
struct Pages;
impl App for Pages {
    type Model = u32;
    type Msg = ();
    fn update(_: &mut u32, _: (), _: InstantMs) -> Change {
        Change::Unchanged
    }
    fn on_action(_: &u32, _: UiAction) -> Option<()> {
        None
    }
    fn view(count: &u32) -> impl Scene {
        layout(Key(0), NodeProps::new(), move |ui| {
            match ui.variant(0, &[0, 1, 2]) {
                0 => ui.label(Key(1), NodeProps::new()),
                1 => ui.column(Key(2), NodeProps::new(), |rows| {
                    rows.show(Bounded::<3>::rows(
                        Key(2),
                        0..*count,
                        product_controls::record,
                    ))
                }),
                _ => ui.column(Key(3), NodeProps::new(), |panel| {
                    if panel.optional(false) {
                        panel.column(Key(4), NodeProps::new(), |nested| {
                            nested.column(Key(5), NodeProps::new(), |end| {
                                end.label(Key(6), NodeProps::new())
                            })
                        })?;
                    }
                    Ok(())
                }),
            }
        })
    }
}
fn main() {
    use liuli::component::Component;
    fn bound<C: Component>(_: C) -> Demand {
        C::DEMAND
    }
    let expected = bound(product_controls::record(0))
        .repeated(3)
        .enclosed()
        .enclosed()
        .alternative(Demand {
            nodes: 5,
            depth: 5,
            text_slots: 1,
        });
    for count in 0..=3 {
        assert_eq!(liuli::scene::capacity::<Pages>(&count).unwrap(), expected);
    }
    let d = planned_product::Product::view(&planned_product::Model::default())
        .static_demand()
        .unwrap();
    println!("liuli-capacity-v1 {} {} {}", d.nodes, d.depth, d.text_slots);
}
