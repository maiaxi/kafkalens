//! Fetching messages, the table (sort, search), the inspector (formats) and
//! the partitions tab.

use std::time::Instant;

use chrono::TimeZone;
use kafkalens_core::{format_payload, FetchRequest, KafkaMessage, PartitionInfo, PayloadFormat, StartPosition};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{AppWindow, HeaderItem, MessageDetail, MessageRow, PartitionRow, Shared, State};

const LIMITS: [u32; 5] = [50, 200, 500, 1000, 5000];
/// Order of the inspector's format picker.
const FORMATS: [PayloadFormat; 4] = [PayloadFormat::Auto, PayloadFormat::Text, PayloadFormat::Json, PayloadFormat::Hex];

/// Table order; fetches arrive newest first.
#[derive(Clone, Copy)]
pub struct Sort {
    pub column: i32,
    pub ascending: bool,
}

const TIMESTAMP_COLUMN: i32 = 2;

impl Default for Sort {
    fn default() -> Self {
        Sort { column: TIMESTAMP_COLUMN, ascending: false }
    }
}

/// Where to start reading, from the fetch bar.
fn start_position(ui: &AppWindow) -> Result<StartPosition, String> {
    let value = ui.get_start_value().trim().to_string();
    match ui.get_start_index() {
        1 => Ok(StartPosition::Beginning),
        2 => value
            .parse::<i64>()
            .map(|offset| StartPosition::Offset { offset })
            .map_err(|_| "Enter the offset to start from.".to_string()),
        3 => parse_local_time(&value)
            .map(|millis| StartPosition::Timestamp { millis })
            .ok_or_else(|| "Enter a time as YYYY-MM-DD HH:MM (local time).".to_string()),
        _ => Ok(StartPosition::Latest),
    }
}

/// "YYYY-MM-DD HH:MM[:SS]" or "YYYY-MM-DD" in local time, as epoch millis.
fn parse_local_time(text: &str) -> Option<i64> {
    let naive = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|f| chrono::NaiveDateTime::parse_from_str(text, f).ok())
        .or_else(|| chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0))?;
    chrono::Local.from_local_datetime(&naive).earliest().map(|t| t.timestamp_millis())
}

