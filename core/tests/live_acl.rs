//! Topic permissions against the ACL-enabled dev broker. Skipped unless
//! KAFKALENS_ACL_BOOTSTRAP is set:
//!
//! docker compose --profile acl up -d && scripts/dev-acl-setup.sh
//! KAFKALENS_ACL_BOOTSTRAP=localhost:9094 cargo test -p kafkalens-core --test live_acl

use std::sync::Arc;

use kafkalens_core::*;

fn reader() -> Option<Arc<KafkaClient>> {
    let bootstrap = std::env::var("KAFKALENS_ACL_BOOTSTRAP").ok()?;
    Some(
        KafkaClient::new(ConnectionConfig {
            bootstrap_servers: bootstrap,
            security_protocol: SecurityProtocol::SaslPlaintext,
            sasl_mechanism: Some(SaslMechanism::Plain),
            sasl_username: Some("reader".into()),
            sasl_password: Some("reader".into()),
            request_timeout_ms: 5000,
            read_only: false,
            extra: vec![],
        })
        .expect("client"),
    )
}

#[test]
fn reports_per_topic_permissions() {
    let Some(client) = reader() else { return };

    let topics = client.list_topics().unwrap();
    let access = |name: &str| {
        let topic = topics.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("{name} not listed"));
        (topic.can_read, topic.can_write)
    };

    assert_eq!(access("acl.readable"), (Some(true), Some(false)));
    assert_eq!(access("acl.writable"), (Some(false), Some(true)));
    assert_eq!(access("acl.describe-only"), (Some(false), Some(false)));
    assert!(topics.iter().all(|t| t.name != "acl.hidden"), "topic without Describe must not be listed");
}

#[test]
fn reading_a_describe_only_topic_fails() {
    let Some(client) = reader() else { return };

    let result = client.fetch_messages(FetchRequest {
        topic: "acl.describe-only".into(),
        partitions: None,
        start: StartPosition::Beginning,
        max_messages: 10,
        timeout_ms: 5000,
    });
    match result {
        Err(KafkaLensError::Kafka { message }) => assert!(message.contains("authorization"), "{message}"),
        other => panic!("expected an authorization error, got {other:?}"),
    }
}

#[test]
fn reading_a_readable_topic_needs_no_group_permissions() {
    let Some(client) = reader() else { return };

    // `reader` has no ACLs on any consumer group. Run a few times: the
    // failure this guards against depended on timing.
    for _ in 0..5 {
        let result = client
            .fetch_messages(FetchRequest {
                topic: "acl.readable".into(),
                partitions: None,
                start: StartPosition::Beginning,
                max_messages: 10,
                timeout_ms: 5000,
            })
            .expect("fetch from a readable topic");
        assert!(!result.messages.is_empty());
    }
}
