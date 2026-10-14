//! The connections sidebar, connecting to a cluster, and its topic list.

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::store::Profile;
use crate::{
    messages, secrets, AppWindow, FolderItem, MessageDetail, RowKind, Shared, SidebarRow, State,
    TopicItem,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Connecting,
    Connected,
    Failed,
}

pub fn show_sidebar(ui: &AppWindow, s: &State) {
    let mut rows = Vec::new();
    let folders = s.store.sorted_folders();

    for folder in &folders {
        rows.push(SidebarRow {
            kind: RowKind::Folder,
            id: folder.id.as_str().into(),
            name: folder.name.as_str().into(),
            expanded: folder.expanded,
            protected: folder.confirms_produce,
            ..SidebarRow::default()
        });
        if folder.expanded {
            rows.extend(
                s.store
                    .connections_in(Some(&folder.id))
                    .into_iter()
                    .map(|p| connection_row(s, p, true)),
            );
        }
    }

    let ungrouped = s.store.connections_in(None);
    if !ungrouped.is_empty() || folders.is_empty() {
        rows.push(SidebarRow {
            kind: RowKind::Heading,
            name: "CONNECTIONS".into(),
            ..SidebarRow::default()
        });
        rows.extend(ungrouped.into_iter().map(|p| connection_row(s, p, false)));
    }

    let folder_items: Vec<FolderItem> = folders
        .iter()
        .map(|f| FolderItem {
            id: f.id.as_str().into(),
            name: f.name.as_str().into(),
        })
        .collect();

    ui.set_sidebar_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_folders(ModelRc::new(VecModel::from(folder_items)));
    ui.set_selected_connection(s.active.as_deref().unwrap_or_default().into());
}

fn connection_row(s: &State, profile: &Profile, indented: bool) -> SidebarRow {
    let status = match s.statuses.get(&profile.id) {
        None => 0,
        Some(Status::Connecting) => 1,
        Some(Status::Connected) => 2,
        Some(Status::Failed) => 3,
    };
    SidebarRow {
        kind: RowKind::Connection,
        id: profile.id.as_str().into(),
        name: profile.name.as_str().into(),
        subtitle: profile.bootstrap_servers.as_str().into(),
        indented,
        writable: !profile.read_only,
        status,
        folder_id: profile.folder_id.as_deref().unwrap_or_default().into(),
        ..SidebarRow::default()
    }
}

/// Opens a connection to `id`, replacing the current one.
pub fn connect(ui: &AppWindow, state: &Shared, id: &str) {
    let (config, generation, servers, hide_unreadable) = {
        let mut s = state.lock().unwrap();
        let Some(profile) = s.store.profile(id).cloned() else {
            return;
        };
        let password = if profile.security.uses_sasl() {
            secrets::password(&profile.id)
        } else {
            None
        };

        clear_session(&mut s);
        s.generation += 1;
        s.active = Some(profile.id.clone());
        s.statuses.insert(profile.id.clone(), Status::Connecting);
        show_sidebar(ui, &s);
        show_topics(ui, &s);
        messages::show_messages(ui, &s);
        (
            profile.config(password),
            s.generation,
            profile.bootstrap_servers,
            profile.hide_unreadable_topics,
        )
    };

    ui.set_selected_topic(SharedString::new());
    ui.set_connected(false);
    ui.set_hide_unreadable(hide_unreadable);
    ui.set_connection_status(format!("Connecting to {servers}…").into());
    ui.set_status_text(SharedString::new());

    let (weak, state, id) = (ui.as_weak(), state.clone(), id.to_string());
    std::thread::spawn(move || {
        let result = kafkalens_core::KafkaClient::new(config).and_then(|client| {
            let info = client.cluster_info()?;
            let topics = client.list_topics()?;
            Ok((client, info, topics))
        });

        let _ = weak.upgrade_in_event_loop(move |ui| {
            let mut s = state.lock().unwrap();
            if s.generation != generation {
                return;
            }
            match result {
                Ok((client, info, topics)) => {
                    ui.set_connection_status(
                        format!("{} broker(s) · {} topics", info.brokers.len(), topics.len())
                            .into(),
                    );
                    s.statuses.insert(id, Status::Connected);
                    s.client = Some(client);
                    s.topics = topics;
                    ui.set_connected(true);
                    ui.set_status_text("Select a topic.".into());
                }
                Err(err) => {
                    s.statuses.insert(id, Status::Failed);
                    ui.set_connection_status(format!("Connection failed: {err}").into());
                }
            }
            show_sidebar(&ui, &s);
            show_topics(&ui, &s);
        });
    });
}

/// Closes the current connection, e.g. when its profile is deleted.
pub fn disconnect(ui: &AppWindow, s: &mut State) {
    clear_session(s);
    s.generation += 1;
    s.active = None;
    ui.set_connected(false);
    ui.set_selected_topic(SharedString::new());
    ui.set_connection_status(SharedString::new());
    ui.set_status_text("Select a connection.".into());
    show_topics(ui, s);
    messages::show_messages(ui, s);
}

fn clear_session(s: &mut State) {
    if let Some(previous) = s.active.take() {
        s.statuses.remove(&previous);
    }
    s.client = None;
    s.topics.clear();
    s.selected_topic = None;
    s.messages.clear();
}

pub fn show_topics(ui: &AppWindow, s: &State) {
    let hide_unreadable = s
        .active
        .as_deref()
        .and_then(|id| s.store.profile(id))
        .is_none_or(|p| p.hide_unreadable_topics);
    let filter = s.topic_filter.to_lowercase();

    let mut hidden = 0;
    let items: Vec<TopicItem> = s
        .topics
        .iter()
        .filter(|t| s.show_internal || !t.is_internal)
        .filter(|t| {
            let unreadable = t.can_read == Some(false);
            hidden += (hide_unreadable && unreadable) as usize;
            !(hide_unreadable && unreadable)
        })
        .filter(|t| filter.is_empty() || t.name.to_lowercase().contains(&filter))
        .map(|t| TopicItem {
            name: t.name.as_str().into(),
            partitions: t.partition_count as i32,
            internal: t.is_internal,
            unreadable: t.can_read == Some(false),
        })
        .collect();

    ui.set_topics(ModelRc::new(VecModel::from(items)));
    ui.set_hidden_topics_note(if hidden > 0 {
        format!("{hidden} without read access hidden").into()
    } else {
        SharedString::new()
    });
}

pub fn select_topic(ui: &AppWindow, state: &Shared, topic: &str) {
    {
        let mut s = state.lock().unwrap();
        s.selected_topic = Some(topic.to_string());
        s.messages.clear();
        messages::show_messages(ui, &s);
    }
    ui.set_selected_topic(topic.into());
    ui.set_detail(MessageDetail::default());
    messages::fetch(ui, state);
}
