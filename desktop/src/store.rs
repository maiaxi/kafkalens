//! Saved connections, folders and window layout, as JSON in the per-user
//! config directory. Passwords are not stored here (see `secrets`).

use std::path::PathBuf;

use kafkalens_core::{ConfigEntry, ConnectionConfig, SaslMechanism, SecurityProtocol};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Security {
    #[default]
    Plaintext,
    Ssl,
    SaslPlaintext,
    SaslSsl,
}

impl Security {
    /// Order of the editor's combo box.
    pub const ALL: [Security; 4] = [
        Security::Plaintext,
        Security::Ssl,
        Security::SaslPlaintext,
        Security::SaslSsl,
    ];

    pub fn uses_sasl(self) -> bool {
        matches!(self, Security::SaslPlaintext | Security::SaslSsl)
    }

    fn core(self) -> SecurityProtocol {
        match self {
            Security::Plaintext => SecurityProtocol::Plaintext,
            Security::Ssl => SecurityProtocol::Ssl,
            Security::SaslPlaintext => SecurityProtocol::SaslPlaintext,
            Security::SaslSsl => SecurityProtocol::SaslSsl,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mechanism {
    #[default]
    Plain,
    ScramSha256,
    ScramSha512,
}

impl Mechanism {
    pub const ALL: [Mechanism; 3] = [
        Mechanism::Plain,
        Mechanism::ScramSha256,
        Mechanism::ScramSha512,
    ];

    fn core(self) -> SaslMechanism {
        match self {
            Mechanism::Plain => SaslMechanism::Plain,
            Mechanism::ScramSha256 => SaslMechanism::ScramSha256,
            Mechanism::ScramSha512 => SaslMechanism::ScramSha512,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    pub key: String,
    pub value: String,
}

/// A saved cluster connection. Missing fields take their defaults, so files
/// written by older versions still load.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub bootstrap_servers: String,
    pub folder_id: Option<String>,
    pub security: Security,
    pub mechanism: Mechanism,
    pub username: String,
    pub request_timeout_ms: u64,
    /// Blocks producing; on by default.
    pub read_only: bool,
    pub hide_unreadable_topics: bool,
    pub extra: Vec<Property>,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            id: new_id(),
            name: String::new(),
            bootstrap_servers: String::new(),
            folder_id: None,
            security: Security::Plaintext,
            mechanism: Mechanism::Plain,
            username: String::new(),
            request_timeout_ms: 10_000,
            read_only: true,
            hide_unreadable_topics: true,
            extra: Vec::new(),
        }
    }
}

impl Profile {
    pub fn config(&self, password: Option<String>) -> ConnectionConfig {
        let sasl = self.security.uses_sasl();
        ConnectionConfig {
            bootstrap_servers: self.bootstrap_servers.clone(),
            security_protocol: self.security.core(),
            sasl_mechanism: sasl.then(|| self.mechanism.core()),
            sasl_username: (sasl && !self.username.is_empty()).then(|| self.username.clone()),
            sasl_password: if sasl { password } else { None },
            request_timeout_ms: self.request_timeout_ms,
            read_only: self.read_only,
            extra: self
                .extra
                .iter()
                .filter(|p| !p.key.trim().is_empty())
                .map(|p| ConfigEntry {
                    key: p.key.clone(),
                    value: p.value.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub expanded: bool,
    /// Ask before every produce to a connection in this folder.
    pub confirms_produce: bool,
}

impl Default for Folder {
    fn default() -> Self {
        Folder {
            id: new_id(),
            name: String::new(),
            expanded: true,
            confirms_produce: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    /// In the order the sidebar shows them, which the user can change.
    pub folders: Vec<Folder>,
    pub connections: Vec<Profile>,
    /// Files from before folders could be reordered showed them sorted by
    /// name; they are sorted once on load so nothing moves.
    pub folders_ordered: bool,
}

impl Store {
    pub fn load() -> Store {
        let mut store: Store = read("connections.json").unwrap_or_else(|| Store {
            folders: Vec::new(),
            connections: vec![Profile {
                name: "Local".into(),
                bootstrap_servers: "localhost:9092".into(),
                ..Profile::default()
            }],
            folders_ordered: true,
        });
        store.adopt_folder_order();
        store
    }

    fn adopt_folder_order(&mut self) {
        if !self.folders_ordered {
            self.folders.sort_by_key(|f| f.name.to_lowercase());
            self.folders_ordered = true;
        }
    }

    pub fn save(&self) {
        write("connections.json", self);
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.connections.iter().find(|c| c.id == id)
    }

    pub fn profile_mut(&mut self, id: &str) -> Option<&mut Profile> {
        self.connections.iter_mut().find(|c| c.id == id)
    }

    pub fn folder(&self, id: &str) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    pub fn folder_mut(&mut self, id: &str) -> Option<&mut Folder> {
        self.folders.iter_mut().find(|f| f.id == id)
    }

    /// Folders in the user's order, as the sidebar and menus show them.
    pub fn sorted_folders(&self) -> Vec<&Folder> {
        self.folders.iter().collect()
    }

    /// Moves folder `id` next to `target` (before it, or after with
    /// `after`), or to the end when `target` is None.
    pub fn move_folder(&mut self, id: &str, target: Option<&str>, after: bool) {
        if target == Some(id) {
            return;
        }
        let Some(from) = self.folders.iter().position(|f| f.id == id) else { return };
        let folder = self.folders.remove(from);
        let index = match target.and_then(|t| self.folders.iter().position(|f| f.id == t)) {
            Some(index) => index + after as usize,
            None => self.folders.len(),
        };
        self.folders.insert(index, folder);
    }

    /// Moves folder `id` one place up (`-1`) or down (`1`).
    pub fn shift_folder(&mut self, id: &str, by: isize) {
        let Some(from) = self.folders.iter().position(|f| f.id == id) else { return };
        let to = from as isize + by;
        if (0..self.folders.len() as isize).contains(&to) {
            self.folders.swap(from, to as usize);
        }
    }

    /// Connections in `folder_id` (None: outside any folder), sorted by name.
    /// A connection whose folder no longer exists counts as outside.
    pub fn connections_in(&self, folder_id: Option<&str>) -> Vec<&Profile> {
        let mut list: Vec<&Profile> = self
            .connections
            .iter()
            .filter(|c| match (&c.folder_id, folder_id) {
                (Some(own), Some(wanted)) => own == wanted,
                (Some(own), None) => self.folder(own).is_none(),
                (None, None) => true,
                (None, Some(_)) => false,
            })
            .collect();
        list.sort_by_key(|c| c.name.to_lowercase());
        list
    }

    pub fn upsert(&mut self, profile: Profile) {
        match self.profile_mut(&profile.id) {
            Some(existing) => *existing = profile,
            None => self.connections.push(profile),
        }
    }

    /// Removes the folder; its connections move to the top level.
    pub fn delete_folder(&mut self, id: &str) {
        self.folders.retain(|f| f.id != id);
        for profile in &mut self.connections {
            if profile.folder_id.as_deref() == Some(id) {
                profile.folder_id = None;
            }
        }
    }
}

/// Pane widths (logical pixels), visibility and view preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub connections_width: f32,
    pub topics_width: f32,
    pub inspector_width: f32,
    pub show_connections: bool,
    pub show_topics: bool,
    pub show_inspector: bool,
    pub show_internal_topics: bool,
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
            show_internal_topics: false,
        }
    }
}

impl Layout {
    pub fn load() -> Layout {
        read("layout.json").unwrap_or_default()
    }

    pub fn save(&self) {
        write("layout.json", self);
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// `%APPDATA%\Kafkalens` on Windows, `$XDG_CONFIG_HOME/kafkalens` (or
/// `~/.config/kafkalens`) elsewhere.
fn config_file(name: &str) -> Option<PathBuf> {
    let dir = if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("APPDATA")?).join("Kafkalens")
    } else if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("kafkalens")
    } else {
        PathBuf::from(std::env::var_os("HOME")?)
            .join(".config")
            .join("kafkalens")
    };
    Some(dir.join(name))
}

fn read<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    let data = std::fs::read(config_file(name)?).ok()?;
    serde_json::from_slice(&data).ok()
}

fn write<T: Serialize + ?Sized>(name: &str, value: &T) {
    let Some(path) = config_file(name) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(data) = serde_json::to_vec_pretty(value) {
        let _ = std::fs::write(path, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str, folder: Option<&str>) -> Profile {
        Profile {
            name: name.into(),
            bootstrap_servers: "localhost:9092".into(),
            folder_id: folder.map(Into::into),
            ..Profile::default()
        }
    }

    #[test]
    fn new_profiles_are_read_only_and_hide_unreadable_topics() {
        let p = Profile::default();
        assert!(p.read_only);
        assert!(p.hide_unreadable_topics);
        assert!(p.config(None).read_only);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let store: Store = serde_json::from_str(
            r#"{"connections":[{"id":"a","name":"Old","bootstrap_servers":"h:9092"}]}"#,
        )
        .unwrap();
        let p = &store.connections[0];
        assert_eq!(p.security, Security::Plaintext);
        assert!(p.read_only);
        assert_eq!(p.request_timeout_ms, 10_000);
        assert!(store.folders.is_empty());
    }

    #[test]
    fn groups_connections_by_folder_sorted_by_name() {
        let mut store = Store::default();
        let folder = Folder {
            name: "Prod".into(),
            ..Folder::default()
        };
        let id = folder.id.clone();
        store.folders.push(folder);
        store.connections = vec![
            profile("zeta", Some(&id)),
            profile("alpha", Some(&id)),
            profile("loose", None),
            profile("orphan", Some("deleted-folder")),
        ];

        let in_folder: Vec<_> = store
            .connections_in(Some(&id))
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(in_folder, ["alpha", "zeta"]);
        // A connection whose folder is gone shows at the top level.
        let top: Vec<_> = store
            .connections_in(None)
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(top, ["loose", "orphan"]);
    }

    fn names(store: &Store) -> Vec<&str> {
        store.folders.iter().map(|f| f.name.as_str()).collect()
    }

    fn store_with_folders(list: &[&str]) -> Store {
        Store {
            folders: list.iter().map(|n| Folder { id: n.to_string(), name: n.to_string(), ..Folder::default() }).collect(),
            folders_ordered: true,
            ..Store::default()
        }
    }

    #[test]
    fn older_files_keep_the_alphabetical_folder_order() {
        let mut store: Store = serde_json::from_str(
            r#"{"folders":[{"id":"p","name":"Prod"},{"id":"d","name":"dev"}],"connections":[]}"#,
        )
        .unwrap();
        store.adopt_folder_order();
        assert_eq!(names(&store), ["dev", "Prod"]);
        // Once ordered, the user's order is kept as is.
        store.shift_folder("p", -1);
        store.adopt_folder_order();
        assert_eq!(names(&store), ["Prod", "dev"]);
    }

    #[test]
    fn moves_folders_before_after_and_to_the_end() {
        let mut store = store_with_folders(&["a", "b", "c", "d"]);
        store.move_folder("d", Some("b"), false);
        assert_eq!(names(&store), ["a", "d", "b", "c"]);
        store.move_folder("a", Some("b"), true);
        assert_eq!(names(&store), ["d", "b", "a", "c"]);
        store.move_folder("d", None, false);
        assert_eq!(names(&store), ["b", "a", "c", "d"]);
        // Dropping a folder on itself changes nothing.
        store.move_folder("a", Some("a"), true);
        assert_eq!(names(&store), ["b", "a", "c", "d"]);
    }

    #[test]
    fn shifts_folders_within_bounds() {
        let mut store = store_with_folders(&["a", "b", "c"]);
        store.shift_folder("a", -1);
        assert_eq!(names(&store), ["a", "b", "c"]);
        store.shift_folder("a", 1);
        assert_eq!(names(&store), ["b", "a", "c"]);
        store.shift_folder("c", 1);
        assert_eq!(names(&store), ["b", "a", "c"]);
    }

    #[test]
    fn deleting_a_folder_keeps_its_connections() {
        let mut store = Store::default();
        let folder = Folder {
            name: "Dev".into(),
            ..Folder::default()
        };
        let id = folder.id.clone();
        store.folders.push(folder);
        store.connections.push(profile("local", Some(&id)));

        store.delete_folder(&id);
        assert!(store.folders.is_empty());
        assert_eq!(store.connections.len(), 1);
        assert_eq!(store.connections[0].folder_id, None);
    }

    #[test]
    fn sasl_settings_only_apply_to_sasl_protocols() {
        let mut p = Profile {
            username: "reader".into(),
            extra: vec![
                Property {
                    key: "ssl.ca.location".into(),
                    value: "/ca.pem".into(),
                },
                Property {
                    key: "  ".into(),
                    value: "ignored".into(),
                },
            ],
            ..profile("x", None)
        };
        let plain = p.config(Some("secret".into()));
        assert_eq!(plain.sasl_password, None);
        assert_eq!(plain.sasl_username, None);
        assert_eq!(plain.extra.len(), 1);

        p.security = Security::SaslSsl;
        p.mechanism = Mechanism::ScramSha512;
        let sasl = p.config(Some("secret".into()));
        assert_eq!(sasl.sasl_password.as_deref(), Some("secret"));
        assert_eq!(sasl.sasl_username.as_deref(), Some("reader"));
        assert_eq!(sasl.sasl_mechanism, Some(SaslMechanism::ScramSha512));
    }
}
