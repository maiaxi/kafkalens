import KafkalensCore
import SwiftUI

struct TopicView: View {
    private enum Tab: String, CaseIterable, Identifiable {
        case messages = "Messages"
        case partitions = "Partitions"
        var id: Self { self }
    }

    @ViewState private var browser: TopicBrowser
    @ViewState private var tab: Tab = .messages
    @ViewState private var selection: MessageRow.ID?
    @ViewState private var showInspector = true
    @ViewState private var producing: PendingProduce?

    /// nil when the connection is read-only.
    let writeAccess: WriteAccess?
    /// False when the broker's ACLs deny producing to this topic.
    let topicWritable: Bool

    private var canProduce: Bool { writeAccess != nil && topicWritable }

    private struct PendingProduce: Identifiable {
        let id = UUID()
        let draft: ProduceDraft
    }

    init(topic: String, client: KafkaClient, writeAccess: WriteAccess?, topicWritable: Bool) {
        _browser = ViewState(initialValue: TopicBrowser(topic: topic, client: client))
        self.writeAccess = writeAccess
        self.topicWritable = topicWritable
    }

    private var selectedRow: MessageRow? {
        selection.flatMap { id in browser.messages.first { $0.id == id } }
    }

    var body: some View {
        VStack(spacing: 0) {
            FetchBar(browser: browser)
            Divider()
            if let error = browser.error {
                ErrorBanner(message: error)
            }
            switch tab {
            case .messages:
                MessageTable(
                    rows: browser.visibleMessages,
                    selection: $selection,
                    onDuplicate: !canProduce ? nil : { row in
                        producing = PendingProduce(draft: ProduceDraft(duplicating: row.message))
                    }
                )
                .overlay { emptyState }
                StatusBar(browser: browser)
            case .partitions:
                PartitionsTable(details: browser.details)
            }
        }
        .navigationTitle(browser.topic)
        .navigationSubtitle(subtitle)
        .toolbar {
            ToolbarItem(placement: .principal) {
                Picker("View", selection: $tab) {
                    ForEach(Tab.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
            }
            ToolbarItem {
                if writeAccess != nil {
                    Button("Produce", systemImage: "paperplane") {
                        producing = PendingProduce(draft: ProduceDraft())
                    }
                    .disabled(!topicWritable)
                    .help(topicWritable
                        ? "Produce a message to this topic"
                        : "Your user has no write permission on this topic")
                } else {
                    Label("Read-only", systemImage: "lock.fill")
                        .foregroundStyle(.secondary)
                        .help("Read-only connection. Turn off Read-only in the connection settings to produce.")
                }
            }
            ToolbarItem {
                Button("Inspector", systemImage: "sidebar.trailing") {
                    showInspector.toggle()
                }
            }
        }
        .inspector(isPresented: $showInspector) {
            MessageInspector(row: selectedRow)
                .inspectorColumnWidth(min: 280, ideal: 380, max: 700)
        }
        .sheet(item: $producing) { pending in
            if let writeAccess {
                ProduceSheet(browser: browser, access: writeAccess, draft: pending.draft) {
                    Task { await browser.reload() }
                }
            }
        }
        .task { await browser.reload() }
    }

    private var subtitle: String {
        guard let details = browser.details else { return "" }
        return "\(details.partitions.count) partitions · ~\(details.approximateMessageCount.formatted()) messages"
    }

    @ViewBuilder
    private var emptyState: some View {
        if browser.isLoading && browser.messages.isEmpty {
            ProgressView()
        } else if !browser.isLoading && browser.error == nil && browser.visibleMessages.isEmpty {
            if browser.messages.isEmpty {
                ContentUnavailableView("No Messages", systemImage: "tray", description: Text("Nothing found for these fetch options."))
            } else {
                ContentUnavailableView.search(text: browser.search)
            }
        }
    }
}

private struct FetchBar: View {
    @Bindable var browser: TopicBrowser

    var body: some View {
        HStack(spacing: 12) {
            Picker("From", selection: $browser.start) {
                ForEach(TopicBrowser.Start.allCases) { Text($0.label).tag($0) }
            }
            .fixedSize()

            switch browser.start {
            case .offset:
                TextField("Offset", value: $browser.offset, format: .number.grouping(.never))
                    .frame(width: 100)
            case .timestamp:
                DatePicker("Since", selection: $browser.since)
                    .labelsHidden()
            case .latest, .beginning:
                EmptyView()
            }

            Picker("Partition", selection: $browser.partition) {
                Text("All").tag(Int32?.none)
                ForEach(browser.details?.partitions ?? [], id: \.id) { partition in
                    Text("\(partition.id)").tag(Int32?.some(partition.id))
                }
            }
            .fixedSize()

            Picker("Limit", selection: $browser.limit) {
                ForEach([50, 200, 500, 1000, 5000], id: \.self) { Text("\($0)").tag($0) }
            }
            .fixedSize()

            Button {
                Task { await browser.reload() }
            } label: {
                Label("Fetch", systemImage: "arrow.down.circle")
            }
            .keyboardShortcut("r", modifiers: .command)
            .disabled(browser.isLoading)

            if browser.isLoading {
                ProgressView().controlSize(.small)
            }

            Spacer()

            TextField("Search key, value or headers", text: $browser.search)
                .textFieldStyle(.roundedBorder)
                .frame(maxWidth: 280)
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}

private struct StatusBar: View {
    let browser: TopicBrowser

    var body: some View {
        HStack {
            if browser.search.isEmpty {
                Text("\(browser.messages.count) messages")
            } else {
                Text("\(browser.visibleMessages.count) of \(browser.messages.count) messages")
            }
            if let duration = browser.lastFetchDuration {
                Text("· \(duration.formatted(.units(allowed: [.seconds, .milliseconds], width: .narrow)))")
            }
            if browser.timedOut {
                Label("Fetch timed out; results may be partial", systemImage: "clock.badge.exclamationmark")
                    .foregroundStyle(.orange)
            }
            Spacer()
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .padding(.horizontal)
        .padding(.vertical, 4)
        .background(.bar)
    }
}

private struct ErrorBanner: View {
    let message: String

    var body: some View {
        Label(message, systemImage: "exclamationmark.triangle.fill")
            .foregroundStyle(.red)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8)
            .background(.red.opacity(0.08))
    }
}
