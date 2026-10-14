#[derive(Debug, Clone, uniffi::Record)]
pub struct BrokerInfo {
    pub id: i32,
    pub host: String,
    pub port: i32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ClusterInfo {
    pub brokers: Vec<BrokerInfo>,
    /// Id of the broker that answered the metadata request.
    pub origin_broker_id: i32,
    pub topic_count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TopicSummary {
    pub name: String,
    pub partition_count: u32,
    /// Topics whose name starts with `__` (e.g. `__consumer_offsets`).
    pub is_internal: bool,
    /// Whether the connected principal may read / produce, from the ACLs the
    /// broker reports. `None` when it does not report them (no authorizer,
    /// or a broker older than Kafka 2.3); treat that as allowed.
    pub can_read: Option<bool>,
    pub can_write: Option<bool>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PartitionInfo {
    pub id: i32,
    pub leader: i32,
    pub replicas: Vec<i32>,
    pub in_sync_replicas: Vec<i32>,
    pub low_watermark: i64,
    pub high_watermark: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TopicDetails {
    pub name: String,
    pub partitions: Vec<PartitionInfo>,
    /// Sum of `high - low` over all partitions. An upper bound on compacted topics.
    pub approximate_message_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum StartPosition {
    /// Oldest available offset on each partition.
    Beginning,
    /// The newest `max_messages` messages of the topic.
    Latest,
    /// The given offset on each selected partition (clamped to the watermarks).
    Offset { offset: i64 },
    /// First offset whose timestamp is >= `millis` (Unix epoch ms).
    Timestamp { millis: i64 },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FetchRequest {
    pub topic: String,
    /// `None` reads every partition.
    pub partitions: Option<Vec<i32>>,
    pub start: StartPosition,
    pub max_messages: u32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MessageHeader {
    pub key: String,
    pub value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct KafkaMessage {
    pub partition: i32,
    pub offset: i64,
    /// Unix epoch ms, if the broker reported one.
    pub timestamp_ms: Option<i64>,
    pub key: Option<Vec<u8>>,
    pub value: Option<Vec<u8>>,
    pub headers: Vec<MessageHeader>,
}

/// Where a fetch stopped on one partition, so a frontend can page forward.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PartitionCursor {
    pub partition: i32,
    pub next_offset: i64,
    pub high_watermark: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ProduceRequest {
    pub topic: String,
    /// `None` lets the partitioner choose (by key hash, or spread when keyless).
    pub partition: Option<i32>,
    pub key: Option<Vec<u8>>,
    /// `None` produces a tombstone.
    pub value: Option<Vec<u8>>,
    pub headers: Vec<MessageHeader>,
}

/// Where the broker stored a produced message.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ProduceResult {
    pub partition: i32,
    pub offset: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FetchResult {
    /// Sorted by timestamp, then partition, then offset (oldest first).
    pub messages: Vec<KafkaMessage>,
    pub cursors: Vec<PartitionCursor>,
    /// True when the fetch hit its timeout before reaching the end or the limit.
    pub timed_out: bool,
}