pub fn fetch(ui: &AppWindow, state: &Shared) {
    let (client, topic) = {
        let s = state.lock().unwrap();
        match (&s.client, &s.selected_topic) {
            (Some(client), Some(topic)) => (client.clone(), topic.clone()),
            _ => return,
        }
    };
    let start = match start_position(ui) {
        Ok(start) => start,
        Err(problem) => {
            ui.set_status_text(problem.into());
            return;
        }
    };
    let limit = LIMITS[(ui.get_limit_index().max(0) as usize).min(LIMITS.len() - 1)];
    // "All" is index 0; then one entry per partition id.
    let partition = ui.get_partition_index();
    let partitions = (partition > 0).then(|| vec![partition - 1]);

    ui.set_loading(true);
    ui.set_status_text(format!("Fetching {topic}…").into());

    let (weak, state) = (ui.as_weak(), state.clone());
    std::thread::spawn(move || {
        let began = Instant::now();
        let result = client.fetch_messages(FetchRequest {
            topic: topic.clone(),
            partitions,
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
                    s.messages = messages;
                    s.sort = Sort::default();
                    s.fetch_summary = format!(
                        "{} ms{}",
                        elapsed.as_millis(),
                        if result.timed_out { " · timed out, results may be partial" } else { "" }
                    );
                    show_messages(&ui, &mut s);
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
        ascending: if s.sort.column == column { !s.sort.ascending } else { true },
    };
    let sort = s.sort;
    sort_messages(&mut s.messages, sort);
    show_messages(ui, &mut s);
}

pub fn search(ui: &AppWindow, state: &Shared, query: &str) {
    let mut s = state.lock().unwrap();
    s.search = query.trim().to_lowercase();
    show_messages(ui, &mut s);
}

/// The message shown in table row `row`, which may be filtered by search.
pub fn message_at(s: &State, row: usize) -> Option<&KafkaMessage> {
    s.visible.get(row).and_then(|&index| s.messages.get(index))
}

pub fn select_row(ui: &AppWindow, state: &Shared, row: i32) {
    let mut s = state.lock().unwrap();
    s.selected_row = usize::try_from(row).ok();
    show_detail(ui, &s);
}

pub fn set_format(ui: &AppWindow, state: &Shared, index: i32) {
    let mut s = state.lock().unwrap();
    s.inspector_format = FORMATS[(index.max(0) as usize).min(FORMATS.len() - 1)];
    show_detail(ui, &s);
}

fn show_detail(ui: &AppWindow, s: &State) {
    let detail = s
        .selected_row
        .and_then(|row| message_at(s, row))
        .map(|m| message_detail(m, s.inspector_format))
        .unwrap_or_default();
    ui.set_detail(detail);
}

/// Rebuilds the table from the messages matching the search, clearing the
/// selection since rows may have moved.
pub fn show_messages(ui: &AppWindow, s: &mut State) {
    let query = s.search.clone();
    s.visible = s
        .messages
        .iter()
        .enumerate()
        .filter(|(_, m)| query.is_empty() || matches(m, &query))
        .map(|(index, _)| index)
        .collect();

    let rows: Vec<MessageRow> = s
        .visible
        .iter()
        .map(|&index| {
            let m = &s.messages[index];
            MessageRow {
                partition: m.partition.to_string().into(),
                offset: m.offset.to_string().into(),
                timestamp: format_timestamp(m.timestamp_ms).into(),
                key: preview(m.key.as_deref()).into(),
                value: preview(m.value.as_deref()).into(),
                size: format_size(m.value.as_ref().map_or(0, Vec::len)).into(),
                key_null: m.key.is_none(),
                value_null: m.value.is_none(),
            }
        })
        .collect();

    ui.set_message_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_sort_column(s.sort.column);
    ui.set_sort_ascending(s.sort.ascending);
    ui.set_current_row(-1);
    s.selected_row = None;
    ui.set_detail(MessageDetail::default());
    ui.set_status_text(status(s).into());
}

fn status(s: &State) -> String {
    if s.selected_topic.is_none() {
        return String::new();
    }
    let total = s.messages.len();
    let count = if s.search.is_empty() {
        format!("{total} message{}", if total == 1 { "" } else { "s" })
    } else {
        format!("{} of {total} messages", s.visible.len())
    };
    if s.fetch_summary.is_empty() {
        count
    } else {
        format!("{count} · {}", s.fetch_summary)
    }
}

/// Case-insensitive match on key, value or header names (query lowercased).
fn matches(m: &KafkaMessage, query: &str) -> bool {
    preview(m.key.as_deref()).to_lowercase().contains(query)
        || preview(m.value.as_deref()).to_lowercase().contains(query)
        || m.headers.iter().any(|h| h.key.to_lowercase().contains(query))
}

fn sort_messages(messages: &mut [KafkaMessage], sort: Sort) {
    match sort.column {
        0 => messages.sort_by_key(|m| (m.partition, m.offset)),
        1 => messages.sort_by_key(|m| m.offset),
        TIMESTAMP_COLUMN => messages.sort_by_key(|m| m.timestamp_ms),
        3 => messages.sort_by_cached_key(|m| preview(m.key.as_deref())),
        4 => messages.sort_by_cached_key(|m| preview(m.value.as_deref())),
        _ => messages.sort_by_key(|m| m.value.as_ref().map_or(0, Vec::len)),
    }
    if !sort.ascending {
        messages.reverse();
    }
}

fn message_detail(m: &KafkaMessage, format: PayloadFormat) -> MessageDetail {
    let (key, key_format) = render(m.key.as_deref(), format);
    let (value, value_format) = render(m.value.as_deref(), format);
    let schema_id = m
        .value
        .as_ref()
        .and_then(|v| format_payload(v.clone(), PayloadFormat::Auto, false).schema_id)
        .map(|id| SharedString::from(id.to_string()))
        .unwrap_or_default();
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
        size: format_size(m.value.as_ref().map_or(0, Vec::len)).into(),
        schema_id,
        key: key.into(),
        key_format: key_format.into(),
        key_null: m.key.is_none(),
        value: value.into(),
        value_format: value_format.into(),
        value_null: m.value.is_none(),
        headers: ModelRc::new(VecModel::from(headers)),
    }
}

/// Text for the inspector in `format`, plus a "JSON · 120 bytes" label
/// naming the format actually used (JSON falls back when it doesn't parse).
fn render(bytes: Option<&[u8]>, format: PayloadFormat) -> (String, String) {
    let Some(bytes) = bytes else {
        return (String::new(), String::new());
    };
    let formatted = format_payload(bytes.to_vec(), format, true);
    let name = match formatted.format {
        PayloadFormat::Json => "JSON",
        PayloadFormat::Hex => "Hex",
        _ => "Text",
    };
    (formatted.text, format!("{name} · {}", format_size(formatted.size_bytes as usize)))
}

/// One-line table cell.
fn preview(bytes: Option<&[u8]>) -> String {
    let Some(bytes) = bytes else {
        return "null".into();
    };
    let text = format_payload(bytes.to_vec(), PayloadFormat::Auto, false).text;
    text.chars().map(|c| if c == '\n' { ' ' } else { c }).take(200).collect()
}

fn format_size(bytes: usize) -> String {
    match bytes {
        0..1024 => format!("{bytes} bytes"),
        1024..1_048_576 => format!("{:.0} kB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

fn format_timestamp(millis: Option<i64>) -> String {
    millis
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S%.3f").to_string())
        .unwrap_or_else(|| "—".into())
}

/// Loads partition details for the Partitions tab and the partition picker.
pub fn load_partitions(ui: &AppWindow, state: &Shared, topic: &str) {
    let Some(client) = state.lock().unwrap().client.clone() else { return };
    let (weak, state, topic) = (ui.as_weak(), state.clone(), topic.to_string());
    std::thread::spawn(move || {
        let result = client.describe_topic(topic.clone());
        let _ = weak.upgrade_in_event_loop(move |ui| {
            let s = state.lock().unwrap();
            if s.selected_topic.as_deref() != Some(topic.as_str()) {
                return;
            }
            match result {
                Ok(details) => show_partitions(&ui, &details.partitions),
                Err(err) => ui.set_status_text(format!("Could not load partitions: {err}").into()),
            }
        });
    });
}

pub fn show_partitions(ui: &AppWindow, partitions: &[PartitionInfo]) {
    let list = |ids: &[i32]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ");
    let rows: Vec<PartitionRow> = partitions
        .iter()
        .map(|p| PartitionRow {
            id: p.id.to_string().into(),
            leader: p.leader.to_string().into(),
            replicas: list(&p.replicas).into(),
            in_sync: list(&p.in_sync_replicas).into(),
            under_replicated: p.in_sync_replicas.len() < p.replicas.len(),
            low: p.low_watermark.to_string().into(),
            high: p.high_watermark.to_string().into(),
            messages: (p.high_watermark - p.low_watermark).max(0).to_string().into(),
        })
        .collect();
    ui.set_partition_rows(ModelRc::new(VecModel::from(rows)));
}

/// Partition picker entries: "All", then each partition id.
pub fn set_partition_names(ui: &AppWindow, count: u32) {
    let mut names: Vec<SharedString> = vec!["All".into()];
    names.extend((0..count).map(|p| SharedString::from(p.to_string())));
    ui.set_partition_names(ModelRc::new(VecModel::from(names)));
    ui.set_partition_index(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_times() {
        let minute = parse_local_time("2026-10-14 21:30").unwrap();
        assert_eq!(parse_local_time("2026-10-14 21:30:00"), Some(minute));
        assert_eq!(parse_local_time("2026-10-14 21:30:59"), Some(minute + 59_000));
        assert!(parse_local_time("2026-10-14").unwrap() < minute);
        assert_eq!(parse_local_time("yesterday"), None);
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(790), "790 bytes");
        assert_eq!(format_size(2048), "2 kB");
        assert_eq!(format_size(3 * 1_048_576), "3.0 MB");
    }
}
