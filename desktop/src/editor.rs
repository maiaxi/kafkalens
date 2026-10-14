//! The connection editor sheet and the folder name prompt.

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::store::{Folder, Mechanism, Profile, Property, Security};
use crate::{secrets, sidebar, AppWindow, PropertyItem, Shared};

/// What the open editor is editing.
pub struct Editor {
    profile_id: String,
    /// Kept from the profile; not edited in the sheet.
    hide_unreadable_topics: bool,
    /// Folder for each entry of the folder combo box ("None" first).
    folder_ids: Vec<Option<String>>,
    /// Source of truth for the property rows, which the UI only displays.
    extra: Vec<Property>,
    /// Bumped per test, so a slow earlier test cannot overwrite a newer one.
    test_generation: u64,
}

pub enum Prompt {
    NewFolder,
    RenameFolder(String),
}

pub fn open(ui: &AppWindow, state: &Shared, profile: Profile, is_new: bool) {
    let mut s = state.lock().unwrap();

    let folders = s.store.sorted_folders();
    let mut names: Vec<SharedString> = vec!["None".into()];
    let mut folder_ids = vec![None];
    for folder in &folders {
        names.push(folder.name.as_str().into());
        folder_ids.push(Some(folder.id.clone()));
    }
    let folder_index = folder_ids
        .iter()
        .position(|id| id.as_deref() == profile.folder_id.as_deref())
        .unwrap_or(0);

    let password = if is_new {
        None
    } else {
        secrets::password(&profile.id)
    };

    ui.set_editor_heading(
        if is_new {
            "New Connection"
        } else {
            "Edit Connection"
        }
        .into(),
    );
    ui.set_editor_name(profile.name.as_str().into());
    ui.set_editor_servers(profile.bootstrap_servers.as_str().into());
    ui.set_editor_folder_names(ModelRc::new(VecModel::from(names)));
    ui.set_editor_folder_index(folder_index as i32);
    ui.set_editor_read_only(profile.read_only);
    ui.set_editor_security_index(index_of(&Security::ALL, profile.security));
    ui.set_editor_mechanism_index(index_of(&Mechanism::ALL, profile.mechanism));
    ui.set_editor_username(profile.username.as_str().into());
    ui.set_editor_password(password.unwrap_or_default().into());
    ui.set_editor_timeout(profile.request_timeout_ms.to_string().into());
    ui.set_editor_test_state(0);
    ui.set_editor_test_message(SharedString::new());

    s.editor = Some(Editor {
        profile_id: profile.id,
        hide_unreadable_topics: profile.hide_unreadable_topics,
        folder_ids,
        extra: profile.extra,
        test_generation: 0,
    });
    show_extra(ui, &s.editor.as_ref().unwrap().extra);
    ui.set_editor_open(true);
}

fn index_of<T: PartialEq>(all: &[T], value: T) -> i32 {
    all.iter().position(|v| *v == value).unwrap_or(0) as i32
}

/// The profile as currently filled in, plus the typed password.
fn profile_from_ui(ui: &AppWindow, editor: &Editor) -> (Profile, String) {
    let pick = |index: i32, len: usize| (index.max(0) as usize).min(len - 1);
    let profile = Profile {
        id: editor.profile_id.clone(),
        name: ui.get_editor_name().trim().to_string(),
        bootstrap_servers: ui.get_editor_servers().trim().to_string(),
        folder_id: editor.folder_ids[pick(ui.get_editor_folder_index(), editor.folder_ids.len())]
            .clone(),
        security: Security::ALL[pick(ui.get_editor_security_index(), Security::ALL.len())],
        mechanism: Mechanism::ALL[pick(ui.get_editor_mechanism_index(), Mechanism::ALL.len())],
        username: ui.get_editor_username().trim().to_string(),
        request_timeout_ms: ui
            .get_editor_timeout()
            .trim()
            .parse::<u64>()
            .unwrap_or(10_000)
            .max(1000),
        read_only: ui.get_editor_read_only(),
        hide_unreadable_topics: editor.hide_unreadable_topics,
        extra: editor
            .extra
            .iter()
            .filter(|p| !p.key.trim().is_empty())
            .map(|p| Property {
                key: p.key.trim().to_string(),
                value: p.value.trim().to_string(),
            })
            .collect(),
    };
    (profile, ui.get_editor_password().to_string())
}

/// Saves the profile and its password; returns its id so it can be
/// (re)connected with the new settings.
pub fn save(ui: &AppWindow, state: &Shared) -> Option<String> {
    let mut s = state.lock().unwrap();
    let editor = s.editor.as_ref()?;
    let (profile, password) = profile_from_ui(ui, editor);
    if profile.name.is_empty() || profile.bootstrap_servers.is_empty() {
        return None;
    }

    let password = profile.security.uses_sasl().then_some(password.as_str());
    secrets::set_password(&profile.id, password);
    let id = profile.id.clone();
    s.store.upsert(profile);
    s.store.save();
    s.editor = None;
    drop(s);

    close(ui);
    Some(id)
}

