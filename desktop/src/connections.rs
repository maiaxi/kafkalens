//! Saved connections and window layout, as JSON in the per-user config
//! directory.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedConnection {
    pub name: String,
    pub bootstrap_servers: String,
}

/// `%APPDATA%\Kafkalens` on Windows, `$XDG_CONFIG_HOME/kafkalens` (or
/// `~/.config/kafkalens`) elsewhere.
fn config_file(name: &str) -> Option<PathBuf> {
    let dir = if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("APPDATA")?).join("Kafkalens")
    } else if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("kafkalens")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".config").join("kafkalens")
    };
    Some(dir.join(name))
}

fn read<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    let data = std::fs::read(config_file(name)?).ok()?;
    serde_json::from_slice(&data).ok()
}

fn write<T: Serialize + ?Sized>(name: &str, value: &T) {
    let Some(path) = config_file(name) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(data) = serde_json::to_vec_pretty(value) {
        let _ = std::fs::write(path, data);
    }
}

pub fn load() -> Vec<SavedConnection> {
    read("connections.json").unwrap_or_else(|| {
        vec![SavedConnection {
            name: "Local".into(),
            bootstrap_servers: "localhost:9092".into(),
        }]
    })
}

pub fn save(connections: &[SavedConnection]) {
    write("connections.json", connections);
}

/// Pane widths (logical pixels) and visibility, restored at startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub connections_width: f32,
    pub topics_width: f32,
    pub inspector_width: f32,
    pub show_connections: bool,
    pub show_topics: bool,
    pub show_inspector: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            connections_width: 210.0,
            topics_width: 240.0,
            inspector_width: 340.0,
            show_connections: true,
            show_topics: true,
            show_inspector: true,
        }
    }
}

pub fn load_layout() -> Layout {
    read("layout.json").unwrap_or_default()
}

pub fn save_layout(layout: &Layout) {
    write("layout.json", layout);
}
