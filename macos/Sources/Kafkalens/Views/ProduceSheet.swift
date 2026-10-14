import KafkalensCore
import SwiftUI

struct ProduceSheet: View {
    @Environment(\.dismiss) private var dismiss

    let browser: TopicBrowser
    let access: WriteAccess
    /// Called on close when at least one message was sent.
    let onSent: () -> Void

    @ViewState private var draft: ProduceDraft
    @ViewState private var isSending = false
    @ViewState private var isConfirming = false
    @ViewState private var outcome: Outcome?
    @ViewState private var sentCount = 0

    private enum Outcome {
        case sent(ProduceResult)
        case failed(String)
    }

    init(browser: TopicBrowser, access: WriteAccess, draft: ProduceDraft, onSent: @escaping () -> Void) {
        self.browser = browser
        self.access = access
        self.onSent = onSent
        _draft = ViewState(initialValue: draft)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            Form {
                Section {
                    Picker("Partition", selection: $draft.partition) {
                        Text("Automatic").tag(Int32?.none)
                        ForEach(browser.details?.partitions ?? [], id: \.id) { partition in
                            Text("\(partition.id)").tag(Int32?.some(partition.id))
                        }
                    }
                }

                Section("Key") {
                    Picker("Encoding", selection: $draft.keyEncoding) {
                        ForEach(ProduceDraft.Encoding.allCases) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    TextField("Key", text: $draft.key, prompt: Text("Empty sends a null key"))
                        .font(.body.monospaced())
                }

                Section {
                    Toggle("Tombstone (null value)", isOn: $draft.isTombstone)
                    if !draft.isTombstone {
                        HStack {
                            Picker("Encoding", selection: $draft.valueEncoding) {
                                ForEach(ProduceDraft.Encoding.allCases) { Text($0.label).tag($0) }
                            }
                            .pickerStyle(.segmented)
                            if draft.valueEncoding == .json {
                                Button("Format") { draft.prettifyValue() }
                                    .disabled(draft.valueProblem != nil)
                            }
                        }
                        TextEditor(text: $draft.value)
                            .font(.body.monospaced())
                            .frame(minHeight: 180)
                        if draft.value.isEmpty && draft.valueProblem != nil {
                            Text("Enter a JSON value, or switch the encoding to Text.")
                                .foregroundStyle(.secondary)
                                .font(.caption)
                        } else if let problem = draft.valueProblem {
                            Label(problem, systemImage: "exclamationmark.triangle.fill")
                                .foregroundStyle(.orange)
                                .font(.caption)
                        }
                    }
                } header: {
                    Text("Value")
                }

                Section("Headers") {
                    ForEach($draft.headers) { $header in
                        HStack {
                            TextField("key", text: $header.key)
                            TextField("value", text: $header.value)
                            Button("Remove", systemImage: "minus.circle") {
                                draft.headers.removeAll { $0.id == header.id }
                            }
                            .labelStyle(.iconOnly)
                            .buttonStyle(.borderless)
                        }
                        .font(.body.monospaced())
                    }
                    Button("Add Header", systemImage: "plus") {
                        draft.headers.append(.init(key: "", value: ""))
                    }
                    .buttonStyle(.borderless)
                }
            }
            .formStyle(.grouped)

            Divider()
            footer
        }
        .frame(width: 620, height: 680)
        .confirmationDialog(
            "Produce to “\(browser.topic)” on “\(access.connectionName)”?",
            isPresented: $isConfirming
        ) {
            Button("Produce", role: .destructive) { send() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This connection is in “\(access.confirmationFolder ?? "")”, which asks for confirmation before every message.")
        }
    }

    private var header: some View {
        HStack {
            Image(systemName: "paperplane")
            Text("Produce to ").foregroundStyle(.secondary) + Text(browser.topic).bold()
            Spacer()
            if let folder = access.confirmationFolder {
                Label(folder, systemImage: "exclamationmark.shield.fill")
                    .foregroundStyle(.orange)
                    .help("Protected folder: every message needs confirmation")
            }
            Text(access.connectionName).foregroundStyle(.secondary)
        }
        .padding()
    }

    private var footer: some View {
        HStack {
            switch outcome {
            case .sent(let result):
                Label("Sent to partition \(result.partition), offset \(result.offset)", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
                    .textSelection(.enabled)
            case .failed(let message):
                Label(message, systemImage: "xmark.octagon.fill")
                    .foregroundStyle(.red)
                    .lineLimit(2)
                    .help(message)
            case nil:
                EmptyView()
            }
            if isSending {
                ProgressView().controlSize(.small)
            }
            Spacer()
            Button(sentCount > 0 ? "Done" : "Cancel", role: .cancel) {
                if sentCount > 0 { onSent() }
                dismiss()
            }
            .keyboardShortcut(.cancelAction)
            Button("Produce") {
                if access.confirmationFolder != nil {
                    isConfirming = true
                } else {
                    send()
                }
            }
            .keyboardShortcut(.return, modifiers: .command)
            .disabled(isSending || draft.valueProblem != nil)
        }
        .padding()
    }

    private func send() {
        let request: ProduceRequest
        do {
            request = try draft.request(topic: browser.topic)
        } catch {
            outcome = .failed(error.localizedDescription)
            return
        }

        isSending = true
        outcome = nil
        Task {
            defer { isSending = false }
            do {
                outcome = .sent(try await browser.produce(request))
                sentCount += 1
            } catch {
                outcome = .failed(describe(error))
            }
        }
    }
}
