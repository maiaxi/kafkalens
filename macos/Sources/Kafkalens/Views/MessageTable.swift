import KafkalensCore
import SwiftUI

extension PartitionInfo: Identifiable {}

struct MessageTable: View {
    let rows: [MessageRow]
    @Binding var selection: MessageRow.ID?
    /// nil hides "Duplicate and Edit" (read-only connections).
    var onDuplicate: ((MessageRow) -> Void)?

    @ViewState private var sortOrder = [KeyPathComparator(\MessageRow.sortTimestamp, order: .reverse)]

    var body: some View {
        Table(rows.sorted(using: sortOrder), selection: $selection, sortOrder: $sortOrder) {
            TableColumn("Partition", value: \.partition) { row in
                Text("\(row.partition)").monospacedDigit()
            }
            .width(min: 50, ideal: 60, max: 80)

            TableColumn("Offset", value: \.offset) { row in
                Text(row.offset, format: .number.grouping(.never)).monospacedDigit()
            }
            .width(min: 60, ideal: 80, max: 140)

            TableColumn("Timestamp", value: \.sortTimestamp) { row in
                if let timestamp = row.timestamp {
                    Text(timestamp, format: .dateTime.year().month().day().hour().minute().second())
                        .monospacedDigit()
                } else {
                    Text("—").foregroundStyle(.secondary)
                }
            }
            .width(min: 120, ideal: 160, max: 220)

            TableColumn("Key", value: \.keyPreview) { row in
                Text(row.keyPreview.isEmpty ? "null" : row.keyPreview)
                    .foregroundStyle(row.keyPreview.isEmpty ? .secondary : .primary)
                    .font(.body.monospaced())
            }
            .width(min: 80, ideal: 140, max: 400)

            TableColumn("Value", value: \.valuePreview) { row in
                Text(row.message.value == nil ? "null" : row.valuePreview)
                    .foregroundStyle(row.message.value == nil ? .secondary : .primary)
                    .font(.body.monospaced())
            }

            TableColumn("Size", value: \.valueSize) { row in
                Text(Int64(row.valueSize), format: .byteCount(style: .memory))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            .width(min: 50, ideal: 70, max: 100)
        }
        .contextMenu(forSelectionType: MessageRow.ID.self) { ids in
            if let onDuplicate, let id = ids.first, let row = rows.first(where: { $0.id == id }) {
                Button("Duplicate and Edit…", systemImage: "plus.square.on.square") { onDuplicate(row) }
            }
        }
    }
}

struct PartitionsTable: View {
    let details: TopicDetails?

    var body: some View {
        Table(details?.partitions ?? []) {
            TableColumn("Partition") { Text("\($0.id)").monospacedDigit() }
            TableColumn("Leader") { Text("\($0.leader)").monospacedDigit() }
            TableColumn("Replicas") { Text(list($0.replicas)).monospacedDigit() }
            TableColumn("In-Sync") { partition in
                Text(list(partition.inSyncReplicas))
                    .monospacedDigit()
                    .foregroundStyle(partition.inSyncReplicas.count < partition.replicas.count ? .orange : .primary)
            }
            TableColumn("Low") { Text($0.lowWatermark, format: .number.grouping(.never)).monospacedDigit() }
            TableColumn("High") { Text($0.highWatermark, format: .number.grouping(.never)).monospacedDigit() }
            TableColumn("Messages") { partition in
                Text(partition.highWatermark - partition.lowWatermark, format: .number).monospacedDigit()
            }
        }
    }

    private func list(_ ids: [Int32]) -> String {
        ids.map(String.init).joined(separator: ", ")
    }
}
