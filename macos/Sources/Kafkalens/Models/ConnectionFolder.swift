import Foundation

/// A sidebar folder grouping connections (e.g. by environment or team).
struct ConnectionFolder: Identifiable, Codable, Hashable {
    var id = UUID()
    var name: String
    var isExpanded = true
    /// Asks for confirmation before every produce to a connection in this
    /// folder, for sensitive environments such as production.
    var confirmsProduce = false

    init(name: String) {
        self.name = name
    }

    // Decoded by hand so folders saved by older versions still load.
    private enum CodingKeys: String, CodingKey {
        case id, name, isExpanded, confirmsProduce
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UUID.self, forKey: .id)
        name = try c.decode(String.self, forKey: .name)
        isExpanded = try c.decodeIfPresent(Bool.self, forKey: .isExpanded) ?? true
        confirmsProduce = try c.decodeIfPresent(Bool.self, forKey: .confirmsProduce) ?? false
    }
}
