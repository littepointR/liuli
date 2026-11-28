use liuli::prelude::*;
fn main() {
    let d = liuli_alarms::AlarmApp::<{ liuli_alarms::LIST_VIEWPORT }>::view(
        &liuli_alarms::Model::new(536),
    )
    .static_demand()
    .unwrap();
    println!("liuli-capacity-v1 {} {} {}", d.nodes, d.depth, d.text_slots);
}
