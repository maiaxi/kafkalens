#!/usr/bin/env bash
# Creates topics and ACLs on the `kafka-acl` dev broker so user `reader` sees
# a mix of permissions:
#
#   acl.readable       Describe + Read
#   acl.writable       Describe + Write
#   acl.describe-only  Describe (listed, but reading fails)
#   acl.hidden         nothing (not even listed)
#
# Usage: docker compose --profile acl up -d && scripts/dev-acl-setup.sh
set -euo pipefail

CONTAINER=kafkalens-kafka-acl
BIN=/opt/kafka/bin

docker exec -i "$CONTAINER" sh -c 'cat > /tmp/admin.properties' <<'PROPS'
security.protocol=SASL_PLAINTEXT
sasl.mechanism=PLAIN
sasl.jaas.config=org.apache.kafka.common.security.plain.PlainLoginModule required username="admin" password="admin";
PROPS

kafka() {
  docker exec "$CONTAINER" "$BIN/$1" --bootstrap-server localhost:9094 \
    --command-config /tmp/admin.properties "${@:2}"
}

echo "waiting for broker…"
for _ in $(seq 1 30); do
  kafka kafka-topics.sh --list >/dev/null 2>&1 && break
  sleep 2
done

for topic in acl.readable acl.writable acl.describe-only acl.hidden; do
  kafka kafka-topics.sh --create --if-not-exists --topic "$topic" --partitions 2 >/dev/null
  # Some data, so reading actually issues a fetch (an empty topic never does).
  echo "{\"topic\":\"$topic\"}" | docker exec -i "$CONTAINER" "$BIN/kafka-console-producer.sh" \
    --bootstrap-server localhost:9094 --producer.config /tmp/admin.properties --topic "$topic"
done

acl() {
  kafka kafka-acls.sh --add --allow-principal User:reader --topic "$1" "${@:2}" >/dev/null
}
acl acl.readable --operation Describe --operation Read
acl acl.writable --operation Describe --operation Write
acl acl.describe-only --operation Describe

echo "acl broker ready on localhost:9094 (SASL_PLAINTEXT, PLAIN, reader/reader)"
