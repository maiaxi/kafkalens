//! Tests against a real broker seeded by `examples/seed.rs`. Skipped unless
//! KAFKALENS_BOOTSTRAP is set:
//!
//! docker compose up -d && cargo run -p kafkalens-core --example seed
//! KAFKALENS_BOOTSTRAP=localhost:9092 cargo test -p kafkalens-core --test live

use std::sync::Arc;

use kafkalens_core::*;

fn client() -> Option<Arc<KafkaClient>> {
    client_with(true)
}

fn client_with(read_only: bool) -> Option<Arc<KafkaClient>> {
    let bootstrap = std::env::var("KAFKALENS_BOOTSTRAP").ok()?;
    Some(
        KafkaClient::new(ConnectionConfig {
            bootstrap_servers: bootstrap,
            security_protocol: SecurityProtocol::Plaintext,
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
            request_timeout_ms: 5000,
            read_only,
            extra: vec![],
        })
        .expect("client"),
    )
}

fn fetch(client: &KafkaClient, topic: &str, start: StartPosition, max: u32) -> FetchResult {
    client
        .fetch_messages(FetchRequest {
            topic: topic.into(),
            partitions: None,
            start,
            max_messages: max,
            timeout_ms: 10_000,
        })
        .expect("fetch")
}

#[test]
fn lists_and_describes_topics() {
    let Some(client) = client() else { return };

    assert!(!client.cluster_info().unwrap().brokers.is_empty());

    let topics = client.list_topics().unwrap();
    let orders = topics.iter().find(|t| t.name == "orders").expect("orders topic");
    assert_eq!(orders.partition_count, 3);
    // No authorizer: nothing may be reported as denied, or topics would be hidden.
    assert!(topics.iter().all(|t| t.can_read != Some(false) && t.can_write != Some(false)));

    let details = client.describe_topic("orders".into()).unwrap();
    assert_eq!(details.partitions.len(), 3);
    assert!(details.approximate_message_count >= 500);

    assert!(matches!(
        client.describe_topic("does-not-exist".into()),
        Err(KafkaLensError::TopicNotFound { .. })
    ));
}

#[test]
fn fetches_from_beginning_and_latest() {
    let Some(client) = client() else { return };

    let first = fetch(&client, "orders", StartPosition::Beginning, 50);
    assert_eq!(first.messages.len(), 50);
    assert!(!first.timed_out);

    let latest = fetch(&client, "orders", StartPosition::Latest, 20);
    assert_eq!(latest.messages.len(), 20);
    let newest = latest.messages.last().unwrap().timestamp_ms.unwrap();
    let oldest_first = first.messages.first().unwrap().timestamp_ms.unwrap();
    assert!(newest > oldest_first);

    let msg = &latest.messages[0];
    assert!(msg.key.is_some());
    assert!(msg.headers.iter().any(|h| h.key == "source"));
}

#[test]
fn fetches_from_offset_and_timestamp() {
    let Some(client) = client() else { return };

    let result = client
        .fetch_messages(FetchRequest {
            topic: "logs".into(),
            partitions: Some(vec![0]),
            start: StartPosition::Offset { offset: 10 },
            max_messages: 5,
            timeout_ms: 10_000,
        })
        .unwrap();
    let offsets: Vec<i64> = result.messages.iter().map(|m| m.offset).collect();
    assert_eq!(offsets, vec![10, 11, 12, 13, 14]);
    assert_eq!(result.cursors[0].next_offset, 15);

    let all = fetch(&client, "logs", StartPosition::Beginning, 1000);
    let pivot = all.messages[all.messages.len() / 2].timestamp_ms.unwrap();
    let since = fetch(&client, "logs", StartPosition::Timestamp { millis: pivot }, 1000);
    assert!(!since.messages.is_empty());
    assert!(since.messages.iter().all(|m| m.timestamp_ms.unwrap() >= pivot));
}

#[test]
fn reading_past_the_end_returns_quickly() {
    let Some(client) = client() else { return };

    let result = fetch(&client, "logs", StartPosition::Offset { offset: i64::MAX }, 10);
    assert!(result.messages.is_empty());
    assert!(!result.timed_out);
}

fn produce_request(value: Option<&[u8]>) -> ProduceRequest {
    ProduceRequest {
        topic: "logs".into(),
        partition: Some(0),
        key: Some(b"kafkalens-test".to_vec()),
        value: value.map(<[u8]>::to_vec),
        headers: vec![MessageHeader {
            key: "origin".into(),
            value: Some(b"live-test".to_vec()),
        }],
    }
}

#[test]
fn read_only_connection_refuses_to_produce() {
    let Some(client) = client_with(true) else { return };

    assert!(client.is_read_only());
    assert!(matches!(
        client.produce_message(produce_request(Some(b"nope"))),
        Err(KafkaLensError::ReadOnly)
    ));
}

#[test]
fn produced_message_can_be_read_back() {
    let Some(client) = client_with(false) else { return };

    let value = format!("produced at {:?}", std::time::SystemTime::now());
    let sent = client
        .produce_message(produce_request(Some(value.as_bytes())))
        .expect("produce");
    assert_eq!(sent.partition, 0);

    let read = client
        .fetch_messages(FetchRequest {
            topic: "logs".into(),
            partitions: Some(vec![0]),
            start: StartPosition::Offset { offset: sent.offset },
            max_messages: 1,
            timeout_ms: 10_000,
        })
        .unwrap();
    let msg = &read.messages[0];
    assert_eq!(msg.offset, sent.offset);
    assert_eq!(msg.value.as_deref(), Some(value.as_bytes()));
    assert_eq!(msg.key.as_deref(), Some(&b"kafkalens-test"[..]));
    assert_eq!(msg.headers[0].key, "origin");

    // Tombstones (null value) are allowed too.
    let tombstone = client.produce_message(produce_request(None)).expect("tombstone");
    assert_eq!(tombstone.offset, sent.offset + 1);
}
