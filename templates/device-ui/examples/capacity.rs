use liuli::prelude::*;
fn main() {
    let d = device_ui::Device::view(&false).static_demand().unwrap();
    println!("liuli-capacity-v1 {} {} {}", d.nodes, d.depth, d.text_slots);
}
