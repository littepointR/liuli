pub type BoardApp = liuli_alarms::AlarmApp<{ super::HEIGHT as u16 - 32 }>;
pub fn model(height: u16) -> liuli_alarms::Model {
    liuli_alarms::Model::new(height)
}
