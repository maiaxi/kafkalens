import SwiftUI

struct ConnectionsSidebar: View {
    @Environment(ProfileStore.self) private var profiles

    @Binding var selection: ConnectionProfile.ID?
    /// Set by the toolbar to start naming a new folder.
    @Binding var isCreatingFolder: Bool
    let sessions: [ConnectionProfile.ID: ClusterSession]
    let onAdd: (ConnectionFolder.ID?) -> Void
    let onEdit: (ConnectionProfile) -> Void
    let onDelete: (ConnectionProfile) -> Void

    @ViewState private var renaming: ConnectionFolder?
    @ViewState private var folderName = ""

    var body: some View {
        List(selection: $selection) {
            ForEach(profiles.folders) { folder in
                Section(isExpanded: expansion(folder)) {
                    ForEach(profiles.profiles(in: folder.id)) { row($0) }
                } header: {
                    folderHeader(folder)
                }
            }

            let ungrouped = profiles.profiles(in: nil)
            if !ungrouped.isEmpty || profiles.folders.isEmpty {
                Section {
                    ForEach(ungrouped) { row($0) }
                } header: {
                    Text("Connections")
                        .dropDestination(for: String.self) { items, _ in
                            move(items, to: nil)
                        }
                }
            }
        }
        .listStyle(.sidebar)
        .alert("New Folder", isPresented: $isCreatingFolder) {
            TextField("Name", text: $folderName)
            Button("Create") {
                if let name = trimmedFolderName {
                    profiles.addFolder(named: name)
                }
            }
            Button("Cancel", role: .cancel) {}
        }
        .alert("Rename Folder", isPresented: isRenaming) {
            TextField("Name", text: $folderName)
            Button("Rename") {
                if let folder = renaming, let name = trimmedFolderName {
                    profiles.rename(folder.id, to: name)
                }
            }
            Button("Cancel", role: .cancel) {}
        }
        .onChange(of: isCreatingFolder) { _, creating in
            if creating { folderName = "" }
        }
    }

    private func row(_ profile: ConnectionProfile) -> some View {
        Label {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    Text(profile.name)
                    // Read-only is the norm; flag the connections that can write.
                    if !profile.readOnly {
                        Image(systemName: "pencil.circle.fill")
                            .foregroundStyle(.orange)
                            .help("Producing is allowed on this connection")
                    }
                }
                Text(profile.bootstrapServers)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        } icon: {
            statusIcon(sessions[profile.id]?.state)
        }
        .tag(profile.id)
        .draggable(profile.id.uuidString)
        // Dropping onto a connection files the dragged one next to it.
        .dropDestination(for: String.self) { items, _ in
            move(items, to: profile.folderID)
        }
        .contextMenu {
            Button("Edit…") { onEdit(profile) }
            moveMenu(profile)
            Divider()
            Button("Delete", role: .destructive) { onDelete(profile) }
        }
    }

    private func folderHeader(_ folder: ConnectionFolder) -> some View {
        HStack(spacing: 4) {
            Label(folder.name, systemImage: "folder")
            if folder.confirmsProduce {
                Image(systemName: "exclamationmark.shield.fill")
                    .foregroundStyle(.orange)
                    .help("Producing to connections in this folder asks for confirmation")
            }
        }
            .contentShape(.rect)
            .dropDestination(for: String.self) { items, _ in
                move(items, to: folder.id)
            }
            .contextMenu {
                Button("New Connection…") { onAdd(folder.id) }
                Button("Rename…") {
                    folderName = folder.name
                    renaming = folder
                }
                Toggle("Confirm Before Producing", isOn: Binding(
                    get: { folder.confirmsProduce },
                    set: { profiles.setConfirmsProduce(folder.id, $0) }
                ))
                Divider()
                Button("Delete Folder", role: .destructive) {
                    profiles.deleteFolder(folder.id)
                }
                .help("Connections in this folder move to the top level")
            }
    }

    @ViewBuilder
    private func moveMenu(_ profile: ConnectionProfile) -> some View {
        if !profiles.folders.isEmpty {
            Menu("Move to Folder") {
                ForEach(profiles.folders) { folder in
                    Button(folder.name) { profiles.move(profile.id, to: folder.id) }
                        .disabled(profile.folderID == folder.id)
                }
                Divider()
                Button("None") { profiles.move(profile.id, to: nil) }
                    .disabled(profile.folderID == nil)
            }
        }
    }

    @ViewBuilder
    private func statusIcon(_ state: ClusterSession.State?) -> some View {
        switch state {
        case nil:
            Image(systemName: "server.rack").foregroundStyle(.secondary)
        case .connecting:
            ProgressView().controlSize(.small)
        case .connected:
            Image(systemName: "server.rack").foregroundStyle(.green)
        case .failed:
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
        }
    }

    private func expansion(_ folder: ConnectionFolder) -> Binding<Bool> {
        Binding(
            get: { profiles.folders.first { $0.id == folder.id }?.isExpanded ?? true },
            set: { profiles.setExpanded(folder.id, $0) }
        )
    }

    private var isRenaming: Binding<Bool> {
        Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })
    }

    private var trimmedFolderName: String? {
        let name = folderName.trimmingCharacters(in: .whitespaces)
        return name.isEmpty ? nil : name
    }

    private func move(_ items: [String], to folderID: ConnectionFolder.ID?) -> Bool {
        let ids = items.compactMap(UUID.init(uuidString:))
        for id in ids {
            profiles.move(id, to: folderID)
        }
        return !ids.isEmpty
    }
}
