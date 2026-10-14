use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rdkafka::consumer::{BaseConsumer, Consumer};
use rdkafka::error::KafkaError;
use rdkafka::message::{Header, Headers, Message, OwnedHeaders};
use rdkafka::metadata::MetadataTopic;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::{Offset, TopicPartitionList};

use crate::access::describe_topic_access;
use crate::config::ConnectionConfig;
use crate::error::{KafkaLensError, Result};
use crate::models::*;

/// A connection to one Kafka cluster. Cheap metadata calls share one consumer;
/// each fetch uses its own short-lived consumer so concurrent fetches do not
/// fight over partition assignments. The producer is only created on the
/// first produce, so read-only use never opens one.
#[derive(uniffi::Object)]
pub struct KafkaClient {
    config: ConnectionConfig,
    metadata_consumer: BaseConsumer,
    producer: Mutex<Option<FutureProducer>>,
    timeout: Duration,
}

#[uniffi::export]
impl KafkaClient {
    #[uniffi::constructor]
    pub fn new(config: ConnectionConfig) -> Result<Arc<Self>> {
        let metadata_consumer = config.consumer_config()?.create()?;
        let timeout = Duration::from_millis(config.request_timeout_ms.max(1000));
        Ok(Arc::new(Self {
            config,
            metadata_consumer,
            producer: Mutex::new(None),
            timeout,
        }))
    }

    pub fn is_read_only(&self) -> bool {
        self.config.read_only
    }

    /// Sends one message and waits for the broker to acknowledge it.
    pub fn produce_message(&self, request: ProduceRequest) -> Result<ProduceResult> {
        if self.config.read_only {
            return Err(KafkaLensError::ReadOnly);
        }
        let producer = self.producer()?;

        let mut headers = OwnedHeaders::new_with_capacity(request.headers.len());
        for header in &request.headers {
            headers = headers.insert(Header {
                key: &header.key,
                value: header.value.as_deref(),
            });
        }

        let mut record = FutureRecord::<[u8], [u8]>::to(&request.topic).headers(headers);
        if let Some(partition) = request.partition {
            record = record.partition(partition);
        }
        if let Some(key) = &request.key {
            record = record.key(key.as_slice());
        }
        if let Some(value) = &request.value {
            record = record.payload(value.as_slice());
        }

        let delivery = producer.send_result(record).map_err(|(err, _)| err)?;
        // `message.timeout.ms` bounds the wait: librdkafka always reports
        // delivery or failure within it.
        match futures_executor::block_on(delivery) {
            Ok(Ok(delivered)) => Ok(ProduceResult {
                partition: delivered.partition,
                offset: delivered.offset,
            }),
            Ok(Err((err, _))) => Err(err.into()),
            Err(_) => Err(KafkaLensError::Kafka {
                message: "producer shut down before delivery".into(),
            }),
        }
    }

    /// Round-trips to the cluster; use it to validate a connection.
    pub fn cluster_info(&self) -> Result<ClusterInfo> {
        let metadata = self.metadata_consumer.fetch_metadata(None, self.timeout)?;
        Ok(ClusterInfo {
            brokers: metadata
                .brokers()
                .iter()
                .map(|b| BrokerInfo {
                    id: b.id(),
                    host: b.host().to_string(),
                    port: b.port(),
                })
                .collect(),
            origin_broker_id: metadata.orig_broker_id(),
            topic_count: metadata.topics().len() as u32,
        })
    }

    /// All topics, sorted by name, with the principal's read/write access.
    pub fn list_topics(&self) -> Result<Vec<TopicSummary>> {
        let metadata = self.metadata_consumer.fetch_metadata(None, self.timeout)?;
        let names: Vec<String> = metadata
            .topics()
            .iter()
            .filter(|t| t.error().is_none())
            .map(|t| t.name().to_string())
            .collect();
        let access = describe_topic_access(
            self.metadata_consumer.client().native_ptr(),
            &names,
            self.timeout,
        );

        let mut topics: Vec<TopicSummary> = metadata
            .topics()
            .iter()
            .filter(|t| t.error().is_none())
            .map(|t| {
                let access = access.get(t.name()).copied().unwrap_or_default();
                TopicSummary {
                    name: t.name().to_string(),
                    partition_count: t.partitions().len() as u32,
                    is_internal: t.name().starts_with("__"),
                    can_read: access.can_read,
                    can_write: access.can_write,
                }
            })
            .collect();
        topics.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(topics)
    }

