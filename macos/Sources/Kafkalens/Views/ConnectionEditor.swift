import KafkalensCore
import SwiftUI

struct ConnectionEditor: View {
    @Environment(ProfileStore.self) private var profiles
    @Environment(\.dismiss) private var dismiss

    @ViewState private var profile: ConnectionProfile
    @ViewState private var password: String
    @ViewState private var testResult: TestResult?
    @ViewState private var isTesting = false

    let onSave: (ConnectionProfile) -> Void

    private enum TestResult {
        case success(String)
        case failure(String)
    }

    init(profile: ConnectionProfile, onSave: @escaping (ConnectionProfile) -> Void) {
        _profile = ViewState(initialValue: profile)
        _password = ViewState(initialValue: Keychain.password(for: profile.id) ?? "")
        self.onSave = onSave
    }

    private var isValid: Bool {
        !profile.name.trimmingCharacters(in: .whitespaces).isEmpty
            && !profile.bootstrapServers.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    TextField("Name", text: $profile.name)
                    TextField("Bootstrap servers", text: $profile.bootstrapServers, prompt: Text("host:9092,host2:9092"))
                    if !profiles.folders.isEmpty {
                        Picker("Folder", selection: $profile.folderID) {
                            Text("None").tag(ConnectionFolder.ID?.none)
                            ForEach(profiles.folders) { folder in
                                Text(folder.name).tag(ConnectionFolder.ID?.some(folder.id))
                            }
                        }
                    }
                }

                Section {
                    Toggle("Read-only", isOn: $profile.readOnly)
                } footer: {
                    Text(profile.readOnly
                        ? "Messages can only be read. Producing is blocked."
                        : "Producing messages is allowed on this connection.")
                        .foregroundStyle(profile.readOnly ? Color.secondary : Color.orange)
                }

                Section("Security") {
                    Picker("Protocol", selection: $profile.security) {
                        ForEach(ConnectionProfile.Security.allCases) { Text($0.label).tag($0) }
                    }
                    if profile.security.usesSasl {
                        Picker("Mechanism", selection: $profile.mechanism) {
                            ForEach(ConnectionProfile.Mechanism.allCases) { Text($0.label).tag($0) }
                        }
                        TextField("Username", text: $profile.username)
                        SecureField("Password", text: $password)
                    }
                }

                Section {
                    LabeledContent("Request timeout") {
                        HStack {
                            TextField("", value: $profile.requestTimeoutMs, format: .number)
                                .frame(width: 80)
                                .multilineTextAlignment(.trailing)
                            Text("ms")
                        }
                    }
                }

                Section {
                    ForEach($profile.extraProperties) { $property in
                        HStack {
                            TextField("key", text: $property.key, prompt: Text("ssl.ca.location"))
                            TextField("value", text: $property.value)
                            Button("Remove", systemImage: "minus.circle") {
                                profile.extraProperties.removeAll { $0.id == property.id }
                            }
                            .labelStyle(.iconOnly)
                            .buttonStyle(.borderless)
                        }
                    }
                    Button("Add Property", systemImage: "plus") {
                        profile.extraProperties.append(.init(key: "", value: ""))
                    }
                    .buttonStyle(.borderless)
                } header: {
                    Text("librdkafka properties")
                } footer: {
                    Text("Passed through as-is; they override the settings above.")
                        .foregroundStyle(.secondary)
                }
            }
            .formStyle(.grouped)

            Divider()

            HStack {
                Button("Test Connection", action: test)
                    .disabled(!isValid || isTesting)
                if isTesting {
                    ProgressView().controlSize(.small)
                }
                testResultView
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    profiles.save(profile, password: password)
                    onSave(profile)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(!isValid)
            }
            .padding()
        }
        .frame(width: 520, height: 560)
    }

    @ViewBuilder
    private var testResultView: some View {
        switch testResult {
        case .success(let message):
            Label(message, systemImage: "checkmark.circle.fill")
                .foregroundStyle(.green)
        case .failure(let message):
            Label(message, systemImage: "xmark.octagon.fill")
                .foregroundStyle(.red)
                .lineLimit(2)
                .help(message)
        case nil:
            EmptyView()
        }
    }

    private func test() {
        let config = profile.coreConfig(password: password)
        isTesting = true
        testResult = nil
        Task {
            defer { isTesting = false }
            do {
                let info = try await offMainActor {
                    try KafkaClient(config: config).clusterInfo()
                }
                testResult = .success("\(info.brokers.count) broker(s), \(info.topicCount) topics")
            } catch {
                testResult = .failure(describe(error))
            }
        }
    }
}
