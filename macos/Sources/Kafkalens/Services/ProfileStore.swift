import Foundation
import Observation

/// Saved connections and the folders grouping them, persisted as JSON in
/// UserDefaults.
@MainActor
@Observable
final class ProfileStore {
    private static let profilesKey = "connectionProfiles"
    private static let foldersKey = "connectionFolders"

    private(set) var profiles: [ConnectionProfile] = []
    private(set) var folders: [ConnectionFolder] = []

    init() {
        profiles = Self.load(Self.profilesKey) ?? []
        folders = Self.load(Self.foldersKey) ?? []
    }

    func profiles(in folderID: ConnectionFolder.ID?) -> [ConnectionProfile] {
        profiles
            .filter { $0.folderID == folderID }
            .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    // MARK: Connections

    func save(_ profile: ConnectionProfile, password: String?) {
        if let index = profiles.firstIndex(where: { $0.id == profile.id }) {
            profiles[index] = profile
        } else {
            profiles.append(profile)
        }
        Keychain.setPassword(profile.security.usesSasl ? password : nil, for: profile.id)
        persistProfiles()
    }

    func delete(_ profile: ConnectionProfile) {
        profiles.removeAll { $0.id == profile.id }
        Keychain.setPassword(nil, for: profile.id)
        persistProfiles()
    }

    func move(_ profileID: ConnectionProfile.ID, to folderID: ConnectionFolder.ID?) {
        guard let index = profiles.firstIndex(where: { $0.id == profileID }),
              profiles[index].folderID != folderID
        else { return }
        profiles[index].folderID = folderID
        persistProfiles()
    }

    func setHideUnreadableTopics(_ profileID: ConnectionProfile.ID, _ hide: Bool) {
        guard let index = profiles.firstIndex(where: { $0.id == profileID }) else { return }
        profiles[index].hideUnreadableTopics = hide
        persistProfiles()
    }

    // MARK: Folders

    @discardableResult
    func addFolder(named name: String) -> ConnectionFolder {
        let folder = ConnectionFolder(name: name)
        folders.append(folder)
        sortFolders()
        persistFolders()
        return folder
    }

    func rename(_ folderID: ConnectionFolder.ID, to name: String) {
        guard let index = folders.firstIndex(where: { $0.id == folderID }) else { return }
        folders[index].name = name
        sortFolders()
        persistFolders()
    }

    func setExpanded(_ folderID: ConnectionFolder.ID, _ expanded: Bool) {
        guard let index = folders.firstIndex(where: { $0.id == folderID }) else { return }
        folders[index].isExpanded = expanded
        persistFolders()
    }

    func setConfirmsProduce(_ folderID: ConnectionFolder.ID, _ confirms: Bool) {
        guard let index = folders.firstIndex(where: { $0.id == folderID }) else { return }
        folders[index].confirmsProduce = confirms
        persistFolders()
    }

    func folder(_ id: ConnectionFolder.ID?) -> ConnectionFolder? {
        id.flatMap { id in folders.first { $0.id == id } }
    }

    /// Removes the folder; its connections move to the top level.
    func deleteFolder(_ folderID: ConnectionFolder.ID) {
        folders.removeAll { $0.id == folderID }
        for index in profiles.indices where profiles[index].folderID == folderID {
            profiles[index].folderID = nil
        }
        persistFolders()
        persistProfiles()
    }

    // MARK: Persistence

    private func sortFolders() {
        folders.sort { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    private func persistProfiles() {
        Self.store(profiles, Self.profilesKey)
    }

    private func persistFolders() {
        Self.store(folders, Self.foldersKey)
    }

    private static func load<T: Decodable>(_ key: String) -> T? {
        UserDefaults.standard.data(forKey: key).flatMap { try? JSONDecoder().decode(T.self, from: $0) }
    }

    private static func store<T: Encodable>(_ value: T, _ key: String) {
        if let data = try? JSONEncoder().encode(value) {
            UserDefaults.standard.set(data, forKey: key)
        }
    }
}
