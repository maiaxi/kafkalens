//! The produce sheet: filling it (new or from a message), validating,
//! confirming for protected folders, and sending.

use kafkalens_core::{
    editable_payload, encode_payload, format_payload, InputEncoding, KafkaMessage, MessageHeader,
    PayloadFormat, ProduceRequest,
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::store::Property;
use crate::{messages, AppWindow, PropertyItem, Shared, State};

/// Order of the encoding combo boxes.
const ENCODINGS: [InputEncoding; 3] =
    [InputEncoding::Text, InputEncoding::Json, InputEncoding::Hex];

/// The open sheet.
pub struct Produce {
    topic: String,
    /// Source of truth for the header rows, which the UI only displays.
    headers: Vec<Property>,
    sent: u32,
}

/// Form contents to open the sheet with.
struct Draft {
    partition: Option<i32>,
    key: String,
    key_encoding: InputEncoding,
    tombstone: bool,
    value: String,
    value_encoding: InputEncoding,
    headers: Vec<Property>,
}

impl Default for Draft {
    fn default() -> Self {
        Draft {
            partition: None,
            key: String::new(),
            key_encoding: InputEncoding::Text,
            tombstone: false,
            value: String::new(),
            value_encoding: InputEncoding::Json,
            headers: Vec::new(),
        }
    }
}

impl Draft {
    /// Pre-filled from an existing message, for "Duplicate and Edit".
    fn duplicating(message: &KafkaMessage) -> Draft {
        let key = message.key.clone().map(|k| editable_payload(k, false));
        let value = message.value.clone().map(|v| editable_payload(v, true));
        Draft {
            partition: Some(message.partition),
            key: key.as_ref().map(|k| k.text.clone()).unwrap_or_default(),
            key_encoding: key.map_or(InputEncoding::Text, |k| k.encoding),
            tombstone: value.is_none(),
            value: value.as_ref().map(|v| v.text.clone()).unwrap_or_default(),
            value_encoding: value.map_or(InputEncoding::Json, |v| v.encoding),
            headers: message
                .headers
                .iter()
                .map(|h| Property {
                    key: h.key.clone(),
                    value: h
                        .value
                        .as_deref()
                        .map(|v| String::from_utf8_lossy(v).into_owned())
                        .unwrap_or_default(),
                })
                .collect(),
        }
    }
}

/// Producing needs a writable connection and a topic the ACLs don't deny.
/// Sets the flags the toolbar and table use.
pub fn update_flags(ui: &AppWindow, s: &State) {
    let writable = s.client.is_some() && active_profile_writable(s);
    let topic_writable = s
        .selected_topic
        .as_deref()
        .and_then(|name| s.topics.iter().find(|t| t.name == name))
        .is_none_or(|t| t.can_write != Some(false));
    ui.set_connection_writable(writable);
    ui.set_topic_writable(topic_writable);
}

fn active_profile_writable(s: &State) -> bool {
    s.active
        .as_deref()
        .and_then(|id| s.store.profile(id))
        .is_some_and(|p| !p.read_only)
}

/// Name of the folder that asks before producing, if any.
fn confirm_folder(s: &State) -> Option<String> {
    let profile = s.store.profile(s.active.as_deref()?)?;
    let folder = s.store.folder(profile.folder_id.as_deref()?)?;
    folder.confirms_produce.then(|| folder.name.clone())
}

pub fn open_new(ui: &AppWindow, state: &Shared) {
    open(ui, state, Draft::default());
}

pub fn open_duplicate(ui: &AppWindow, state: &Shared, row: usize) {
    // `row` is a table row, which may be filtered by the search.
    let draft = messages::message_at(&state.lock().unwrap(), row).map(Draft::duplicating);
    if let Some(draft) = draft {
        open(ui, state, draft);
    }
}

fn open(ui: &AppWindow, state: &Shared, draft: Draft) {
    let mut s = state.lock().unwrap();
    let Some(topic) = s.selected_topic.clone() else {
        return;
    };
    if !active_profile_writable(&s) {
        return;
    }

    let partition_count = s
        .topics
        .iter()
        .find(|t| t.name == topic)
        .map_or(0, |t| t.partition_count);
    let mut names: Vec<SharedString> = vec!["Automatic".into()];
    names.extend((0..partition_count).map(|p| SharedString::from(p.to_string())));
    let partition_index = draft
        .partition
        .filter(|p| (*p as u32) < partition_count)
        .map_or(0, |p| p + 1);

    let profile_name = s
        .active
        .as_deref()
        .and_then(|id| s.store.profile(id))
        .map(|p| p.name.clone());
    ui.set_produce_connection(profile_name.unwrap_or_default().into());
    ui.set_produce_confirm_folder(confirm_folder(&s).unwrap_or_default().into());
    ui.set_produce_partition_names(ModelRc::new(VecModel::from(names)));
    ui.set_produce_partition_index(partition_index);
    ui.set_produce_key(draft.key.into());
    ui.set_produce_key_encoding_index(encoding_index(draft.key_encoding));
    ui.set_produce_tombstone(draft.tombstone);
    ui.set_produce_value(draft.value.into());
    ui.set_produce_value_encoding_index(encoding_index(draft.value_encoding));
    ui.set_produce_sending(false);
    ui.set_produce_confirming(false);
    ui.set_produce_result(SharedString::new());
    ui.set_produce_sent_count(0);

    show_headers(ui, &draft.headers);
    s.produce = Some(Produce {
        topic,
        headers: draft.headers,
        sent: 0,
    });
    drop(s);

    validate(ui);
    ui.set_produce_open(true);
}

fn encoding_index(encoding: InputEncoding) -> i32 {
    ENCODINGS.iter().position(|e| *e == encoding).unwrap_or(0) as i32
}

fn encoding(index: i32) -> InputEncoding {
    ENCODINGS[(index.max(0) as usize).min(ENCODINGS.len() - 1)]
}

/// Shows why the value can't be sent as typed, or nothing when it can.
pub fn validate(ui: &AppWindow) {
    let problem = if ui.get_produce_tombstone() {
        String::new()
    } else {
        let value = ui.get_produce_value().to_string();
        let encoding = encoding(ui.get_produce_value_encoding_index());
        if value.trim().is_empty() && encoding == InputEncoding::Json {
            "Enter a JSON value, or switch the encoding to Text.".into()
        } else {
            encode_payload(value, encoding, "Value".into())
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default()
        }
    };
    ui.set_produce_value_problem(problem.into());
}

/// Re-indents the value when it is valid JSON.
pub fn format_value(ui: &AppWindow) {
    if encoding(ui.get_produce_value_encoding_index()) != InputEncoding::Json {
        return;
    }
    let formatted = format_payload(
        ui.get_produce_value().as_bytes().to_vec(),
        PayloadFormat::Json,
        true,
    );
    if formatted.format == PayloadFormat::Json {
        ui.set_produce_value(formatted.text.into());
    }
}

fn request(ui: &AppWindow, produce: &Produce) -> Result<ProduceRequest, String> {
    let key = ui.get_produce_key().to_string();
    let key = if key.is_empty() {
        None
    } else {
        Some(
            encode_payload(
                key,
                encoding(ui.get_produce_key_encoding_index()),
                "Key".into(),
            )
            .map_err(|e| e.to_string())?,
        )
    };
    let value = if ui.get_produce_tombstone() {
        None
    } else {
        let value = ui.get_produce_value().to_string();
        Some(
            encode_payload(
                value,
                encoding(ui.get_produce_value_encoding_index()),
                "Value".into(),
            )
            .map_err(|e| e.to_string())?,
        )
    };
    let headers = produce
        .headers
        .iter()
        .map(|h| {
            let key = h.key.trim();
            if key.is_empty() {
                return Err("Every header needs a key.".to_string());
            }
            Ok(MessageHeader {
                key: key.to_string(),
                value: Some(h.value.as_bytes().to_vec()),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let partition = ui.get_produce_partition_index();

    Ok(ProduceRequest {
        topic: produce.topic.clone(),
        partition: (partition > 0).then_some(partition - 1),
        key,
        value,
        headers,
    })
}

pub fn send(ui: &AppWindow, state: &Shared) {
    let needs_confirmation = {
        let s = state.lock().unwrap();
        let Some(produce) = s.produce.as_ref() else {
            return;
        };
        if let Err(problem) = request(ui, produce) {
            show_result(ui, false, &problem);
            return;
        }
        confirm_folder(&s).is_some()
    };
    if needs_confirmation {
        ui.set_produce_confirming(true);
    } else {
        do_send(ui, state);
    }
}

pub fn confirm(ui: &AppWindow, state: &Shared) {
    ui.set_produce_confirming(false);
    do_send(ui, state);
}

fn do_send(ui: &AppWindow, state: &Shared) {
    let (client, request) = {
        let s = state.lock().unwrap();
        let (Some(client), Some(produce)) = (s.client.clone(), s.produce.as_ref()) else {
            return;
        };
        match request(ui, produce) {
            Ok(request) => (client, request),
            Err(problem) => {
                show_result(ui, false, &problem);
                return;
            }
        }
    };
    ui.set_produce_sending(true);
    show_result(ui, true, "");

    let (weak, state) = (ui.as_weak(), state.clone());
    std::thread::spawn(move || {
        let result = client.produce_message(request);
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_produce_sending(false);
            let mut s = state.lock().unwrap();
            let Some(produce) = s.produce.as_mut() else {
                return;
            };
            match result {
                Ok(sent) => {
                    produce.sent += 1;
                    ui.set_produce_sent_count(produce.sent as i32);
                    show_result(
                        &ui,
                        true,
                        &format!(
                            "Sent to partition {}, offset {}",
                            sent.partition, sent.offset
                        ),
                    );
                }
                Err(err) => show_result(&ui, false, &err.to_string()),
            }
        });
    });
}

fn show_result(ui: &AppWindow, ok: bool, message: &str) {
    ui.set_produce_result_ok(ok);
    ui.set_produce_result(message.into());
}

/// Closes the sheet; refreshes the messages if anything was sent.
pub fn close(ui: &AppWindow, state: &Shared) {
    let sent = state.lock().unwrap().produce.take().map_or(0, |p| p.sent);
    ui.set_produce_open(false);
    if sent > 0 {
        messages::fetch(ui, state);
    }
}

pub fn add_header(ui: &AppWindow, state: &Shared) {
    let mut s = state.lock().unwrap();
    if let Some(produce) = s.produce.as_mut() {
        produce.headers.push(Property::default());
        show_headers(ui, &produce.headers);
    }
}

pub fn remove_header(ui: &AppWindow, state: &Shared, index: usize) {
    let mut s = state.lock().unwrap();
    if let Some(produce) = s.produce.as_mut() {
        if index < produce.headers.len() {
            produce.headers.remove(index);
            show_headers(ui, &produce.headers);
        }
    }
}

/// Typing only updates the stored rows, so the text fields keep focus.
pub fn header_edited(state: &Shared, index: usize, key: &str, value: &str) {
    let mut s = state.lock().unwrap();
    if let Some(header) = s.produce.as_mut().and_then(|p| p.headers.get_mut(index)) {
        header.key = key.to_string();
        header.value = value.to_string();
    }
}

fn show_headers(ui: &AppWindow, headers: &[Property]) {
    let items: Vec<PropertyItem> = headers
        .iter()
        .map(|h| PropertyItem {
            key: h.key.as_str().into(),
            value: h.value.as_str().into(),
        })
        .collect();
    ui.set_produce_headers(ModelRc::new(VecModel::from(items)));
}
