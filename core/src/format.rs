//! Payload rendering and parsing shared by every frontend, so key/value
//! display and entry behave the same across platforms.

use std::fmt::Write;

use crate::error::KafkaLensError;

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

/// How a payload typed by the user is turned into bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum InputEncoding {
    /// UTF-8 as typed.
    Text,
    /// UTF-8 as typed, after checking it is valid JSON.
    Json,
    /// Hex digits; whitespace and a leading 0x are ignored.
    Hex,
}

/// A payload as text the user can edit, e.g. to duplicate a message.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EditablePayload {
    pub text: String,
    pub encoding: InputEncoding,
}

/// Bytes for a payload typed in `encoding`. `field` names it in errors
/// ("Key", "Value").
#[uniffi::export]
pub fn encode_payload(
    text: String,
    encoding: InputEncoding,
    field: String,
) -> Result<Vec<u8>, KafkaLensError> {
    match encoding {
        InputEncoding::Text => Ok(text.into_bytes()),
        InputEncoding::Json => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(_) => Ok(text.into_bytes()),
            Err(err) => Err(KafkaLensError::InvalidPayload {
                message: format!("{field} is not valid JSON: {err}"),
            }),
        },
        InputEncoding::Hex => parse_hex(&text).ok_or_else(|| KafkaLensError::InvalidPayload {
            message: format!("{field} is not valid hex: use pairs of 0-9 / a-f, spaces allowed."),
        }),
    }
}

/// `bytes` as editable text: pretty JSON when `prefer_json` and it parses
/// (unless Schema Registry framed, which JSON would lose), else UTF-8 text,
/// else hex.
#[uniffi::export]
pub fn editable_payload(bytes: Vec<u8>, prefer_json: bool) -> EditablePayload {
    if prefer_json && split_schema_id(&bytes).0.is_none() {
        if let Some(json) = render_json(&bytes, true) {
            return EditablePayload {
                text: json,
                encoding: InputEncoding::Json,
            };
        }
    }
    match String::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => EditablePayload {
            text,
            encoding: InputEncoding::Text,
        },
        Ok(text) => EditablePayload {
            text: hex_lines(text.as_bytes()),
            encoding: InputEncoding::Hex,
        },
        Err(err) => EditablePayload {
            text: hex_lines(err.as_bytes()),
            encoding: InputEncoding::Hex,
        },
    }
}

fn parse_hex(text: &str) -> Option<Vec<u8>> {
    let digits: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let digits = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
        .unwrap_or(&digits);
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    (0..digits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(digits.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Space-separated hex, 16 bytes per line: what `parse_hex` reads back.
fn hex_lines(bytes: &[u8]) -> String {
    bytes
        .chunks(16)
        .map(|chunk| {
            chunk
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    fn encodes_text_json_and_hex() {
        let enc = |t: &str, e| encode_payload(t.into(), e, "Value".into());
        assert_eq!(enc("hi", InputEncoding::Text).unwrap(), b"hi");
        // JSON is sent exactly as typed, only validated.
        assert_eq!(
            enc("{ \"a\": 1 }", InputEncoding::Json).unwrap(),
            b"{ \"a\": 1 }"
        );
        assert!(matches!(
            enc("{oops", InputEncoding::Json),
            Err(KafkaLensError::InvalidPayload { .. })
        ));
        assert_eq!(
            enc("0xFF 00\n0a", InputEncoding::Hex).unwrap(),
            vec![0xff, 0x00, 0x0a]
        );
        assert!(enc("abc", InputEncoding::Hex).is_err());
        assert!(enc("zz", InputEncoding::Hex).is_err());
    }

    #[test]
    fn editable_payload_round_trips() {
        let json = editable_payload(br#"{"a":1}"#.to_vec(), true);
        assert_eq!(json.encoding, InputEncoding::Json);
        assert_eq!(json.text, "{\n  \"a\": 1\n}");

        assert_eq!(
            editable_payload(b"order-1".to_vec(), true).encoding,
            InputEncoding::Text
        );
        // Keys are not prettified even when they look like JSON.
        assert_eq!(
            editable_payload(br#"{"k":1}"#.to_vec(), false).encoding,
            InputEncoding::Text
        );

        let bytes = vec![0xff, 0x00, 0x10];
        let hex = editable_payload(bytes.clone(), true);
        assert_eq!(hex.encoding, InputEncoding::Hex);
        assert_eq!(
            encode_payload(hex.text, InputEncoding::Hex, "Value".into()).unwrap(),
            bytes
        );

        // Schema Registry framing must survive a duplicate, so it stays hex.
        let mut framed = vec![0, 0, 0, 0, 7];
        framed.extend_from_slice(br#"{"x":1}"#);
        assert_eq!(editable_payload(framed, true).encoding, InputEncoding::Hex);
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
