//! Kafkalens core: UI-agnostic Kafka access, exposed to the native frontends
//! through UniFFI. Every call is blocking; frontends are expected to call it
//! off their main thread.

mod access;
mod client;
mod config;
mod error;
mod format;
mod models;

pub use client::KafkaClient;
pub use config::{ConfigEntry, ConnectionConfig, SaslMechanism, SecurityProtocol};
pub use error::KafkaLensError;
pub use format::{
    editable_payload, encode_payload, format_payload, EditablePayload, FormattedPayload,
    InputEncoding, PayloadFormat,
};
pub use models::*;

uniffi::setup_scaffolding!();

/// Version of the core library, so frontends can show it in an About panel.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
