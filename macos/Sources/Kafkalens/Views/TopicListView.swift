import KafkalensCore
import SwiftUI

struct TopicListView: View {
    @Environment(ProfileStore.self) private var profiles

    let session: ClusterSession
    @Binding var selection: String?

    @ViewState private var filter = ""
    @AppStorage("showInternalTopics") private var showInternal = false

    /// Per connection, read live so toggling it applies immediately.
    private var hideUnreadable: Bool {
        profiles.profiles.first { $0.id == session.profile.id }?.hideUnreadableTopics ?? true
    }

    private var hideUnreadableBinding: Binding<Bool> {
        Binding(
            get: { hideUnreadable },
            set: { profiles.setHideUnreadableTopics(session.profile.id, $0) }
        )
    }

    private var visibleTopics: [TopicSummary] {
        session.topics.filter { topic in
            (showInternal || !topic.isInternal)
                && !(hideUnreadable && topic.isUnreadable)
                && (filter.isEmpty || topic.name.localizedCaseInsensitiveContains(filter))
        }
    }

    private var hiddenUnreadableCount: Int {
        guard hideUnreadable else { return 0 }
        return session.topics.count { $0.isUnreadable && (showInternal || !$0.isInternal) }
    }

    var body: some View {
        Group {
            switch session.state {
            case .connecting:
                ProgressView("Connecting to \(session.profile.bootstrapServers)…")
            case .failed(let message):
                ContentUnavailableView {
                    Label("Connection Failed", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(message).textSelection(.enabled)
                } actions: {
                    Button("Retry") { Task { await session.connect() } }
                }
            case .connected:
                List(visibleTopics, id: \.name, selection: $selection) { topic in
                    row(topic)
                }
                .overlay {
                    if visibleTopics.isEmpty {
                        ContentUnavailableView.search(text: filter)
                    }
                }
                .safeAreaInset(edge: .bottom) {
                    if hiddenUnreadableCount > 0 {
                        hiddenNotice
                    }
                }
                .searchable(text: $filter, placement: .sidebar, prompt: "Filter topics")
            }
        }
        .navigationTitle(session.profile.name)
        .toolbar {
            ToolbarItemGroup {
                Menu("Topic Filters", systemImage: "line.3.horizontal.decrease.circle") {
                    Toggle("Show Internal Topics", isOn: $showInternal)
                    Toggle("Hide Topics Without Read Access", isOn: hideUnreadableBinding)
                }
                .help("Choose which topics to list")
                Button("Refresh Topics", systemImage: "arrow.clockwise") {
                    Task { await session.refreshTopics() }
                }
                .disabled(session.isRefreshing)
            }
        }
    }

    private func row(_ topic: TopicSummary) -> some View {
        HStack {
            Text(topic.name)
                .foregroundStyle(topic.isInternal || topic.isUnreadable ? .secondary : .primary)
            if topic.isUnreadable {
                Image(systemName: "lock.fill")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .help("No read permission on this topic")
            }
            Spacer()
            Text("\(topic.partitionCount)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .help("\(topic.partitionCount) partitions")
        }
    }

    private var hiddenNotice: some View {
        HStack {
            Image(systemName: "lock")
            Text("\(hiddenUnreadableCount) without read access hidden")
            Spacer()
            Button("Show") { hideUnreadableBinding.wrappedValue = false }
                .buttonStyle(.link)
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(.bar)
    }
}

extension TopicSummary {
    /// Only an explicit "no" counts: brokers that do not report ACLs leave it nil.
    var isUnreadable: Bool { canRead == false }
    var isWritable: Bool { canWrite != false }
}
