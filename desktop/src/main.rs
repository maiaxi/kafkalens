//! Kafkalens for Windows and Linux: a Slint UI over kafkalens-core, which it
//! calls directly (no bindings layer, unlike the macOS app). Core calls block
//! on the network, so they run on worker threads and post results back to
//! the UI thread.

mod editor;
mod messages;
mod produce;
mod secrets;
mod sidebar;
mod store;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use kafkalens_core::{KafkaClient, KafkaMessage, PayloadFormat, TopicSummary};

use crate::store::{Layout, Profile, Store};

slint::include_modules!();

pub struct State {
    pub store: Store,
    /// Global preference, saved with the layout.
    pub show_internal: bool,

    /// Profile id of the open connection.
    pub active: Option<String>,
    pub client: Option<Arc<KafkaClient>>,
    pub statuses: HashMap<String, sidebar::Status>,
    /// Bumped on every connect, so results from an older one are dropped.
    pub generation: u64,
    pub topics: Vec<TopicSummary>,
    pub topic_filter: String,
    pub selected_topic: Option<String>,
    /// In the table's current order.
    pub messages: Vec<KafkaMessage>,
    pub sort: messages::Sort,
    /// Lowercased search query; empty shows everything.
    pub search: String,
    /// Indices into `messages` of the rows the table shows.
    pub visible: Vec<usize>,
    /// Selected table row, to re-render it when the format changes.
    pub selected_row: Option<usize>,
    pub inspector_format: PayloadFormat,
    /// Timing and partial-result note of the last fetch, for the status bar.
    pub fetch_summary: String,

    pub editor: Option<editor::Editor>,
    pub prompt: Option<editor::Prompt>,
    pub produce: Option<produce::Produce>,
}

pub type Shared = Arc<Mutex<State>>;

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    ui.set_mono_font(monospace_font().into());

    let layout = Layout::load();
    apply_layout(&ui, &layout);

    let state: Shared = Arc::new(Mutex::new(State {
        store: Store::load(),
        show_internal: layout.show_internal_topics,
        active: None,
        client: None,
        statuses: HashMap::new(),
        generation: 0,
        topics: Vec::new(),
        topic_filter: String::new(),
        selected_topic: None,
        messages: Vec::new(),
        sort: messages::Sort::default(),
        search: String::new(),
        visible: Vec::new(),
        selected_row: None,
        inspector_format: PayloadFormat::Auto,
        fetch_summary: String::new(),
        editor: None,
        prompt: None,
        produce: None,
    }));
    sidebar::show_sidebar(&ui, &state.lock().unwrap());
    ui.set_status_text("Select a connection.".into());

    wire_sidebar(&ui, &state);
    wire_editor(&ui, &state);
    wire_topics_and_messages(&ui, &state);
    wire_produce(&ui, &state);

    open_from_env(&ui, &state);
    ui.run()?;

    let mut layout = current_layout(&ui);
    layout.show_internal_topics = state.lock().unwrap().show_internal;
    layout.save();
    Ok(())
}

/// Callbacks get a weak window handle and the shared state.
macro_rules! handler {
    ($ui:expr, $state:expr, |$w:ident, $s:ident $(, $arg:ident)*| $body:expr) => {{
        let (weak, state) = ($ui.as_weak(), $state.clone());
        move |$($arg),*| {
            let $w = weak.unwrap();
            let $s = &state;
            $body
        }
    }};
}

