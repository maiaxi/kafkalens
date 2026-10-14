//! Fetching messages and showing them in the table and inspector.

use std::time::Instant;

use kafkalens_core::{format_payload, FetchRequest, KafkaMessage, PayloadFormat, StartPosition};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppWindow, HeaderItem, MessageDetail, MessageRow, Shared, State};

const LIMITS: [u32; 4] = [50, 200, 500, 1000];

/// Table order; fetches arrive newest first.
#[derive(Clone, Copy)]
pub struct Sort {
    pub column: i32,
    pub ascending: bool,
}

const TIMESTAMP_COLUMN: i32 = 2;

impl Default for Sort {
    fn default() -> Self {
        Sort {
            column: TIMESTAMP_COLUMN,
            ascending: false,
        }
    }
}

pub fn fetch(ui: &AppWindow, state: &Shared) {
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
                    let partial = if result.timed_out {
                        " · timed out, results may be partial"
                    } else {
                        ""
                    };
                    ui.set_status_text(
                        format!(
                            "{} messages · {} ms{partial}",
                            messages.len(),
                            elapsed.as_millis()
                        )
                        .into(),
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

/// Clicking the sorted column flips it; a new column starts ascending.
pub fn sort_by(ui: &AppWindow, state: &Shared, column: i32) {
    let mut s = state.lock().unwrap();
    s.sort = Sort {
        column,
        ascending: if s.sort.column == column {
            !s.sort.ascending
        } else {
            true
        },
    };
    let sort = s.sort;
    sort_messages(&mut s.messages, sort);
    show_messages(ui, &s);
}

pub fn select_row(ui: &AppWindow, state: &Shared, row: i32) {
    let s = state.lock().unwrap();
    let detail = usize::try_from(row)
        .ok()
        .and_then(|row| s.messages.get(row))
        .map(message_detail)
        .unwrap_or_default();
    ui.set_detail(detail);
}

/// Rebuilds the table, clearing the selection since rows may have moved.
pub fn show_messages(ui: &AppWindow, s: &State) {
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
    let Some(bytes) = bytes else {
        return "null".into();
    };
    let text = format_payload(bytes.to_vec(), PayloadFormat::Auto, false).text;
    let flat: String = text
        .chars()
        .map(|c| if c == '\n' { ' ' } else { c })
        .take(200)
        .collect();
    flat
}

fn format_timestamp(millis: Option<i64>) -> String {
    millis
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string()
        })
        .unwrap_or_else(|| "—".into())
}