pub fn cancel(ui: &AppWindow, state: &Shared) {
    state.lock().unwrap().editor = None;
    close(ui);
}

fn close(ui: &AppWindow) {
    ui.set_editor_open(false);
    // Don't keep the password around in the UI.
    ui.set_editor_password(SharedString::new());
}

pub fn test(ui: &AppWindow, state: &Shared) {
    let (config, generation) = {
        let mut s = state.lock().unwrap();
        let Some(editor) = s.editor.as_mut() else {
            return;
        };
        editor.test_generation += 1;
        let generation = editor.test_generation;
        let (profile, password) = profile_from_ui(ui, editor);
        (profile.config(Some(password)), generation)
    };
    ui.set_editor_test_state(1);
    ui.set_editor_test_message("Connecting…".into());

    let (weak, state) = (ui.as_weak(), state.clone());
    std::thread::spawn(move || {
        let result =
            kafkalens_core::KafkaClient::new(config).and_then(|client| client.cluster_info());
        let _ = weak.upgrade_in_event_loop(move |ui| {
            let current = state
                .lock()
                .unwrap()
                .editor
                .as_ref()
                .map(|e| e.test_generation);
            if current != Some(generation) {
                return;
            }
            match result {
                Ok(info) => {
                    ui.set_editor_test_state(2);
                    ui.set_editor_test_message(
                        format!(
                            "Connected: {} broker(s), {} topics",
                            info.brokers.len(),
                            info.topic_count
                        )
                        .into(),
                    );
                }
                Err(err) => {
                    ui.set_editor_test_state(3);
                    ui.set_editor_test_message(err.to_string().into());
                }
            }
        });
    });
}

pub fn add_extra(ui: &AppWindow, state: &Shared) {
    let mut s = state.lock().unwrap();
    if let Some(editor) = s.editor.as_mut() {
        editor.extra.push(Property::default());
        show_extra(ui, &editor.extra);
    }
}

pub fn remove_extra(ui: &AppWindow, state: &Shared, index: usize) {
    let mut s = state.lock().unwrap();
    if let Some(editor) = s.editor.as_mut() {
        if index < editor.extra.len() {
            editor.extra.remove(index);
            show_extra(ui, &editor.extra);
        }
    }
}

/// Typing only updates the stored rows: rebuilding the model here would
/// recreate the text fields and drop their focus.
pub fn extra_edited(state: &Shared, index: usize, key: &str, value: &str) {
    let mut s = state.lock().unwrap();
    if let Some(row) = s.editor.as_mut().and_then(|e| e.extra.get_mut(index)) {
        row.key = key.to_string();
        row.value = value.to_string();
    }
}

fn show_extra(ui: &AppWindow, extra: &[Property]) {
    let items: Vec<PropertyItem> = extra
        .iter()
        .map(|p| PropertyItem {
            key: p.key.as_str().into(),
            value: p.value.as_str().into(),
        })
        .collect();
    ui.set_editor_extra(ModelRc::new(VecModel::from(items)));
}

pub fn open_prompt(ui: &AppWindow, state: &Shared, prompt: Prompt) {
    let mut s = state.lock().unwrap();
    let (heading, accept, value) = match &prompt {
        Prompt::NewFolder => ("New Folder", "Create", String::new()),
        Prompt::RenameFolder(id) => {
            let Some(folder) = s.store.folder(id) else {
                return;
            };
            ("Rename Folder", "Rename", folder.name.clone())
        }
    };
    ui.set_prompt_heading(heading.into());
    ui.set_prompt_accept_label(accept.into());
    ui.set_prompt_value(value.into());
    s.prompt = Some(prompt);
    ui.set_prompt_open(true);
}

pub fn accept_prompt(ui: &AppWindow, state: &Shared) {
    let mut s = state.lock().unwrap();
    let name = ui.get_prompt_value().trim().to_string();
    let Some(prompt) = s.prompt.take() else {
        return;
    };
    if !name.is_empty() {
        match prompt {
            Prompt::NewFolder => s.store.folders.push(Folder {
                name,
                ..Folder::default()
            }),
            Prompt::RenameFolder(id) => {
                if let Some(folder) = s.store.folder_mut(&id) {
                    folder.name = name;
                }
            }
        }
        s.store.save();
        sidebar::show_sidebar(ui, &s);
    }
    ui.set_prompt_open(false);
}

pub fn cancel_prompt(ui: &AppWindow, state: &Shared) {
    state.lock().unwrap().prompt = None;
    ui.set_prompt_open(false);
}
