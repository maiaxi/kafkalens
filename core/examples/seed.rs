//! Fills a local broker with sample topics: JSON, plain text, binary and
//! Schema Registry framed payloads, with keys and headers.
//!
//! cargo run -p kafkalens-core --example seed [bootstrap] [count]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
use rdkafka::client::DefaultClientContext;
use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{BaseRecord, DefaultProducerContext, Producer, ThreadedProducer};
use rdkafka::ClientConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let bootstrap = args.next().unwrap_or_else(|| "localhost:9092".into());
    let count: usize = args.next().map(|c| c.parse()).transpose()?.unwrap_or(500);

    let mut cfg = ClientConfig::new();
    cfg.set("bootstrap.servers", &bootstrap);

    let topics = [("orders", 3), ("logs", 1), ("telemetry.binary", 2), ("users.avro", 2)];
    create_topics(&cfg, &topics)?;

    let producer: ThreadedProducer<DefaultProducerContext> = cfg.create()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;

    for i in 0..count {
        // Spread timestamps over the last hour so timestamp seeks are testable.
        let ts = now - ((count - i) as i64 * 3_600_000 / count as i64);

        let order = format!(
            r#"{{"id":{i},"customer":"c-{}","total":{:.2},"items":[{{"sku":"A{}","qty":{}}}],"status":"{}"}}"#,
            i % 37,
            (i as f64 * 7.31) % 500.0,
            i % 11,
            1 + i % 4,
            ["created", "paid", "shipped"][i % 3],
        );
        let key = format!("order-{i}");
        let headers = OwnedHeaders::new()
            .insert(Header { key: "source", value: Some("seed") })
            .insert(Header { key: "trace-id", value: Some(&format!("{:016x}", i * 7919)) });
        send(&producer, BaseRecord::to("orders").key(&key).payload(&order).headers(headers).timestamp(ts));

        let level = ["INFO", "WARN", "ERROR", "DEBUG"][i % 4];
        let line = format!("{level} worker-{} processed batch {i}", i % 5);
        send(&producer, BaseRecord::<(), _>::to("logs").payload(&line).timestamp(ts));

        let binary: Vec<u8> = (0..24).map(|b| ((i * 31 + b * 17) % 256) as u8).collect();
        let sensor = format!("sensor-{}", i % 8);
        send(&producer, BaseRecord::to("telemetry.binary").key(&sensor).payload(&binary).timestamp(ts));

        let mut framed = vec![0u8, 0, 0, 0, 7];
        framed.extend_from_slice(format!(r#"{{"user":"u{i}","active":{}}}"#, i % 2 == 0).as_bytes());
        let user = format!("u{i}");
        send(&producer, BaseRecord::to("users.avro").key(&user).payload(&framed).timestamp(ts));
    }

    producer.flush(Duration::from_secs(30))?;
    println!("produced {count} messages to each of {} topics on {bootstrap}", topics.len());
    Ok(())
}

fn send<K, P>(producer: &ThreadedProducer<DefaultProducerContext>, mut record: BaseRecord<K, P>)
where
    K: rdkafka::message::ToBytes + ?Sized,
    P: rdkafka::message::ToBytes + ?Sized,
{
    loop {
        match producer.send(record) {
            Ok(()) => return,
            Err((err, rec)) => {
                if err.rdkafka_error_code() != Some(rdkafka::types::RDKafkaErrorCode::QueueFull) {
                    panic!("produce failed: {err}");
                }
                record = rec;
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn create_topics(cfg: &ClientConfig, topics: &[(&str, i32)]) -> Result<(), Box<dyn std::error::Error>> {
    let admin: AdminClient<DefaultClientContext> = cfg.create()?;
    let new_topics: Vec<NewTopic> = topics
        .iter()
        .map(|(name, partitions)| NewTopic::new(name, *partitions, TopicReplication::Fixed(1)))
        .collect();
    let runtime = futures_executor::block_on(admin.create_topics(&new_topics, &AdminOptions::new()))?;
    for result in runtime {
        if let Err((topic, err)) = result {
            if err != rdkafka::types::RDKafkaErrorCode::TopicAlreadyExists {
                return Err(format!("creating {topic}: {err}").into());
            }
        }
    }
    Ok(())
}
