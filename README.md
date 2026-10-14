# Kafkalens

A desktop viewer for Kafka topics. macOS first, with Windows and Linux to follow.

## Architecture

```
kafkalens/
├── core/      # Rust crate (kafkalens-core): Kafka access, UI-agnostic
├── macos/     # Native SwiftUI app, talks to the core via UniFFI
└── scripts/   # Build, binding generation and .app packaging
```

- **core (Rust)**: lists topics, partitions and watermarks, and reads messages (rdkafka). Exposed to the frontends through bindings generated with [UniFFI](https://github.com/mozilla/uniffi-rs).
- **macOS (SwiftUI)**: native frontend that calls the core in-process.
- **Windows / Linux**: future frontends built on the same core.
