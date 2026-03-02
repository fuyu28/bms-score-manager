use chrono::Utc;
use serde_json::{Map, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug)]
pub struct JsonlLogger {
    path: PathBuf,
    lock: Mutex<()>,
}

impl JsonlLogger {
    pub fn new(path: PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self {
            path,
            lock: Mutex::new(()),
        })
    }

    pub fn log(&self, event: &str, mut payload: Map<String, Value>) {
        payload.insert(
            "timestamp".to_string(),
            Value::String(Utc::now().to_rfc3339()),
        );
        payload.insert("event".to_string(), Value::String(event.to_string()));

        let _guard = match self.lock.lock() {
            Ok(g) => g,
            Err(_) => return,
        };

        let mut file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            Ok(f) => f,
            Err(_) => return,
        };

        if let Ok(line) = serde_json::to_string(&payload) {
            let _ = writeln!(file, "{}", line);
        }
    }
}
