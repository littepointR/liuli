use liuli::{prelude::Size, scene::capacity};
use liuli_widget_demo::{
    Catalog, Model,
    showcase::{ShowcaseApp, ShowcaseModel},
};

fn demand(app: &str, size: Size) -> liuli::component::Demand {
    match app {
        "showcase" => capacity::<ShowcaseApp>(&ShowcaseModel::for_size(size)),
        "studio" => capacity::<Catalog>(&Model::for_size(size)),
        _ => panic!("unknown view {app}"),
    }
    .expect("all declared layouts must build")
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let d = match args.as_slice() {
        [app, width, height] => demand(
            app,
            Size::new(
                width.parse().expect("width"),
                height.parse().expect("height"),
            ),
        ),
        [] => {
            let mut combined = liuli::component::Demand::EMPTY;
            for app in ["studio", "showcase"] {
                for size in [Size::new(800, 480), Size::new(240, 536)] {
                    combined = combined.alternative(demand(app, size));
                }
            }
            combined
        }
        _ => panic!("supply the view name, width and height together"),
    };
    println!("liuli-capacity-v1 {} {} {}", d.nodes, d.depth, d.text_slots);
}
