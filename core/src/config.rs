use rdkafka::ClientConfig;

use crate::error::{KafkaLensError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SecurityProtocol {
    Plaintext,
    Ssl,
    SaslPlaintext,
    SaslSsl,
}

impl SecurityProtocol {
    fn as_str(self) -> &'static str {
        match self {
            SecurityProtocol::Plaintext => "plaintext",
            SecurityProtocol::Ssl => "ssl",
            SecurityProtocol::SaslPlaintext => "sasl_plaintext",
            SecurityProtocol::SaslSsl => "sasl_ssl",
        }
    }

    fn uses_sasl(self) -> bool {
        matches!(self, SecurityProtocol::SaslPlaintext | SecurityProtocol::SaslSsl)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SaslMechanism {
    Plain,
    ScramSha256,
    ScramSha512,
}

impl SaslMechanism {
    fn as_str(self) -> &'static str {
        match self {
            SaslMechanism::Plain => "PLAIN",
            SaslMechanism::ScramSha256 => "SCRAM-SHA-256",
            SaslMechanism::ScramSha512 => "SCRAM-SHA-512",
        }
    }
}

/// Raw librdkafka property, for anything the typed fields do not cover.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ConnectionConfig {
    /// Comma-separated `host:port` list.
    pub bootstrap_servers: String,
    pub security_protocol: SecurityProtocol,
    pub sasl_mechanism: Option<SaslMechanism>,
    pub sasl_username: Option<String>,
    pub sasl_password: Option<String>,
    /// Timeout for metadata, watermark and produce requests.
    pub request_timeout_ms: u64,
    /// When true the client refuses to produce. Enforced here rather than
    /// only in the UI so no frontend bug can write to a protected cluster.
    pub read_only: bool,
    pub extra: Vec<ConfigEntry>,
}

impl ConnectionConfig {
    /// Settings for reading: never joins a real group or commits offsets.
    pub(crate) fn consumer_config(&self) -> Result<ClientConfig> {
        let mut cfg = self.base_config()?;
        cfg.set("group.id", format!("kafkalens-{}", uuid::Uuid::new_v4()))
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("enable.partition.eof", "true");
        self.apply_extra(&mut cfg);
        Ok(cfg)
    }

    pub(crate) fn producer_config(&self) -> Result<ClientConfig> {
        let mut cfg = self.base_config()?;
        cfg.set("acks", "all")
            .set("message.timeout.ms", self.request_timeout_ms.max(5000).to_string())
            // Send each message straight away instead of batching.
            .set("linger.ms", "0");
        self.apply_extra(&mut cfg);
        Ok(cfg)
    }

    fn base_config(&self) -> Result<ClientConfig> {
        let servers = self.bootstrap_servers.trim();
        if servers.is_empty() {
            return Err(KafkaLensError::InvalidConfig {
                message: "bootstrap servers must not be empty".into(),
            });
        }

        let mut cfg = ClientConfig::new();
        cfg.set("bootstrap.servers", servers)
            .set("security.protocol", self.security_protocol.as_str())
            .set("client.id", "kafkalens")
            .set("socket.timeout.ms", self.request_timeout_ms.max(1000).to_string());

        if self.security_protocol.uses_sasl() {
            let mechanism = self.sasl_mechanism.unwrap_or(SaslMechanism::Plain);
            cfg.set("sasl.mechanism", mechanism.as_str());
            if let Some(user) = &self.sasl_username {
                cfg.set("sasl.username", user);
            }
            if let Some(pass) = &self.sasl_password {
                cfg.set("sasl.password", pass);
            }
        }

        Ok(cfg)
    }

    /// User-supplied properties go last so they override everything above.
    fn apply_extra(&self, cfg: &mut ClientConfig) {
        for entry in &self.extra {
            let key = entry.key.trim();
            if !key.is_empty() {
                cfg.set(key, entry.value.trim());
            }
        }
    }
}