    /// Partition layout and watermarks of one topic.
    pub fn describe_topic(&self, topic: String) -> Result<TopicDetails> {
        let metadata = self
            .metadata_consumer
            .fetch_metadata(Some(&topic), self.timeout)?;
        let meta = find_topic(metadata.topics(), &topic)?;

        let mut partitions = Vec::with_capacity(meta.partitions().len());
        for p in meta.partitions() {
            let (low, high) =
                self.metadata_consumer
                    .fetch_watermarks(&topic, p.id(), self.timeout)?;
            partitions.push(PartitionInfo {
                id: p.id(),
                leader: p.leader(),
                replicas: p.replicas().to_vec(),
                in_sync_replicas: p.isr().to_vec(),
                low_watermark: low,
                high_watermark: high,
            });
        }
        partitions.sort_by_key(|p| p.id);

        let approximate_message_count = partitions
            .iter()
            .map(|p| (p.high_watermark - p.low_watermark).max(0))
            .sum();

        Ok(TopicDetails {
            name: topic,
            partitions,
            approximate_message_count,
        })
    }

    /// Reads up to `max_messages` messages without committing any offsets.
    pub fn fetch_messages(&self, request: FetchRequest) -> Result<FetchResult> {
        let details = self.describe_topic(request.topic.clone())?;
        let selected: Vec<&PartitionInfo> = details
            .partitions
            .iter()
            .filter(|p| match &request.partitions {
                Some(ids) => ids.contains(&p.id),
                None => true,
            })
            .collect();

        let max = request.max_messages.max(1) as i64;
        let starts = self.start_offsets(&request, &selected, max)?;

        let mut cursors: HashMap<i32, PartitionCursor> = selected
            .iter()
            .map(|p| {
                let start = starts.get(&p.id).copied().unwrap_or(p.high_watermark);
                (
                    p.id,
                    PartitionCursor {
                        partition: p.id,
                        next_offset: start,
                        high_watermark: p.high_watermark,
                    },
                )
            })
            .collect();

        // Partitions that still have messages between their start and the
        // high watermark captured above.
        let mut pending: HashMap<i32, i64> = cursors
            .values()
            .filter(|c| c.next_offset < c.high_watermark)
            .map(|c| (c.partition, c.high_watermark))
            .collect();

        let mut messages = Vec::new();
        let mut timed_out = false;

        if !pending.is_empty() {
            let consumer: BaseConsumer = self.config.consumer_config()?.create()?;
            let mut tpl = TopicPartitionList::new();
            for &partition in pending.keys() {
                let start = cursors[&partition].next_offset;
                tpl.add_partition_offset(&request.topic, partition, Offset::Offset(start))?;
            }
            consumer.assign(&tpl)?;

            // In `Latest` mode every partition contributes up to `max` and the
            // newest ones are kept below; otherwise stop as soon as we have `max`.
            let stop_at_limit = request.start != StartPosition::Latest;
            let deadline = Instant::now() + Duration::from_millis(request.timeout_ms.max(100));

            while !pending.is_empty() && !(stop_at_limit && messages.len() as i64 >= max) {
                let now = Instant::now();
                if now >= deadline {
                    timed_out = true;
                    break;
                }
                let wait = (deadline - now).min(Duration::from_millis(200));
                match consumer.poll(wait) {
                    None => {}
                    Some(Err(KafkaError::PartitionEOF(partition))) => {
                        pending.remove(&partition);
                    }
                    // The throwaway group.id makes librdkafka look up a group
                    // coordinator even though partitions are assigned by hand
                    // and nothing is committed. Clusters that grant topic Read
                    // but no group ACLs answer that with an error which says
                    // nothing about the read itself, so skip it.
                    Some(Err(err)) if is_group_authorization_error(&err) => {}
                    Some(Err(err)) => return Err(err.into()),
                    Some(Ok(msg)) => {
                        let partition = msg.partition();
                        let Some(&high) = pending.get(&partition) else {
                            continue;
                        };
                        if msg.offset() >= high {
                            pending.remove(&partition);
                            continue;
                        }
                        if let Some(cursor) = cursors.get_mut(&partition) {
                            cursor.next_offset = msg.offset() + 1;
                        }
                        if msg.offset() + 1 >= high {
                            pending.remove(&partition);
                        }
                        messages.push(to_message(&msg));
                    }
                }
            }
        }

        messages.sort_by_key(|m| (m.timestamp_ms.unwrap_or(i64::MIN), m.partition, m.offset));
        let max = max as usize;
        if messages.len() > max {
            if request.start == StartPosition::Latest {
                messages.drain(..messages.len() - max);
            } else {
                messages.truncate(max);
            }
        }

        let mut cursors: Vec<PartitionCursor> = cursors.into_values().collect();
        cursors.sort_by_key(|c| c.partition);

        Ok(FetchResult {
            messages,
            cursors,
            timed_out,
        })
    }
}

