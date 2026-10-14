import Foundation
import KafkalensCore
import Observation

/// An open connection to one cluster and its topic list.
@MainActor
@Observable
final class ClusterSession {
    enum State {
        case connecting
        case connected(ClusterInfo)
        case failed(String)
    }

    let profile: ConnectionProfile
    private(set) var state: State = .connecting
    private(set) var topics: [TopicSummary] = []
    private(set) var isRefreshing = false
    private(set) var client: KafkaClient?

    init(profile: ConnectionProfile) {
        self.profile = profile
    }

    func connect() async {
        state = .connecting
        let config = profile.coreConfig(password: Keychain.password(for: profile.id))
        do {
            let client = try await offMainActor { try KafkaClient(config: config) }
            let info = try await offMainActor { try client.clusterInfo() }
            self.client = client
            state = .connected(info)
            await refreshTopics()
        } catch {
            client = nil
            topics = []
            state = .failed(describe(error))
        }
    }

    func refreshTopics() async {
        guard let client, !isRefreshing else { return }
        isRefreshing = true
        defer { isRefreshing = false }
        do {
            topics = try await offMainActor { try client.listTopics() }
        } catch {
            state = .failed(describe(error))
        }
    }
}
