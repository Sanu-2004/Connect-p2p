use serde::{Serialize, Deserialize};
use chrono::{Local, TimeZone};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatPacket {
    username: String,
    message: String,
    time: u64,
}

impl ChatPacket {
    pub fn new(name: String, message: String) -> Self {
        let time = Local::now().timestamp() as u64; 
        ChatPacket {
            username: name,
            message: message.trim().to_string(),
            time,
        }
    }
    pub fn display(&self) {
        let time = Local.timestamp_opt(self.time as i64, 0).unwrap();
        println!("[{}] {}: {}", time.format("%H:%M:%S"), self.username, self.message);
    }
}