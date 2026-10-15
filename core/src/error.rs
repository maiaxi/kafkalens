use rdkafka::error::KafkaError;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum KafkaLensError {
    #[error("invalid configuration: {message}")]
    InvalidConfig { message: String },

    #[error("topic not found: {topic}")]
    TopicNotFound { topic: String },

    #[error("this connection is read-only")]
    ReadOnly,

    #[error("{message}")]
    InvalidPayload { message: String },

    #[error("kafka error: {message}")]
    Kafka { message: String },
}

impl From<KafkaError> for KafkaLensError {
    fn from(err: KafkaError) -> Self {
        KafkaLensError::Kafka {
            message: err.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, KafkaLensError>;
