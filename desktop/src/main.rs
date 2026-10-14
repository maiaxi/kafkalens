//! Kafkalens for Windows and Linux: a Slint UI over kafkalens-core, which it
//! calls directly (no bindings layer, unlike the macOS app). Core calls block
//! on the network, so they run on worker threads and post results back to
//! the UI thread.

mod connections;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use kafkalens_core::{
    format_payload, ConnectionConfig, FetchRequest, KafkaClient, KafkaMessage, PayloadFormat,
    SecurityProtocol, StartPosition, TopicSummary,
};
use slint::{ModelRc, SharedString, VecModel};

use crate::connections::SavedConnection;

slint::include_modules!();

const LIMITS: [u32; 4] = [50, 200, 500, 1000];

#[derive(Default)]
struct State {
    connections: Vec<SavedConnection>,
    client: Option<Arc<KafkaClient>>,
    /// Bumped on every connect, so results from an older one are dropped.
    connection_generation: u64,
    topics: Vec<TopicSummary>,
    topic_filter: String,
    selected_topic: Option<String>,
    /// In the table's current order.
    messages: Vec<KafkaMessage>,
    sort: Sort,
}

/// Table order; fetches arrive newest first.
#[derive(Clone, Copy)]
struct Sort {
    column: i32,
    ascending: bool,
}

const TIMESTAMP_COLUMN: i32 = 2;

impl Default for Sort {
    fn default() -> Self {
        Sort { column: TIMESTAMP_COLUMN, ascending: false }
    }
}

type Shared = Arc<Mutex<State>>;

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    ui.set_mono_font(monospace_font().into());
    apply_layout(&ui, &connections::load_layout());

    let state: Shared = Arc::new(Mutex::new(State {
        connections: connections::load(),
        ..State::default()
    }));
    show_connections(&ui, &state.lock().unwrap());
    ui.set_status_text("Select a connection.".into());

    ui.on_select_connection({
        let (weak, state) = (ui.as_weak(), state.clone());
        move |index| connect(&weak.unwrap(), &state, index as usize)
    });

    ui.on_add_connection({
        let (weak, state) = (ui.as_weak(), state.clone());
        move |name, servers| {
            let ui = weak.unwrap();
            let index = {
                let mut s = state.lock().unwrap();
                s.connections.push(SavedConnection {
                    name: name.trim().to_string(),
                    bootstrap_servers: servers.trim().to_string(),
                });
                connections::save(&s.connections);
                show_connections(&ui, &s);
                s.connections.len() - 1
            };
            connect(&ui, &state, index);
        }
    });

    ui.on_topic_filter_changed({
        let (weak, state) = (ui.as_weak(), state.clone());
        move |filter| {
            let mut s = state.lock().unwrap();
            s.topic_filter = filter.to_string();
            show_topics(&weak.unwrap(), &s);
        }
    });

    ui.on_select_topic({
        let (weak, state) = (ui.as_weak(), state.clone());
        move |topic| {
            let ui = weak.unwrap();
            {
                let mut s = state.lock().unwrap();
                s.selected_topic = Some(topic.to_string());
                s.messages.clear();
                show_messages(&ui, &s);
            }
            ui.set_selected_topic(topic);
            ui.set_detail(MessageDetail::default());
            fetch(&ui, &state);
        }
    });

    ui.on_fetch({
        let (weak, state) = (ui.as_weak(), state.clone());
        move || fetch(&weak.unwrap(), &state)
    });

    ui.on_select_message({
        let (weak, state) = (ui.as_weak(), state.clone());
        move |row| {
            let s = state.lock().unwrap();
            let detail = usize::try_from(row)
                .ok()
                .and_then(|row| s.messages.get(row))
                .map(message_detail)
                .unwrap_or_default();
            weak.unwrap().set_detail(detail);
        }
    });

    ui.on_sort_messages({
        let (weak, state) = (ui.as_weak(), state.clone());
        move |column| {
            let mut s = state.lock().unwrap();
            // Clicking the sorted column flips it; a new column starts ascending.
            s.sort = Sort {
                column,
                ascending: if s.sort.column == column { !s.sort.ascending } else { true },
            };
            let sort = s.sort;
            sort_messages(&mut s.messages, sort);
            show_messages(&weak.unwrap(), &s);
        }
    });

    open_from_env(&ui, &state);
    ui.run()?;
    connections::save_layout(&current_layout(&ui));
    Ok(())
}

/// Widths are clamped to the splitters' ranges in case the file was edited.
fn apply_layout(ui: &AppWindow, layout: &connections::Layout) {
    ui.set_connections_width(layout.connections_width.clamp(160.0, 420.0));
    ui.set_topics_width(layout.topics_width.clamp(160.0, 520.0));
    ui.set_inspector_width(layout.inspector_width.clamp(240.0, 720.0));
    ui.set_show_connections(layout.show_connections);
    ui.set_show_topics(layout.show_topics);
    ui.set_show_inspector(layout.show_inspector);
}