fn wire_sidebar(ui: &AppWindow, state: &Shared) {
    ui.on_select_connection(handler!(ui, state, |ui, state, id| sidebar::connect(
        &ui, state, &id
    )));

    ui.on_toggle_folder(handler!(ui, state, |ui, state, id| {
        let mut s = state.lock().unwrap();
        if let Some(folder) = s.store.folder_mut(&id) {
            folder.expanded = !folder.expanded;
        }
        s.store.save();
        sidebar::show_sidebar(&ui, &s);
    }));

    ui.on_new_connection(handler!(ui, state, |ui, state, folder| {
        let profile = Profile {
            folder_id: (!folder.is_empty()).then(|| folder.to_string()),
            ..Profile::default()
        };
        editor::open(&ui, state, profile, true);
    }));

    ui.on_edit_connection(handler!(ui, state, |ui, state, id| {
        let profile = state.lock().unwrap().store.profile(&id).cloned();
        if let Some(profile) = profile {
            editor::open(&ui, state, profile, false);
        }
    }));

    ui.on_delete_connection(handler!(ui, state, |ui, state, id| {
        let mut s = state.lock().unwrap();
        s.store.connections.retain(|c| c.id != id.as_str());
        s.store.save();
        secrets::set_password(&id, None);
        if s.active.as_deref() == Some(id.as_str()) {
            sidebar::disconnect(&ui, &mut s);
        }
        sidebar::show_sidebar(&ui, &s);
    }));

    ui.on_move_connection(handler!(ui, state, |ui, state, id, folder| {
        let mut s = state.lock().unwrap();
        if let Some(profile) = s.store.profile_mut(&id) {
            profile.folder_id = (!folder.is_empty()).then(|| folder.to_string());
        }
        s.store.save();
        sidebar::show_sidebar(&ui, &s);
    }));

    ui.on_new_folder(handler!(ui, state, |ui, state| editor::open_prompt(
        &ui,
        state,
        editor::Prompt::NewFolder
    )));
    ui.on_rename_folder(handler!(ui, state, |ui, state, id| {
        editor::open_prompt(&ui, state, editor::Prompt::RenameFolder(id.to_string()))
    }));

    ui.on_toggle_confirm(handler!(ui, state, |ui, state, id| {
        let mut s = state.lock().unwrap();
        if let Some(folder) = s.store.folder_mut(&id) {
            folder.confirms_produce = !folder.confirms_produce;
        }
        s.store.save();
        sidebar::show_sidebar(&ui, &s);
    }));

    ui.on_delete_folder(handler!(ui, state, |ui, state, id| {
        let mut s = state.lock().unwrap();
        s.store.delete_folder(&id);
        s.store.save();
        sidebar::show_sidebar(&ui, &s);
    }));

    ui.on_move_folder(handler!(ui, state, |ui, state, id, target, after| {
        let mut s = state.lock().unwrap();
        s.store.move_folder(&id, (!target.is_empty()).then_some(target.as_str()), after);
        s.store.save();
        sidebar::show_sidebar(&ui, &s);
    }));

    ui.on_shift_folder(handler!(ui, state, |ui, state, id, by| {
        let mut s = state.lock().unwrap();
        s.store.shift_folder(&id, by as isize);
        s.store.save();
        sidebar::show_sidebar(&ui, &s);
    }));

    ui.on_prompt_accept(handler!(ui, state, |ui, state| editor::accept_prompt(
        &ui, state
    )));
    ui.on_prompt_cancel(handler!(ui, state, |ui, state| editor::cancel_prompt(
        &ui, state
    )));
}

fn wire_editor(ui: &AppWindow, state: &Shared) {
    ui.on_editor_save(handler!(ui, state, |ui, state| {
        if let Some(id) = editor::save(&ui, state) {
            // Reconnect so the new settings apply.
            sidebar::connect(&ui, state, &id);
        }
    }));
    ui.on_editor_cancel(handler!(ui, state, |ui, state| editor::cancel(&ui, state)));
    ui.on_editor_test(handler!(ui, state, |ui, state| editor::test(&ui, state)));
    ui.on_editor_add_extra(handler!(ui, state, |ui, state| editor::add_extra(
        &ui, state
    )));
    ui.on_editor_remove_extra(handler!(ui, state, |ui, state, index| {
        editor::remove_extra(&ui, state, index.max(0) as usize)
    }));
    ui.on_editor_extra_edited(handler!(ui, state, |_ui, state, index, key, value| {
        editor::extra_edited(state, index.max(0) as usize, &key, &value)
    }));
}

