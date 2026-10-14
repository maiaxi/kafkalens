import Foundation
import KafkalensCore

/// Editable form state for producing one message.
struct ProduceDraft {
    enum Encoding: String, CaseIterable, Identifiable {
        case text, json, hex

        var id: Self { self }

        var label: String {
            switch self {
            case .text: "Text"
            case .json: "JSON"
            case .hex: "Hex"
            }
        }
    }

    struct Header: Identifiable {
        var id = UUID()
        var key: String
        var value: String
    }

    enum DraftError: LocalizedError {
        case invalidJSON(field: String)
        case invalidHex(field: String)
        case emptyHeaderKey

        var errorDescription: String? {
            switch self {
            case .invalidJSON(let field): "\(field) is not valid JSON."
            case .invalidHex(let field): "\(field) is not valid hex: use pairs of 0-9 / a-f, spaces allowed."
            case .emptyHeaderKey: "Every header needs a key."
            }
        }
    }

    /// nil lets the partitioner choose.
    var partition: Int32?
    /// An empty key is sent as a null key.
    var key = ""
    var keyEncoding: Encoding = .text
    var value = ""
    var valueEncoding: Encoding = .json
    /// Sends a null value, which deletes the key on compacted topics.
    var isTombstone = false
    var headers: [Header] = []

    init() {}

    /// Pre-fills the form from an existing message, for "duplicate and edit".
    init(duplicating message: KafkaMessage) {
        partition = message.partition
        if let key = message.key {
            (self.key, keyEncoding) = Self.editable(key, preferJSON: false)
        }
        if let value = message.value {
            (self.value, valueEncoding) = Self.editable(value, preferJSON: true)
        } else {
            isTombstone = true
        }
        headers = message.headers.map {
            Header(key: $0.key, value: $0.value.map { String(decoding: $0, as: UTF8.self) } ?? "")
        }
    }

    func request(topic: String) throws -> ProduceRequest {
        let headers = try headers.map { header in
            let key = header.key.trimmingCharacters(in: .whitespaces)
            guard !key.isEmpty else { throw DraftError.emptyHeaderKey }
            return MessageHeader(key: key, value: Data(header.value.utf8))
        }
        return ProduceRequest(
            topic: topic,
            partition: partition,
            key: key.isEmpty ? nil : try Self.bytes(key, keyEncoding, field: "Key"),
            value: isTombstone ? nil : try Self.bytes(value, valueEncoding, field: "Value"),
            headers: headers
        )
    }

    /// Validation message for the value as typed, or nil when it is fine.
    var valueProblem: String? {
        guard !isTombstone else { return nil }
        do {
            _ = try Self.bytes(value, valueEncoding, field: "Value")
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    /// Re-indents the value when it is JSON.
    mutating func prettifyValue() {
        guard valueEncoding == .json, let data = value.data(using: .utf8) else { return }
        let formatted = formatPayload(bytes: data, format: .json, pretty: true)
        if formatted.format == .json {
            value = formatted.text
        }
    }

    // MARK: Encoding

    static func bytes(_ text: String, _ encoding: Encoding, field: String) throws -> Data {
        switch encoding {
        case .text:
            return Data(text.utf8)
        case .json:
            let data = Data(text.utf8)
            guard (try? JSONSerialization.jsonObject(with: data, options: .fragmentsAllowed)) != nil else {
                throw DraftError.invalidJSON(field: field)
            }
            return data
        case .hex:
            guard let data = Data(hex: text) else { throw DraftError.invalidHex(field: field) }
            return data
        }
    }

    private static func editable(_ bytes: Data, preferJSON: Bool) -> (String, Encoding) {
        if preferJSON {
            let json = formatPayload(bytes: bytes, format: .json, pretty: true)
            // Schema Registry framed payloads would lose their header as JSON.
            if json.format == .json && json.schemaId == nil {
                return (json.text, .json)
            }
        }
        if let text = String(data: bytes, encoding: .utf8), !text.contains("\0") {
            return (text, .text)
        }
        return (bytes.hexString, .hex)
    }
}

extension Data {
    /// Parses hex digits, ignoring whitespace and an optional 0x prefix.
    init?(hex: String) {
        var digits = hex.filter { !$0.isWhitespace }
        if digits.hasPrefix("0x") || digits.hasPrefix("0X") {
            digits.removeFirst(2)
        }
        guard digits.count.isMultiple(of: 2) else { return nil }

        var bytes = [UInt8]()
        bytes.reserveCapacity(digits.count / 2)
        var index = digits.startIndex
        while index < digits.endIndex {
            let next = digits.index(index, offsetBy: 2)
            guard let byte = UInt8(digits[index..<next], radix: 16) else { return nil }
            bytes.append(byte)
            index = next
        }
        self.init(bytes)
    }

    /// Space-separated hex, 16 bytes per line.
    var hexString: String {
        stride(from: 0, to: count, by: 16).map { start in
            self[(startIndex + start)..<(startIndex + Swift.min(start + 16, count))]
                .map { String(format: "%02x", $0) }
                .joined(separator: " ")
        }
        .joined(separator: "\n")
    }
}
