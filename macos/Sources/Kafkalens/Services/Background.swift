import Foundation
import KafkalensCore

/// Core calls block on network I/O, so they always run off the main actor.
func offMainActor<T: Sendable>(_ work: @escaping @Sendable () throws -> T) async throws -> T {
    try await Task.detached(priority: .userInitiated) { try work() }.value
}

/// A readable message for errors coming from the core or elsewhere.
func describe(_ error: Error) -> String {
    switch error {
    case KafkaLensError.InvalidConfig(let message): "Invalid configuration: \(message)"
    case KafkaLensError.TopicNotFound(let topic): "Topic not found: \(topic)"
    case KafkaLensError.ReadOnly: "This connection is read-only. Turn off Read-only in its settings to produce."
    case KafkaLensError.Kafka(let message) where message.contains("TopicAuthorizationFailed"):
        "Your user is not authorized for this operation on the topic (TopicAuthorizationFailed)."
    case KafkaLensError.Kafka(let message): message
    default: error.localizedDescription
    }
}
