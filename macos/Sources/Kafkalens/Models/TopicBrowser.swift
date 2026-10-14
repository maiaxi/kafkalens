import Foundation
import KafkalensCore
import Observation

/// Fetch options, partition details and fetched messages for one topic.
@MainActor
@Observable
final class TopicBrowser {
    enum Start: String, CaseIterable, Identifiable {
        case latest, beginning, offset, timestamp

        var id: Self { self }

        var label: String {
            switch self {
            case .latest: "Latest"
            case .beginning: "Beginning"
            case .offset: "Offset"
            case .timestamp: "Timestamp"
            }
        }
    }

    let topic: String
    private let client: KafkaClient

    // Fetch options.
    var start: Start = .latest
    var offset: Int64 = 0
    var since = Date().addingTimeInterval(-3600)
    /// nil reads every partition.
    var partition: Int32?
    var limit = 200

    // Results.
    private(set) var details: TopicDetails?
    private(set) var messages: [MessageRow] = []
    private(set) var isLoading = false
    private(set) var error: String?
    private(set) var timedOut = false
    private(set) var lastFetchDuration: Duration?

    var search = ""

    var visibleMessages: [MessageRow] {
        let query = search.trimmingCharacters(in: .whitespaces)
        return query.isEmpty ? messages : messages.filter { $0.matches(query) }
    }

    init(topic: String, client: KafkaClient) {
        self.topic = topic
        self.client = client
    }

    /// Refreshes partition details and messages together.
    func reload() async {
        guard !isLoading else { return }
        isLoading = true
        error = nil
        defer { isLoading = false }

        let client = client
        let topic = topic
        do {
            details = try await offMainActor { try client.describeTopic(topic: topic) }
            try await fetchMessages()
        } catch {
            self.error = describe(error)
        }
    }

    func produce(_ request: ProduceRequest) async throws -> ProduceResult {
        let client = client
        return try await offMainActor { try client.produceMessage(request: request) }
    }

    private func fetchMessages() async throws {
        let request = FetchRequest(
            topic: topic,
            partitions: partition.map { [$0] },
            start: corePosition,
            maxMessages: UInt32(clamping: limit),
            timeoutMs: 15_000
        )
        let client = client
        let clock = ContinuousClock()
        let began = clock.now

        let (rows, timedOut) = try await offMainActor {
            let result = try client.fetchMessages(request: request)
            // Newest first, which is what people usually look for.
            return (result.messages.reversed().map(MessageRow.init), result.timedOut)
        }

        messages = rows
        self.timedOut = timedOut
        lastFetchDuration = clock.now - began
    }

    private var corePosition: StartPosition {
        switch start {
        case .latest: .latest
        case .beginning: .beginning
        case .offset: .offset(offset: offset)
        case .timestamp: .timestamp(millis: Int64(since.timeIntervalSince1970 * 1000))
        }
    }
}
