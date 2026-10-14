/// Present only when the connection is not read-only.
struct WriteAccess: Equatable {
    let connectionName: String
    /// Name of the folder that requires confirming each produce, if any.
    let confirmationFolder: String?

    init?(profile: ConnectionProfile, folder: ConnectionFolder?) {
        guard !profile.readOnly else { return nil }
        connectionName = profile.name
        confirmationFolder = folder?.confirmsProduce == true ? folder?.name : nil
    }
}