impl KafkaClient {
    fn producer(&self) -> Result<FutureProducer> {
        let mut slot = self.producer.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(producer) = slot.as_ref() {
            return Ok(producer.clone());
        }
        let producer: FutureProducer = self.config.producer_config()?.create()?;
        *slot = Some(producer.clone());
        Ok(producer)
    }

    /// First offset to read on each selected partition.
    fn start_offsets(
        &self,
        request: &FetchRequest,
        partitions: &[&PartitionInfo],
        max: i64,
    ) -> Result<HashMap<i32, i64>> {
        let clamp = |p: &PartitionInfo, offset: i64| offset.clamp(p.low_watermark, p.high_watermark);

        match &request.start {
            StartPosition::Beginning => Ok(partitions.iter().map(|p| (p.id, p.low_watermark)).collect()),
            StartPosition::Latest => Ok(partitions
                .iter()
                .map(|p| (p.id, clamp(p, p.high_watermark - max)))
                .collect()),
            StartPosition::Offset { offset } => {
                Ok(partitions.iter().map(|p| (p.id, clamp(p, *offset))).collect())
            }
            StartPosition::Timestamp { millis } => {
                let mut tpl = TopicPartitionList::new();
                for p in partitions {
                    tpl.add_partition_offset(&request.topic, p.id, Offset::Offset(*millis))?;
                }
                let resolved = self.metadata_consumer.offsets_for_times(tpl, self.timeout)?;
                Ok(partitions
                    .iter()
                    .map(|p| {
                        let offset = resolved
                            .find_partition(&request.topic, p.id)
                            .and_then(|e| match e.offset() {
                                Offset::Offset(o) => Some(o),
                                _ => None,
                            })
                            // No message at or after the timestamp.
                            .unwrap_or(p.high_watermark);
                        (p.id, clamp(p, offset))
                    })
                    .collect())
            }
        }
    }
}

fn is_group_authorization_error(err: &KafkaError) -> bool {
    err.rdkafka_error_code() == Some(rdkafka::types::RDKafkaErrorCode::GroupAuthorizationFailed)
}

fn find_topic<'a>(topics: &'a [MetadataTopic], name: &str) -> Result<&'a MetadataTopic> {
    topics
        .iter()
        .find(|t| t.name() == name && t.error().is_none() && !t.partitions().is_empty())
        .ok_or_else(|| KafkaLensError::TopicNotFound {
            topic: name.to_string(),
        })
}

fn to_message<M: Message>(msg: &M) -> KafkaMessage {
    let headers = msg
        .headers()
        .map(|hs| {
            hs.iter()
                .map(|h| MessageHeader {
                    key: h.key.to_string(),
                    value: h.value.map(|v| v.to_vec()),
                })
                .collect()
        })
        .unwrap_or_default();

    KafkaMessage {
        partition: msg.partition(),
        offset: msg.offset(),
        timestamp_ms: msg.timestamp().to_millis(),
        key: msg.key().map(|k| k.to_vec()),
        value: msg.payload().map(|v| v.to_vec()),
        headers,
    }
}
