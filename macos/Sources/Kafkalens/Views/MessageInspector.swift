import AppKit
import KafkalensCore
import SwiftUI

struct MessageInspector: View {
    let row: MessageRow?

    @AppStorage("inspectorFormat") private var formatChoice = "auto"

    private var format: PayloadFormat {
        switch formatChoice {
        case "text": .text
        case "json": .json
        case "hex": .hex
        default: .auto
        }
    }

    var body: some View {
        if let row {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    metadata(row)

                    Picker("Format", selection: $formatChoice) {
                        ForEach([PayloadFormat.auto, .text, .json, .hex], id: \.label) { format in
                            Text(format.label).tag(format.label.lowercased())
                        }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()

                    PayloadSection(title: "Key", bytes: row.message.key, format: format)
                    PayloadSection(title: "Value", bytes: row.message.value, format: format)

                    if !row.message.headers.isEmpty {
                        headers(row.message.headers)
                    }
                }
                .padding()
            }
        } else {
            ContentUnavailableView("No Message Selected", systemImage: "doc.text.magnifyingglass")
        }
    }

    private func metadata(_ row: MessageRow) -> some View {
        Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 4) {
            GridRow {
                Text("Partition").foregroundStyle(.secondary)
                Text("\(row.partition)")
            }
            GridRow {
                Text("Offset").foregroundStyle(.secondary)
                Text(row.offset, format: .number.grouping(.never))
            }
            GridRow {
                Text("Timestamp").foregroundStyle(.secondary)
                if let timestamp = row.timestamp {
                    Text(timestamp.formatted(.iso8601.year().month().day().time(includingFractionalSeconds: true)))
                } else {
                    Text("—")
                }
            }
            if let schemaId = row.schemaId {
                GridRow {
                    Text("Schema ID").foregroundStyle(.secondary)
                    Text("\(schemaId)")
                }
            }
        }
        .font(.callout.monospacedDigit())
        .textSelection(.enabled)
    }

    private func headers(_ headers: [MessageHeader]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Headers").font(.headline)
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 4) {
                ForEach(Array(headers.enumerated()), id: \.offset) { _, header in
                    GridRow {
                        Text(header.key).foregroundStyle(.secondary)
                        Text(header.value.map { formatPayload(bytes: $0, format: .auto, pretty: false).text } ?? "null")
                    }
                }
            }
            .font(.callout.monospaced())
            .textSelection(.enabled)
        }
    }
}

private struct PayloadSection: View {
    let title: String
    let bytes: Data?
    let format: PayloadFormat

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let bytes {
                let payload = formatPayload(bytes: bytes, format: format, pretty: true)
                HStack {
                    Text(title).font(.headline)
                    Text("\(payload.format.label) · \(Int64(payload.sizeBytes).formatted(.byteCount(style: .memory)))")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Copy", systemImage: "doc.on.doc") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(payload.text, forType: .string)
                    }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .help("Copy \(title.lowercased())")
                }
                Text(payload.text)
                    .font(.callout.monospaced())
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(8)
                    .background(.quaternary.opacity(0.5), in: .rect(cornerRadius: 6))
            } else {
                HStack {
                    Text(title).font(.headline)
                    Text("null").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }
}