fn wire_topics_and_messages(ui: &AppWindow, state: &Shared) {
    ui.on_topic_filter_changed(handler!(ui, state, |ui, state, filter| {
        let mut s = state.lock().unwrap();
        s.topic_filter = filter.to_string();
        sidebar::show_topics(&ui, &s);
    }));

    ui.on_show_internal_toggled(handler!(ui, state, |ui, state, on| {
        let mut s = state.lock().unwrap();
        s.show_internal = on;
        sidebar::show_topics(&ui, &s);
    }));

    ui.on_hide_unreadable_toggled(handler!(ui, state, |ui, state, on| {
        let mut s = state.lock().unwrap();
        let Some(id) = s.active.clone() else { return };
        if let Some(profile) = s.store.profile_mut(&id) {
            profile.hide_unreadable_topics = on;
        }
        s.store.save();
        sidebar::show_topics(&ui, &s);
    }));

    ui.on_search_changed(handler!(ui, state, |ui, state, query| messages::search(&ui, state, &query)));
    ui.on_format_changed(handler!(ui, state, |ui, state, index| messages::set_format(&ui, state, index)));
    ui.on_refresh_topics(handler!(ui, state, |ui, state| sidebar::refresh_topics(&ui, state)));

    ui.on_select_topic(handler!(ui, state, |ui, state, topic| {
        sidebar::select_topic(&ui, state, &topic)
    }));
    ui.on_fetch(handler!(ui, state, |ui, state| messages::fetch(&ui, state)));
    ui.on_select_message(handler!(ui, state, |ui, state, row| messages::select_row(
        &ui, state, row
    )));
    ui.on_sort_messages(handler!(ui, state, |ui, state, column| messages::sort_by(
        &ui, state, column
    )));
}

fn wire_produce(ui: &AppWindow, state: &Shared) {
    ui.on_produce_new(handler!(ui, state, |ui, state| produce::open_new(
        &ui, state
    )));
    ui.on_produce_duplicate(handler!(ui, state, |ui, state, row| {
        produce::open_duplicate(&ui, state, row.max(0) as usize)
    }));
    ui.on_produce_send(handler!(ui, state, |ui, state| produce::send(&ui, state)));
    ui.on_produce_confirm(handler!(ui, state, |ui, state| produce::confirm(
        &ui, state
    )));
    ui.on_produce_cancel_confirm(handler!(ui, state, |ui, _state| ui.set_produce_confirming(false)));
    ui.on_produce_close(handler!(ui, state, |ui, state| produce::close(&ui, state)));
    ui.on_produce_format_value(handler!(ui, state, |ui, _state| {
        produce::format_value(&ui);
        produce::validate(&ui);
    }));
    ui.on_produce_value_edited(handler!(ui, state, |ui, _state| produce::validate(&ui)));
    ui.on_produce_add_header(handler!(ui, state, |ui, state| produce::add_header(
        &ui, state
    )));
    ui.on_produce_remove_header(handler!(ui, state, |ui, state, index| {
        produce::remove_header(&ui, state, index.max(0) as usize)
    }));
    ui.on_produce_header_edited(handler!(ui, state, |_ui, state, index, key, value| {
        produce::header_edited(state, index.max(0) as usize, &key, &value)
    }));
}

/// Widths are clamped to the splitters' ranges in case the file was edited.
fn apply_layout(ui: &AppWindow, layout: &Layout) {
    ui.set_connections_width(layout.connections_width.clamp(160.0, 420.0));
    ui.set_topics_width(layout.topics_width.clamp(160.0, 520.0));
    ui.set_inspector_width(layout.inspector_width.clamp(240.0, 720.0));
    ui.set_show_connections(layout.show_connections);
    ui.set_show_topics(layout.show_topics);
    ui.set_show_inspector(layout.show_inspector);
    ui.set_show_internal(layout.show_internal_topics);
}