fn current_layout(ui: &AppWindow) -> connections::Layout {
    connections::Layout {
        connections_width: ui.get_connections_width(),
        topics_width: ui.get_topics_width(),
        inspector_width: ui.get_inspector_width(),
        show_connections: ui.get_show_connections(),
        show_topics: ui.get_show_topics(),
        show_inspector: ui.get_show_inspector(),
    }
}

/// Debug builds only: KAFKALENS_CONNECT=<index> and KAFKALENS_TOPIC=<name>
/// open a connection and topic at startup, and KAFKALENS_ROW=<n> shows that
/// message in the inspector, to iterate on the UI quickly.
fn open_from_env(ui: &AppWindow, state: &Shared) {
    if !cfg!(debug_assertions) {
        return;
    }
    let Some(index) = std::env::var("KAFKALENS_CONNECT").ok().and_then(|v| v.parse().ok()) else {
        return;
    };
    connect(ui, state, index);
    let Ok(topic) = std::env::var("KAFKALENS_TOPIC") else { return };

    let row: Option<i32> = std::env::var("KAFKALENS_ROW").ok().and_then(|v| v.parse().ok());

    // Poll until the connection has listed the topic, then until its
    // messages have loaded.
    let (weak, state) = (ui.as_weak(), state.clone());
    let topic_opened = std::cell::Cell::new(false);
    let row_shown = std::cell::Cell::new(row.is_none());
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(200), move || {
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
            }
        }
    });
    // Leaked on purpose: debug-only, and it must outlive this function.
    std::mem::forget(timer);
}

fn connect(ui: &AppWindow, state: &Shared, index: usize) {
    let (config, generation) = {
        let mut s = state.lock().unwrap();
        let Some(saved) = s.connections.get(index) else { return };
        let config = ConnectionConfig {
            bootstrap_servers: saved.bootstrap_servers.clone(),
            security_protocol: SecurityProtocol::Plaintext,
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            request_timeout_ms: 10_000,
            read_only: true,
            extra: vec![],
        };
        s.connection_generation += 1;
        s.client = None;
        s.topics.clear();
        s.selected_topic = None;
        s.messages.clear();
        show_topics(ui, &s);
        show_messages(ui, &s);
        (config, s.connection_generation)
    };

    ui.set_selected_connection(index as i32);
    ui.set_selected_topic(SharedString::new());
    ui.set_detail(MessageDetail::default());
    ui.set_connection_status(format!("Connecting to {}…", config.bootstrap_servers).into());

    let (weak, state) = (ui.as_weak(), state.clone());
    std::thread::spawn(move || {
        let result = KafkaClient::new(config).and_then(|client| {
            let info = client.cluster_info()?;
            let topics = client.list_topics()?;
            Ok((client, info, topics))
        });

        let _ = weak.upgrade_in_event_loop(move |ui| {
            let mut s = state.lock().unwrap();
            if s.connection_generation != generation {
                return;
            }
            match result {
                Ok((client, info, topics)) => {
                    ui.set_connection_status(
                        format!("{} broker(s) · {} topics", info.brokers.len(), topics.len()).into(),
                    );
                    s.client = Some(client);
                    s.topics = topics;
                    show_topics(&ui, &s);
                    ui.set_status_text("Select a topic.".into());
                }
                Err(err) => ui.set_connection_status(format!("Connection failed: {err}").into()),
            }
        });
    });
}

fn fetch(ui: &AppWindow, state: &Shared) {
    let (client, topic) = {
        let s = state.lock().unwrap();
        match (&s.client, &s.selected_topic) {
            (Some(client), Some(topic)) => (client.clone(), topic.clone()),
            _ => return,
        }
    };
    let start = if ui.get_start_index() == 1 {
        StartPosition::Beginning
    } else {
        StartPosition::Latest
    };
    let limit = LIMITS[(ui.get_limit_index().max(0) as usize).min(LIMITS.len() - 1)];

    ui.set_loading(true);
    ui.set_status_text(format!("Fetching {topic}…").into());

    let (weak, state) = (ui.as_weak(), state.clone());
    std::thread::spawn(move || {
        let began = Instant::now();
        let result = client.fetch_messages(FetchRequest {
            topic: topic.clone(),
            partitions: None,
            start,
            max_messages: limit,
            timeout_ms: 15_000,
        });
        let elapsed = began.elapsed();

        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_loading(false);
            let mut s = state.lock().unwrap();
            // The user may have moved to another topic meanwhile.
            if s.selected_topic.as_deref() != Some(topic.as_str()) {
                return;
            }
            match result {
                Ok(result) => {
                    let mut messages = result.messages;
                    messages.reverse();
                    let partial = if result.timed_out { " · timed out, results may be partial" } else { "" };
                    ui.set_status_text(
                        format!("{} messages · {} ms{partial}", messages.len(), elapsed.as_millis()).into(),
                    );
                    s.messages = messages;
                    s.sort = Sort::default();
                    show_messages(&ui, &s);
                }
                Err(err) => ui.set_status_text(format!("Fetch failed: {err}").into()),
            }
        });
    });
}

