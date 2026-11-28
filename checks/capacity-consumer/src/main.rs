use liuli::prelude::*;
use planned_product::{Model, Product};
struct Screen(bool);
impl DisplaySink for Screen {
    type Error = ();
    fn flush_rect(&mut self, _: Rect, pixels: &[u16]) -> Result<(), ()> {
        self.0 |= pixels.contains(&rgb565_to_u16(product_controls::BRAND));
        Ok(())
    }
}
fn verify_stock_components() {
    use liuli::component::Component;
    use liuli_widgets::{DropdownState, TableColumn, Theme, components};

    fn check(children: impl Component, expected: usize) {
        let declaration = scene(Key(0), NodeProps::new(), children);
        let capacity = declaration.static_demand().unwrap();
        let mut draft = liuli::core::view::ViewDraft::new();
        let mut view = ViewBuilder::new(&mut draft);
        declaration.build(&mut view).unwrap();
        view.finish().unwrap();
        assert_eq!(capacity.nodes, expected);
        assert_eq!(usize::from(draft.count), expected);
    }
    let theme = Theme::DARK;
    let keys = [Key(1), Key(2), Key(3)];
    check(
        components::dropdown::<3>(
            keys,
            10,
            &["A", "B", "C"],
            DropdownState {
                open: true,
                selected: 1,
            },
            theme,
        ),
        7,
    );
    let columns = [TableColumn::new("Value", Length::Fill(1), TextAlign::Start); 2];
    check(
        components::data_table::<2, _>(
            Key(1),
            &columns,
            &[&["A", "B"], &["C", "D"]],
            None,
            24,
            theme,
        ),
        11,
    );
    let tabs = [
        (Key(10), TextRef::Static("First")),
        (Key(11), TextRef::Static("Second")),
    ];
    check(
        (
            components::tab_strip::<2>(
                Key(2),
                &tabs,
                Key(11),
                liuli_widgets::TabLayout::Scroll,
                theme,
            ),
            components::tab_panel(
                Key(3),
                Key(11),
                NodeProps::new(),
                liuli::component::single(Key(12), |ui| {
                    liuli_widgets::checkbox(ui, Key(12), TextRef::Static("On"), true, theme)
                }),
            ),
        ),
        6,
    );
}

fn main() {
    verify_stock_components();
    let bytes = std::fs::read(std::env::args().nth(1).expect("font path")).unwrap();
    let d = Product::view(&Model::default()).static_demand().unwrap();
    assert_eq!(
        (MAX_NODES, MAX_DEPTH, liuli::core::capacity::MAX_TEXT_SLOTS),
        (d.nodes, d.depth, d.text_slots)
    );
    let mut storage = Storage::new();
    let mut app = Engine::<Product>::new_in_place(
        &mut storage,
        Model::default(),
        Lf4Font::from_bytes(&bytes).unwrap(),
        Rect::new(0, 0, 240, 160),
        InstantMs(0),
    )
    .unwrap();
    let state = Model {
        count: 3,
        ..Model::default()
    };
    app.try_message(state, InstantMs(1), false).unwrap();
    let id = |tree: &liuli::core::node::RetainedTree| {
        let slot = tree.find_slot_by_key(Key(103)).unwrap();
        tree.nodes[slot as usize].id(slot)
    };
    let before = id(app.active_tree());
    let mut sink = Screen(false);
    app.present(&mut sink, &mut [0; 240 * 8]).unwrap();
    assert!(sink.0, "external part painter never reached the display");
    app.try_message(
        Model {
            first: 1,
            count: 2,
            ..state
        },
        InstantMs(2),
        false,
    )
    .unwrap();
    assert_eq!(id(app.active_tree()), before);
    app.try_message(
        Model {
            confirm: true,
            ..state
        },
        InstantMs(3),
        false,
    )
    .unwrap();
    assert_eq!(usize::from(app.active_tree().active_count), d.nodes);
    let version = app.scene_version();
    assert_eq!(
        app.try_message(Model { count: 4, ..state }, InstantMs(4), false),
        Err(TransactionError::Build(BuildError::ItemLimit))
    );
    assert_eq!(
        app.build_failure(),
        Some((0, Some(Key(2)), BuildError::ItemLimit))
    );
    assert_eq!(app.scene_version(), version);
    for i in 0..20 {
        app.try_message(
            Model {
                history: true,
                offset: 200 + i,
                ..state
            },
            InstantMs(5 + i as u64),
            false,
        )
        .unwrap();
        assert!(app.active_tree().active_count <= 11);
    }
    app.try_message(Model::default(), InstantMs(30), false)
        .unwrap();
    assert_eq!(app.active_tree().active_count, 2);
    println!(
        "qualified {} nodes, {} levels, {} text slots; Storage {} B",
        MAX_NODES,
        MAX_DEPTH,
        d.text_slots,
        size_of::<Storage>()
    );
}