fn current_layout(ui: &AppWindow) -> Layout {
    Layout {
        connections_width: ui.get_connections_width(),
        topics_width: ui.get_topics_width(),
        inspector_width: ui.get_inspector_width(),
        show_connections: ui.get_show_connections(),
        show_topics: ui.get_show_topics(),
        show_inspector: ui.get_show_inspector(),
        show_internal_topics: ui.get_show_internal(),
    }
}

/// Debug builds only: KAFKALENS_CONNECT=<index> and KAFKALENS_TOPIC=<name>
/// open a connection and topic at startup, KAFKALENS_ROW=<n> shows that
/// message in the inspector (KAFKALENS_PRODUCE=open|send|confirm also
/// duplicates it into the produce sheet and goes that far), KAFKALENS_EDIT=<index> opens the editor on
/// that connection, KAFKALENS_TAB=1 opens the Partitions tab, and
/// KAFKALENS_SIZE=<w>x<h> sets the window size, to
/// iterate on the UI quickly.
fn open_from_env(ui: &AppWindow, state: &Shared) {
    if !cfg!(debug_assertions) {
        return;
    }
    if let Some((w, h)) = std::env::var("KAFKALENS_SIZE").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
    }) {
        ui.window().set_size(slint::LogicalSize::new(w, h));
    }
    if let Some(tab) = std::env::var("KAFKALENS_TAB").ok().and_then(|v| v.parse::<i32>().ok()) {
        ui.set_tab_index(tab);
    }
    if let Some(index) = std::env::var("KAFKALENS_EDIT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        let profile = state.lock().unwrap().store.connections.get(index).cloned();
        if let Some(profile) = profile {
            editor::open(ui, state, profile, false);
        }
    }
    let Some(index) = std::env::var("KAFKALENS_CONNECT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    else {
        return;
    };
    let Some(id) = state
        .lock()
        .unwrap()
        .store
        .connections
        .get(index)
        .map(|p| p.id.clone())
    else {
        return;
    };
    sidebar::connect(ui, state, &id);
    let Ok(topic) = std::env::var("KAFKALENS_TOPIC") else {
        return;
    };
    let row: Option<i32> = std::env::var("KAFKALENS_ROW")
        .ok()
        .and_then(|v| v.parse().ok());

    // Poll until the connection has listed the topic, then until its
    // messages have loaded.
    let (weak, state) = (ui.as_weak(), state.clone());
    let topic_opened = std::cell::Cell::new(false);
    let row_shown = std::cell::Cell::new(row.is_none());
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(200),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            if !topic_opened.get() {
                if state.lock().unwrap().topics.iter().any(|t| t.name == topic) {
                    topic_opened.set(true);
                    ui.invoke_select_topic(topic.as_str().into());
                }
            } else if let (false, Some(row)) = (row_shown.get(), row) {
                if state.lock().unwrap().messages.len() > row as usize {
                    row_shown.set(true);
                    ui.set_current_row(row);
                    ui.invoke_select_message(row);
                    if let Ok(step) = std::env::var("KAFKALENS_PRODUCE") {
                        ui.invoke_produce_duplicate(row);
                        if step == "send" || step == "confirm" {
                            ui.invoke_produce_send();
                        }
                        if step == "confirm" {
                            ui.invoke_produce_confirm();
                        }
                    }
                }
            }
        },
    );
    // Leaked on purpose: debug-only, and it must outlive this function.
    std::mem::forget(timer);
}

fn monospace_font() -> &'static str {
    if cfg!(target_os = "windows") {
        "Consolas"
    } else if cfg!(target_os = "macos") {
        "Menlo"
    } else {
        "DejaVu Sans Mono"
    }
}
