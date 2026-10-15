import SwiftUI
import UniformTypeIdentifiers

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
    /// What is being dragged, set when the drag starts, so drop targets know
    /// it without reading the pasteboard and can show the right indicator.
    @ViewState private var dragged: DragItem?
    /// Where the drag in progress would land.
    @ViewState private var dropTarget: DropTarget?
    /// Folder drags: the header the dragged folder will land above. "After
    /// folder X" is shown as "before the next one" (or the Connections
    /// heading), so the line never sits between a folder and its contents.
    @ViewState private var lineAbove: DropTarget?
    /// Folder header heights, to tell their upper half from their lower half.
    @ViewState private var headerHeights: [ConnectionFolder.ID: CGFloat] = [:]

    var body: some View {
        List(selection: $selection) {
            ForEach(profiles.folders) { folder in
                Section(isExpanded: expansion(folder)) {
                    ForEach(profiles.profiles(in: folder.id)) { row($0) }
                } header: {
                    folderHeader(folder)
                }
            }

            // Always shown, even with every connection in a folder: dropping a
            // connection here takes it out of its folder, and dropping a
            // folder here moves it to the end.
            Section {
                ForEach(profiles.profiles(in: nil)) { row($0) }
            } header: {
                Text("Connections")
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(.rect)
                    .dropHighlight(isTargeted(.ungrouped, by: .connection))
                    // A folder dropped here goes to the end: line above the heading.
                    .insertionLine(dragged?.kind == .folder && lineAbove == .ungrouped)
                    .onDrop(of: Self.dropTypes, delegate: dropDelegate(.ungrouped))
            }
        }
        .listStyle(.sidebar)
        // The list's empty space means the same as the Connections heading.
        .onDrop(of: Self.dropTypes, delegate: dropDelegate(.ungrouped))
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
        .onDrag { dragStarted(.connection(profile.id)) }
        .dropHighlight(isTargeted(.connection(profile.id), by: .connection))
        // Dropping a connection onto another files it in that one's folder.
        .onDrop(of: Self.dropTypes, delegate: dropDelegate(.connection(profile.id)))
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
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(.rect)
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { headerHeights[folder.id] = $0 }
            .onDrag { dragStarted(.folder(folder.id)) }
            // A connection goes into the folder (box); a folder is reordered
            // next to it (line above or below, from the pointer's half).
            .dropHighlight(isTargeted(.folder(folder.id), by: .connection))
            .insertionLine(dragged?.kind == .folder && lineAbove == .folder(folder.id))
            .onDrop(of: Self.dropTypes, delegate: dropDelegate(.folder(folder.id), height: headerHeights[folder.id] ?? 0))
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
                Button("Move Up") { profiles.shiftFolder(folder.id, by: -1) }
                    .disabled(profiles.folders.first?.id == folder.id)
                Button("Move Down") { profiles.shiftFolder(folder.id, by: 1) }
                    .disabled(profiles.folders.last?.id == folder.id)
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

    // MARK: Drag and drop

    /// Pasteboard types the sidebar accepts (drags carry a string payload).
    private static let dropTypes: [UTType] = [.utf8PlainText, .plainText, .text]

    private func dragStarted(_ item: DragItem) -> NSItemProvider {
        dragged = item
        return NSItemProvider(object: item.payload as NSString)
    }

    private func isTargeted(_ target: DropTarget, by kind: DragItem.Kind) -> Bool {
        dropTarget == target && dragged?.kind == kind
    }

    /// What dropping the current drag on `target` would do, if anything.
    private func accepts(_ target: DropTarget) -> Bool {
        switch (dragged, target) {
        case (.connection, _): true
        case (.folder(let id), .folder(let other)): id != other
        case (.folder, .ungrouped): true
        case (.folder, .connection), (nil, _): false
        }
    }

    private func dropDelegate(_ target: DropTarget, height: CGFloat = 0) -> SidebarDropDelegate {
        SidebarDropDelegate(
            height: height,
            accepts: { accepts(target) },
            update: { after in
                dropTarget = target
                lineAbove = lineTarget(for: target, after: after)
            },
            exit: {
                if dropTarget == target {
                    dropTarget = nil
                    lineAbove = nil
                }
            },
            perform: { after in perform(on: target, after: after) }
        )
    }

    /// Header a dragged folder would land above.
    /// nil when the drop would leave the folder where it is (right above or
    /// right below itself): no line then, and dropping does nothing.
    private func lineTarget(for target: DropTarget, after: Bool) -> DropTarget? {
        let folders = profiles.folders
        // Position in the folder list the line marks; the end is `count`.
        let position: Int
        switch target {
        case .folder(let id):
            guard let index = folders.firstIndex(where: { $0.id == id }) else { return nil }
            position = after ? index + 1 : index
        case .ungrouped:
            position = folders.count
        case .connection:
            return nil
        }
        if case .folder(let id)? = dragged, let from = folders.firstIndex(where: { $0.id == id }),
           position == from || position == from + 1 {
            return nil
        }
        return folders.indices.contains(position) ? .folder(folders[position].id) : .ungrouped
    }

    /// A connection moves into the target's folder (nil: none). A folder is
    /// reordered before or after the target folder, or to the end.
    private func perform(on target: DropTarget, after: Bool) -> Bool {
        defer {
            dropTarget = nil
            lineAbove = nil
            dragged = nil
        }
        switch dragged {
        case .connection(let id):
            let folderID: ConnectionFolder.ID? = switch target {
            case .folder(let folder): folder
            case .connection(let other): profiles.profiles.first { $0.id == other }?.folderID
            case .ungrouped: nil
            }
            profiles.move(id, to: folderID)
            return true
        case .folder(let id):
            // Right above or below itself: nothing to do.
            guard lineTarget(for: target, after: after) != nil else { return false }
            switch target {
            case .folder(let other): profiles.moveFolder(id, nextTo: other, after: after)
            case .ungrouped: profiles.moveFolder(id, nextTo: nil, after: false)
            case .connection: return false
            }
            return true
        case nil:
            return false
        }
    }
}

