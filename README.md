<p align="center">
  <img src="macos/Resources/AppIcon.png" width="128" alt="Kafkalens icon">
</p>

# Kafkalens

A fast, native desktop viewer for Kafka topics. macOS today; Windows and Linux
to follow on the same core.

## Features

- **Browse clusters and topics**: partitions, replicas, in-sync replicas and
  watermarks. Connections can be grouped into folders.
- **Read messages** from the beginning, the latest N, a given offset or a point
  in time, across all partitions or just one. Sort and search by key, value or
  headers.
- **Inspect payloads**: JSON is pretty-printed, text shown as-is, binary as a
  hex dump. The Confluent Schema Registry wire format is recognised and its
  schema ID shown.
- **Produce messages** with key, value (text, JSON or hex), headers, a chosen
  partition, or a tombstone. "Duplicate and edit" starts from an existing
  message.
- **Safe by default**: every connection is read-only until you allow
  producing, and a folder (say, "Prod") can require confirmation before each
  message. The read-only check is enforced in the core, not just the UI.
- **Respects ACLs**: topics your user cannot read are hidden (optional, per
  connection), and producing is disabled where you lack write permission.
- **Security**: PLAINTEXT, SSL, SASL/PLAIN and SCRAM-SHA-256/512. Passwords
  are kept in the macOS Keychain. Any librdkafka property can be added for
  setups the form does not cover.

## Install

Download the latest `Kafkalens-<version>-macos-universal.zip` from
[Releases](https://github.com/maiaxi/kafkalens/releases), unzip it and move
`Kafkalens.app` to Applications. It runs on macOS 14 or later, on Apple
Silicon and Intel.

Releases are not notarized by Apple, so macOS blocks the first launch of
each downloaded version. Open System Settings → Privacy & Security and click
**Open Anyway**, or run:

```sh
xattr -dr com.apple.quarantine /Applications/Kafkalens.app
```

## Architecture

```
kafkalens/
├── core/      # Rust crate (kafkalens-core): Kafka access, UI-agnostic
├── macos/     # Native SwiftUI app, talks to the core via UniFFI
├── scripts/   # Build, binding generation, packaging, dev setup
└── docs/      # Release process
```

- **core (Rust)**: cluster metadata, topic permissions, reading and producing
  messages ([rdkafka](https://github.com/fede1024/rust-rdkafka)), and payload
  formatting. Exposed to frontends through bindings generated with
  [UniFFI](https://github.com/mozilla/uniffi-rs), so every frontend shows
  messages the same way.
- **macOS (SwiftUI)**: native frontend that calls the core in-process.
- **Windows / Linux**: future frontends built on the same core.

## Building from source

Requirements: Rust (stable) and the macOS Command Line Tools (Xcode is not
required). Docker for the development brokers.

```sh
scripts/build-core.sh             # Rust core -> macos/RustLib (XCFramework + Swift bindings)
cd macos && swift run             # run the app from source
scripts/bundle-app.sh             # release build -> dist/Kafkalens.app
scripts/bundle-app.sh --install   # same, then copy to /Applications
scripts/bundle-app.sh --universal # Apple Silicon + Intel, as releases ship
```

Re-run `scripts/build-core.sh` whenever the core's public API changes.

## Local Kafka

```sh
docker compose up -d                              # single-node KRaft broker on localhost:9092
cargo run -p kafkalens-core --example seed        # sample topics: JSON, text, binary, Schema Registry
```

To test topic permissions, a second broker with SASL and ACLs runs on
`localhost:9094` (users `admin/admin` and `reader/reader`; `reader` gets a
different mix of Describe/Read/Write on each `acl.*` topic):

```sh
docker compose --profile acl up -d
scripts/dev-acl-setup.sh
```

## Tests

```sh
cargo test -p kafkalens-core                                                        # unit tests
KAFKALENS_BOOTSTRAP=localhost:9092 cargo test -p kafkalens-core --test live         # local broker
KAFKALENS_ACL_BOOTSTRAP=localhost:9094 cargo test -p kafkalens-core --test live_acl # topic permissions
```

CI runs all of them on Linux against both brokers, and builds the macOS app.

## Releasing

See [docs/RELEASING.md](docs/RELEASING.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

Apache Kafka is a trademark of The Apache Software Foundation. Kafkalens is
not affiliated with or endorsed by the ASF.
