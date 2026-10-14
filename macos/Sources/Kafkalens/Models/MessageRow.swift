import Foundation
import KafkalensCore

/// A fetched message plus the one-line previews the table shows, computed
/// once off the main actor instead of on every render.
struct MessageRow: Identifiable, Hashable, Sendable {
    let message: KafkaMessage
    let timestamp: Date?
    let keyPreview: String
    let valuePreview: String
    let valueFormat: PayloadFormat?
    let schemaId: Int32?

    var id: String { "\(message.partition)-\(message.offset)" }
    var partition: Int32 { message.partition }
    var offset: Int64 { message.offset }
    var valueSize: Int { message.value?.count ?? 0 }
    /// Sort key for the timestamp column; messages without one sort first.
    var sortTimestamp: Date { timestamp ?? .distantPast }

    init(_ message: KafkaMessage) {
        self.message = message
        timestamp = message.timestampMs.map { Date(timeIntervalSince1970: Double($0) / 1000) }
        keyPreview = message.key.map { Self.preview(formatPayload(bytes: $0, format: .auto, pretty: false).text) } ?? ""

        if let value = message.value {
            let formatted = formatPayload(bytes: value, format: .auto, pretty: false)
            valuePreview = Self.preview(formatted.text)
            valueFormat = formatted.format
            schemaId = formatted.schemaId
        } else {
            valuePreview = ""
            valueFormat = nil
            schemaId = nil
        }
    }

    func matches(_ query: String) -> Bool {
        keyPreview.localizedCaseInsensitiveContains(query)
            || valuePreview.localizedCaseInsensitiveContains(query)
            || message.headers.contains { $0.key.localizedCaseInsensitiveContains(query) }
    }

    private static func preview(_ text: String) -> String {
        let flat = text.replacingOccurrences(of: "\n", with: " ")
        return flat.count > 300 ? String(flat.prefix(300)) + "…" : flat
    }
}

extension PayloadFormat {
    var label: String {
        switch self {
        case .auto: "Auto"
        case .text: "Text"
        case .json: "JSON"
        case .hex: "Hex"
        }
    }
}