fn show_connections(ui: &AppWindow, s: &State) {
    let items: Vec<ConnectionItem> = s
        .connections
        .iter()
        .map(|c| ConnectionItem {
            name: c.name.as_str().into(),
            bootstrap: c.bootstrap_servers.as_str().into(),
        })
        .collect();
    ui.set_connections(ModelRc::new(VecModel::from(items)));
}

fn show_topics(ui: &AppWindow, s: &State) {
    let filter = s.topic_filter.to_lowercase();
    let mut hidden = 0;
    let items: Vec<TopicItem> = s
        .topics
        .iter()
        .filter(|t| {
            let unreadable = t.can_read == Some(false);
            hidden += unreadable as usize;
            !t.is_internal && !unreadable
        })
        .filter(|t| filter.is_empty() || t.name.to_lowercase().contains(&filter))
        .map(|t| TopicItem {
            name: t.name.as_str().into(),
            partitions: t.partition_count as i32,
            internal: t.is_internal,
        })
        .collect();
    ui.set_topics(ModelRc::new(VecModel::from(items)));
    ui.set_hidden_topics_note(if hidden > 0 {
        format!("{hidden} without read access hidden").into()
    } else {
        SharedString::new()
    });
}

/// Rebuilds the table, clearing the selection since rows may have moved.
fn show_messages(ui: &AppWindow, s: &State) {
    let rows: Vec<MessageRow> = s
        .messages
        .iter()
        .map(|m| MessageRow {
            partition: m.partition.to_string().into(),
            offset: m.offset.to_string().into(),
            timestamp: format_timestamp(m.timestamp_ms).into(),
            key: preview(m.key.as_deref()).into(),
            value: preview(m.value.as_deref()).into(),
            key_null: m.key.is_none(),
            value_null: m.value.is_none(),
        })
        .collect();
    ui.set_message_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_sort_column(s.sort.column);
    ui.set_sort_ascending(s.sort.ascending);
    ui.set_current_row(-1);
    ui.set_detail(MessageDetail::default());
}

fn sort_messages(messages: &mut [KafkaMessage], sort: Sort) {
    match sort.column {
        0 => messages.sort_by_key(|m| (m.partition, m.offset)),
        1 => messages.sort_by_key(|m| m.offset),
        TIMESTAMP_COLUMN => messages.sort_by_key(|m| m.timestamp_ms),
        3 => messages.sort_by_cached_key(|m| preview(m.key.as_deref())),
        _ => messages.sort_by_cached_key(|m| preview(m.value.as_deref())),
    }
    if !sort.ascending {
        messages.reverse();
    }
}

fn message_detail(m: &KafkaMessage) -> MessageDetail {
    let (key, key_format) = render(m.key.as_deref());
    let (value, value_format) = render(m.value.as_deref());
    let headers: Vec<HeaderItem> = m
        .headers
        .iter()
        .map(|h| HeaderItem {
            key: h.key.as_str().into(),
            value: h
                .value
                .as_ref()
                .map(|v| format_payload(v.clone(), PayloadFormat::Auto, false).text)
                .unwrap_or_else(|| "null".into())
                .into(),
        })
        .collect();

    MessageDetail {
        present: true,
        partition: m.partition.to_string().into(),
        offset: m.offset.to_string().into(),
        timestamp: format_timestamp(m.timestamp_ms).into(),
        key: key.into(),
        key_format: key_format.into(),
        value: value.into(),
        value_format: value_format.into(),
        size: format!("{} bytes", m.value.as_ref().map_or(0, Vec::len)).into(),
        headers: ModelRc::new(VecModel::from(headers)),
    }
}

/// Pretty text for the inspector plus a "JSON · 120 bytes" style label.
fn render(bytes: Option<&[u8]>) -> (String, String) {
    let Some(bytes) = bytes else {
        return ("null".into(), String::new());
    };
    let formatted = format_payload(bytes.to_vec(), PayloadFormat::Auto, true);
    let format = match formatted.format {
        PayloadFormat::Json => "JSON",
        PayloadFormat::Hex => "Hex",
        _ => "Text",
    };
    let mut label = format!("{format} · {} bytes", formatted.size_bytes);
    if let Some(id) = formatted.schema_id {
        label.push_str(&format!(" · schema {id}"));
    }
    (formatted.text, label)
}

/// One-line table cell.
fn preview(bytes: Option<&[u8]>) -> String {
    let Some(bytes) = bytes else { return "null".into() };
    let text = format_payload(bytes.to_vec(), PayloadFormat::Auto, false).text;
    let flat: String = text.chars().map(|c| if c == '\n' { ' ' } else { c }).take(200).collect();
    flat
}

fn format_timestamp(millis: Option<i64>) -> String {
    millis
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S%.3f").to_string())
        .unwrap_or_else(|| "—".into())
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
