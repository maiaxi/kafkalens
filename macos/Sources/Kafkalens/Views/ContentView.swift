import SwiftUI

struct ContentView: View {
    @Environment(ProfileStore.self) private var profiles

    @ViewState private var selectedProfileID: ConnectionProfile.ID?
    @ViewState private var selectedTopic: String?
    /// Open connections, kept while the window lives so switching back is instant.
    @ViewState private var sessions: [ConnectionProfile.ID: ClusterSession] = [:]
    @ViewState private var editing: ConnectionProfile?
    @ViewState private var isCreatingFolder = false

    private var session: ClusterSession? {
        selectedProfileID.flatMap { sessions[$0] }
    }

    var body: some View {
        NavigationSplitView {
            ConnectionsSidebar(
                selection: $selectedProfileID,
                isCreatingFolder: $isCreatingFolder,
                sessions: sessions,
                onAdd: { editing = .new(in: $0) },
                onEdit: { editing = $0 },
                onDelete: delete
            )
            .navigationSplitViewColumnWidth(min: 180, ideal: 220)
        } content: {
            if let session {
                TopicListView(session: session, selection: $selectedTopic)
                    .navigationSplitViewColumnWidth(min: 220, ideal: 280)
            } else {
                ContentUnavailableView(
                    "No Connection",
                    systemImage: "server.rack",
                    description: Text("Select or add a Kafka cluster.")
                )
            }
        } detail: {
            if let session, let client = session.client, let selectedTopic {
                TopicView(
                    topic: selectedTopic,
                    client: client,
                    writeAccess: writeAccess(for: session),
                    topicWritable: session.topics.first { $0.name == selectedTopic }?.isWritable ?? true
                )
                .id("\(session.profile.id)/\(selectedTopic)")
            } else {
                ContentUnavailableView("No Topic Selected", systemImage: "list.bullet.rectangle")
            }
        }
        .onChange(of: selectedProfileID) { _, id in
            selectedTopic = nil
            openSession(id)
        }
        .sheet(item: $editing) { profile in
            ConnectionEditor(profile: profile) { saved in
                // Reconnect with the new settings.
                sessions[saved.id] = nil
                if selectedProfileID == saved.id {
                    openSession(saved.id)
                } else {
                    selectedProfileID = saved.id
                }
            }
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Menu("Add", systemImage: "plus") {
                    Button("New Connection…") { editing = .new() }
                    Button("New Folder…") { isCreatingFolder = true }
                }
            }
        }
        .onAppear {
            if profiles.profiles.isEmpty {
                editing = .new()
            }
        }
    }

    private func openSession(_ id: ConnectionProfile.ID?) {
        guard let id, sessions[id] == nil,
              let profile = profiles.profiles.first(where: { $0.id == id })
        else { return }
        let session = ClusterSession(profile: profile)
        sessions[id] = session
        Task { await session.connect() }
    }

    /// Read-only comes from the profile the client was built with, so it always
    /// matches what the core enforces; the folder is looked up live, since
    /// moving a connection does not reconnect it.
    private func writeAccess(for session: ClusterSession) -> WriteAccess? {
        let folderID = profiles.profiles.first { $0.id == session.profile.id }?.folderID
        return WriteAccess(profile: session.profile, folder: profiles.folder(folderID))
    }

    private func delete(_ profile: ConnectionProfile) {
        if selectedProfileID == profile.id {
            selectedProfileID = nil
        }
        sessions[profile.id] = nil
        profiles.delete(profile)
    }
}
