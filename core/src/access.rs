//! Per-topic permissions of the connected principal, from the operations the
//! broker reports as authorized (KIP-430) in a DescribeTopics request.
//! rust-rdkafka does not wrap DescribeTopics, so this calls librdkafka directly.

use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::time::Duration;

use rdkafka::bindings as rd;
use rdkafka::bindings::rd_kafka_AclOperation_t as Op;
use rdkafka::bindings::rd_kafka_resp_err_t as Err;

/// `None` means the broker did not say, e.g. a cluster without an authorizer
/// or a broker older than Kafka 2.3.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TopicAccess {
    pub can_read: Option<bool>,
    pub can_write: Option<bool>,
}

impl TopicAccess {
    const DENIED: TopicAccess = TopicAccess {
        can_read: Some(false),
        can_write: Some(false),
    };

    fn from_operations(ops: &[Op]) -> Self {
        let allows = |op: Op| ops.iter().any(|&o| o == op || o == Op::RD_KAFKA_ACL_OPERATION_ALL);
        TopicAccess {
            can_read: Some(allows(Op::RD_KAFKA_ACL_OPERATION_READ)),
            can_write: Some(allows(Op::RD_KAFKA_ACL_OPERATION_WRITE)),
        }
    }
}

/// Permissions for `topics`, keyed by name. Returns an empty map when the
/// request fails as a whole, so callers fall back to "unknown" rather than
/// hiding topics on a transient error.
pub(crate) fn describe_topic_access(
    rk: *mut rd::rd_kafka_t,
    topics: &[String],
    timeout: Duration,
) -> HashMap<String, TopicAccess> {
    let names: Vec<CString> = topics
        .iter()
        .filter_map(|t| CString::new(t.as_str()).ok())
        .collect();
    if names.is_empty() {
        return HashMap::new();
    }
    let mut name_ptrs: Vec<*const c_char> = names.iter().map(|n| n.as_ptr()).collect();
    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;

    // SAFETY: `rk` is a live client handle owned by the caller for the whole
    // call. Every object created here is destroyed before returning, and
    // pointers read from the result event are only used while it is alive.
    unsafe {
        let collection = rd::rd_kafka_TopicCollection_of_topic_names(name_ptrs.as_mut_ptr(), name_ptrs.len());
        let options = rd::rd_kafka_AdminOptions_new(rk, rd::rd_kafka_admin_op_t::RD_KAFKA_ADMIN_OP_DESCRIBETOPICS);
        let mut errstr = [0 as c_char; 256];
        rd::rd_kafka_AdminOptions_set_request_timeout(options, timeout_ms, errstr.as_mut_ptr(), errstr.len());
        let err = rd::rd_kafka_AdminOptions_set_include_authorized_operations(options, 1);
        if !err.is_null() {
            rd::rd_kafka_error_destroy(err);
        }

        let queue = rd::rd_kafka_queue_new(rk);
        rd::rd_kafka_DescribeTopics(rk, collection, options, queue);
        // librdkafka enforces the request timeout itself; the margin only
        // guards against never getting an event at all.
        let event = rd::rd_kafka_queue_poll(queue, timeout_ms.saturating_add(2000));

        let access = if event.is_null() {
            HashMap::new()
        } else {
            let parsed = parse_result(event);
            rd::rd_kafka_event_destroy(event);
            parsed
        };

        rd::rd_kafka_queue_destroy(queue);
        rd::rd_kafka_AdminOptions_destroy(options);
        rd::rd_kafka_TopicCollection_destroy(collection);
        access
    }
}

unsafe fn parse_result(event: *mut rd::rd_kafka_event_t) -> HashMap<String, TopicAccess> {
    let mut access = HashMap::new();
    if rd::rd_kafka_event_error(event) != Err::RD_KAFKA_RESP_ERR_NO_ERROR {
        return access;
    }
    let result = rd::rd_kafka_event_DescribeTopics_result(event);
    if result.is_null() {
        return access;
    }

    let mut count = 0usize;
    let descriptions = rd::rd_kafka_DescribeTopics_result_topics(result, &mut count);
    if descriptions.is_null() {
        return access;
    }

    for &desc in std::slice::from_raw_parts(descriptions, count) {
        let name = rd::rd_kafka_TopicDescription_name(desc);
        if name.is_null() {
            continue;
        }
        let name = CStr::from_ptr(name).to_string_lossy().into_owned();

        let error = rd::rd_kafka_TopicDescription_error(desc);
        if !error.is_null() {
            if rd::rd_kafka_error_code(error) == Err::RD_KAFKA_RESP_ERR_TOPIC_AUTHORIZATION_FAILED {
                access.insert(name, TopicAccess::DENIED);
            }
            continue;
        }

        let mut op_count = 0usize;
        let ops = rd::rd_kafka_TopicDescription_authorized_operations(desc, &mut op_count);
        // No list means the broker did not report operations: leave unknown.
        if !ops.is_null() && op_count > 0 {
            let ops = std::slice::from_raw_parts(ops, op_count);
            access.insert(name, TopicAccess::from_operations(ops));
        }
    }
    access
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_grants_read_and_write() {
        let access = TopicAccess::from_operations(&[Op::RD_KAFKA_ACL_OPERATION_ALL]);
        assert_eq!(access.can_read, Some(true));
        assert_eq!(access.can_write, Some(true));
    }

    #[test]
    fn describe_only_denies_read_and_write() {
        let access = TopicAccess::from_operations(&[Op::RD_KAFKA_ACL_OPERATION_DESCRIBE]);
        assert_eq!(access.can_read, Some(false));
        assert_eq!(access.can_write, Some(false));
    }

    #[test]
    fn read_without_write() {
        let access = TopicAccess::from_operations(&[
            Op::RD_KAFKA_ACL_OPERATION_DESCRIBE,
            Op::RD_KAFKA_ACL_OPERATION_READ,
        ]);
        assert_eq!(access.can_read, Some(true));
        assert_eq!(access.can_write, Some(false));
    }
}
