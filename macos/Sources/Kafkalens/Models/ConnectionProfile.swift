import Foundation
import KafkalensCore

/// A saved cluster connection. The SASL password lives in the Keychain,
/// keyed by `id`, never in this struct's persisted form.
struct ConnectionProfile: Identifiable, Codable, Hashable {
    enum Security: String, Codable, CaseIterable, Identifiable {
        case plaintext, ssl, saslPlaintext, saslSsl

        var id: Self { self }

        var label: String {
            switch self {
            case .plaintext: "PLAINTEXT"
            case .ssl: "SSL"
            case .saslPlaintext: "SASL_PLAINTEXT"
            case .saslSsl: "SASL_SSL"
            }
        }

        var usesSasl: Bool { self == .saslPlaintext || self == .saslSsl }
    }

    enum Mechanism: String, Codable, CaseIterable, Identifiable {
        case plain, scramSha256, scramSha512

        var id: Self { self }

        var label: String {
            switch self {
            case .plain: "PLAIN"
            case .scramSha256: "SCRAM-SHA-256"
            case .scramSha512: "SCRAM-SHA-512"
            }
        }
    }

    struct Property: Codable, Hashable, Identifiable {
        var id = UUID()
        var key: String
        var value: String
    }

    var id = UUID()
    var name: String
    var bootstrapServers: String
    /// nil keeps the connection at the top level of the sidebar.
    var folderID: ConnectionFolder.ID?
    var security: Security = .plaintext
    var mechanism: Mechanism = .plain
    var username: String = ""
    var requestTimeoutMs: UInt64 = 10_000
    /// Blocks producing. On by default so a connection only writes once
    /// someone has deliberately allowed it.
    var readOnly = true
    /// Hides topics the broker reports as not readable by this principal.
    var hideUnreadableTopics = true
    var extraProperties: [Property] = []

    init(name: String, bootstrapServers: String, folderID: ConnectionFolder.ID? = nil) {
        self.name = name
        self.bootstrapServers = bootstrapServers
        self.folderID = folderID
    }

    static func new(in folderID: ConnectionFolder.ID? = nil) -> ConnectionProfile {
        ConnectionProfile(name: "Local", bootstrapServers: "localhost:9092", folderID: folderID)
    }

    // Decoded by hand so profiles saved by older versions, which lack newer
    // fields, still load with defaults instead of being dropped.
    private enum CodingKeys: String, CodingKey {
        case id, name, bootstrapServers, folderID, security, mechanism, username, requestTimeoutMs, readOnly, hideUnreadableTopics, extraProperties
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UUID.self, forKey: .id)
        name = try c.decode(String.self, forKey: .name)
        bootstrapServers = try c.decode(String.self, forKey: .bootstrapServers)
        folderID = try c.decodeIfPresent(UUID.self, forKey: .folderID)
        security = try c.decodeIfPresent(Security.self, forKey: .security) ?? .plaintext
        mechanism = try c.decodeIfPresent(Mechanism.self, forKey: .mechanism) ?? .plain
        username = try c.decodeIfPresent(String.self, forKey: .username) ?? ""
        requestTimeoutMs = try c.decodeIfPresent(UInt64.self, forKey: .requestTimeoutMs) ?? 10_000
        readOnly = try c.decodeIfPresent(Bool.self, forKey: .readOnly) ?? true
        hideUnreadableTopics = try c.decodeIfPresent(Bool.self, forKey: .hideUnreadableTopics) ?? true
        extraProperties = try c.decodeIfPresent([Property].self, forKey: .extraProperties) ?? []
    }

    func coreConfig(password: String?) -> ConnectionConfig {
        ConnectionConfig(
            bootstrapServers: bootstrapServers,
            securityProtocol: security.core,
            saslMechanism: security.usesSasl ? mechanism.core : nil,
            saslUsername: security.usesSasl && !username.isEmpty ? username : nil,
            saslPassword: security.usesSasl ? password : nil,
            requestTimeoutMs: requestTimeoutMs,
            readOnly: readOnly,
            extra: extraProperties.map { ConfigEntry(key: $0.key, value: $0.value) }
        )
    }
}

private extension ConnectionProfile.Security {
    var core: SecurityProtocol {
        switch self {
        case .plaintext: .plaintext
        case .ssl: .ssl
        case .saslPlaintext: .saslPlaintext
        case .saslSsl: .saslSsl
        }
    }
}

private extension ConnectionProfile.Mechanism {
    var core: SaslMechanism {
        switch self {
        case .plain: .plain
        case .scramSha256: .scramSha256
        case .scramSha512: .scramSha512
        }
    }
}
