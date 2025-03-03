use std::io;

use crossterm::{
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
};
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
        let msg = format!("[{}] {}: {} \n", time.format("%H:%M:%S"), self.username, self.message);
        if let Err(e) = execute!(
            io::stdout(),
            SetForegroundColor(Color::Yellow),
            Print(msg),
            ResetColor
        ) {
            println!("Error Printing Chat Mesasge, {}", e);
        }
        
    }
}