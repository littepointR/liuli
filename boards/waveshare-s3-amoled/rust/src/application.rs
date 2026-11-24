pub use liuli_alarms::Alarms as BoardApp;
pub fn model(height: u16) -> liuli_alarms::Model {
    liuli_alarms::Model::new(height)
}
