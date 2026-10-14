//! Payload rendering shared by every frontend, so key/value display is
//! consistent across platforms.

use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PayloadFormat {
    /// JSON when it parses, else UTF-8 text, else hex.
    Auto,
    Text,
    Json,
    Hex,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FormattedPayload {
    pub text: String,
    /// The format actually used (never `Auto`). `Json` can fall back to `Text`
    /// or `Hex`, and `Text` to `Hex`, when the bytes do not fit.
    pub format: PayloadFormat,
    /// Schema id when the bytes use the Confluent Schema Registry wire format
    /// (magic byte 0 followed by a 4-byte big-endian id). The id is stripped
    /// before rendering.
    pub schema_id: Option<i32>,
    pub size_bytes: u64,
}

/// Renders `bytes` for display. `pretty` controls JSON indentation.
#[uniffi::export]
pub fn format_payload(bytes: Vec<u8>, format: PayloadFormat, pretty: bool) -> FormattedPayload {
    let size_bytes = bytes.len() as u64;
    let (schema_id, body) = split_schema_id(&bytes);

    let (text, format) = match format {
        PayloadFormat::Hex => (to_hex(body), PayloadFormat::Hex),
        PayloadFormat::Text => match std::str::from_utf8(body) {
            Ok(s) => (s.to_string(), PayloadFormat::Text),
            Err(_) => (to_hex(body), PayloadFormat::Hex),
        },
        PayloadFormat::Json | PayloadFormat::Auto => match render_json(body, pretty) {
            Some(json) => (json, PayloadFormat::Json),
            None => match std::str::from_utf8(body) {
                Ok(s) => (s.to_string(), PayloadFormat::Text),
                Err(_) => (to_hex(body), PayloadFormat::Hex),
            },
        },
    };

    FormattedPayload {
        text,
        format,
        schema_id,
        size_bytes,
    }
}

fn split_schema_id(bytes: &[u8]) -> (Option<i32>, &[u8]) {
    if bytes.len() > 5 && bytes[0] == 0 {
        let id = i32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        if id > 0 {
            return (Some(id), &bytes[5..]);
        }
    }
    (None, bytes)
}

fn render_json(bytes: &[u8], pretty: bool) -> Option<String> {
    let first = bytes.iter().find(|b| !b.is_ascii_whitespace())?;
    // Only objects and arrays: a bare number or string reads better as text.
    if *first != b'{' && *first != b'[' {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    if pretty {
        serde_json::to_string_pretty(&value).ok()
    } else {
        serde_json::to_string(&value).ok()
    }
}

/// Classic hex dump: offset, 16 bytes per line, ASCII gutter.
fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4);
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let _ = write!(out, "{:08x}  ", i * 16);
        for j in 0..16 {
            match chunk.get(j) {
                Some(b) => {
                    let _ = write!(out, "{b:02x} ");
                }
                None => out.push_str("   "),
            }
            if j == 7 {
                out.push(' ');
            }
        }
        out.push(' ');
        out.extend(chunk.iter().map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '.'
            }
        }));
        out.push('\n');
    }
    out.pop();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_detects_json() {
        let out = format_payload(br#"{"a":1}"#.to_vec(), PayloadFormat::Auto, true);
        assert_eq!(out.format, PayloadFormat::Json);
        assert_eq!(out.text, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn auto_falls_back_to_text_then_hex() {
        let text = format_payload(b"hello".to_vec(), PayloadFormat::Auto, true);
        assert_eq!(text.format, PayloadFormat::Text);

        let bin = format_payload(vec![0xff, 0xfe, 0x01], PayloadFormat::Auto, true);
        assert_eq!(bin.format, PayloadFormat::Hex);
        assert!(bin.text.starts_with("00000000  ff fe 01"));
    }

    #[test]
    fn strips_schema_registry_header() {
        let mut bytes = vec![0, 0, 0, 0, 42];
        bytes.extend_from_slice(br#"{"x":true}"#);
        let out = format_payload(bytes, PayloadFormat::Auto, false);
        assert_eq!(out.schema_id, Some(42));
        assert_eq!(out.text, r#"{"x":true}"#);
        assert_eq!(out.size_bytes, 15);
    }
}