/// Drop handling for one sidebar target. Proposes a move, so the pointer
/// shows no "+" (copy) badge.
private struct SidebarDropDelegate: DropDelegate {
    /// Height of the target, to tell its upper half from its lower half.
    let height: CGFloat
    let accepts: () -> Bool
    let update: (_ after: Bool) -> Void
    let exit: () -> Void
    let perform: (_ after: Bool) -> Bool

    private func isAfter(_ info: DropInfo) -> Bool {
        height > 0 && info.location.y > height / 2
    }

    func validateDrop(info: DropInfo) -> Bool { accepts() }

    func dropEntered(info: DropInfo) { update(isAfter(info)) }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        update(isAfter(info))
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) { exit() }

    func performDrop(info: DropInfo) -> Bool { perform(isAfter(info)) }
}

/// Where a sidebar drag can land.
private enum DropTarget: Equatable {
    case folder(ConnectionFolder.ID)
    case connection(ConnectionProfile.ID)
    case ungrouped
}

/// What a sidebar drag carries, as a string: a connection's id, or a folder's
/// id with a prefix so the two can be told apart.
private enum DragItem: Equatable {
    case connection(ConnectionProfile.ID)
    case folder(ConnectionFolder.ID)

    enum Kind { case connection, folder }

    var kind: Kind {
        switch self {
        case .connection: .connection
        case .folder: .folder
        }
    }

    private static let folderPrefix = "folder:"

    var payload: String {
        switch self {
        case .connection(let id): id.uuidString
        case .folder(let id): Self.folderPrefix + id.uuidString
        }
    }

    init?(_ payload: String) {
        if payload.hasPrefix(Self.folderPrefix) {
            guard let id = UUID(uuidString: String(payload.dropFirst(Self.folderPrefix.count))) else { return nil }
            self = .folder(id)
        } else {
            guard let id = UUID(uuidString: payload) else { return nil }
            self = .connection(id)
        }
    }
}

private extension View {
    /// Fixed blue, as in the Windows/Linux app: the accent colour turns grey
    /// in inactive windows and with the Graphite accent, which hid the
    /// indicators.
    static var dropColor: Color { Color(nsColor: .systemBlue) }

    /// Where a dragged folder will land: a line along the top of the header
    /// it will sit above.
    func insertionLine(_ isOn: Bool) -> some View {
        overlay(alignment: .top) {
            if isOn {
                // Inside the header's bounds: the list clips anything drawn
                // outside a section header.
                Capsule()
                    .fill(Self.dropColor)
                    .frame(height: 3)
            }
        }
    }

    /// Accent outline and tint on the row a drag would drop onto.
    func dropHighlight(_ isOn: Bool) -> some View {
        background {
            RoundedRectangle(cornerRadius: 5)
                .fill(isOn ? Self.dropColor.opacity(0.18) : .clear)
                .strokeBorder(isOn ? Self.dropColor : .clear, lineWidth: 1.5)
                .padding(-3)
        }
    }
}
