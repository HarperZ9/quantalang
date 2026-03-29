// ===============================================================================
// QUANTALANG LSP TRANSPORT
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Transport layer for LSP communication over stdio.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

// =============================================================================
// LSP TRANSPORT
// =============================================================================

/// LSP transport error.
#[derive(Debug)]
pub enum TransportError {
    /// I/O error.
    Io(io::Error),
    /// Invalid header.
    InvalidHeader(String),
    /// Missing content length.
    MissingContentLength,
    /// Parse error.
    ParseError(String),
    /// Channel disconnected.
    Disconnected,
}

impl From<io::Error> for TransportError {
    fn from(err: io::Error) -> Self {
        TransportError::Io(err)
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "I/O error: {}", e),
            TransportError::InvalidHeader(h) => write!(f, "Invalid header: {}", h),
            TransportError::MissingContentLength => write!(f, "Missing Content-Length header"),
            TransportError::ParseError(e) => write!(f, "Parse error: {}", e),
            TransportError::Disconnected => write!(f, "Channel disconnected"),
        }
    }
}

impl std::error::Error for TransportError {}

/// Result type for transport operations.
pub type TransportResult<T> = Result<T, TransportError>;

// =============================================================================
// RAW MESSAGE
// =============================================================================

/// A raw LSP message (unparsed JSON).
#[derive(Debug, Clone)]
pub struct RawMessage {
    /// The content type (usually "application/vscode-jsonrpc").
    pub content_type: Option<String>,
    /// The JSON content.
    pub content: String,
}

impl RawMessage {
    /// Create a new raw message.
    pub fn new(content: String) -> Self {
        Self {
            content_type: None,
            content,
        }
    }

    /// Encode to LSP wire format.
    pub fn encode(&self) -> Vec<u8> {
        let content_bytes = self.content.as_bytes();
        let mut output = Vec::new();

        // Header
        output.extend_from_slice(format!("Content-Length: {}\r\n", content_bytes.len()).as_bytes());
        if let Some(ref ct) = self.content_type {
            output.extend_from_slice(format!("Content-Type: {}\r\n", ct).as_bytes());
        }
        output.extend_from_slice(b"\r\n");

        // Body
        output.extend_from_slice(content_bytes);

        output
    }
}

// =============================================================================
// STDIO TRANSPORT
// =============================================================================

/// Standard I/O transport for LSP.
pub struct StdioTransport {
    /// Incoming message receiver.
    incoming_rx: Receiver<RawMessage>,
    /// Outgoing message sender.
    outgoing_tx: Sender<RawMessage>,
    /// Shutdown flag.
    shutdown_tx: Sender<()>,
}

impl StdioTransport {
    /// Create a new stdio transport.
    pub fn new() -> Self {
        let (incoming_tx, incoming_rx) = channel::<RawMessage>();
        let (outgoing_tx, outgoing_rx) = channel::<RawMessage>();
        let (shutdown_tx, shutdown_rx) = channel::<()>();

        // Reader thread
        thread::spawn(move || {
            let stdin = io::stdin();
            let mut reader = BufReader::new(stdin.lock());

            loop {
                // Check for shutdown
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }

                match read_message(&mut reader) {
                    Ok(msg) => {
                        if incoming_tx.send(msg).is_err() {
                            break;
                        }
                    }
                    Err(TransportError::Io(ref e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        break;
                    }
                    Err(_) => {
                        // Continue on parse errors
                    }
                }
            }
        });

        // Writer thread
        thread::spawn(move || {
            let stdout = io::stdout();
            let mut writer = stdout.lock();

            while let Ok(msg) = outgoing_rx.recv() {
                let encoded = msg.encode();
                if writer.write_all(&encoded).is_err() {
                    break;
                }
                if writer.flush().is_err() {
                    break;
                }
            }
        });

        Self {
            incoming_rx,
            outgoing_tx,
            shutdown_tx,
        }
    }

    /// Receive an incoming message (blocking).
    pub fn recv(&self) -> TransportResult<RawMessage> {
        self.incoming_rx
            .recv()
            .map_err(|_| TransportError::Disconnected)
    }

    /// Try to receive an incoming message (non-blocking).
    pub fn try_recv(&self) -> TransportResult<Option<RawMessage>> {
        match self.incoming_rx.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(TransportError::Disconnected),
        }
    }

    /// Send an outgoing message.
    pub fn send(&self, msg: RawMessage) -> TransportResult<()> {
        self.outgoing_tx
            .send(msg)
            .map_err(|_| TransportError::Disconnected)
    }

    /// Shutdown the transport.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a single LSP message from a reader.
fn read_message<R: BufRead>(reader: &mut R) -> TransportResult<RawMessage> {
    let mut content_length: Option<usize> = None;
    let mut content_type: Option<String> = None;

    // Read headers
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;

        // Empty line signals end of headers
        if line == "\r\n" || line == "\n" {
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            break;
        }

        // Parse header
        if let Some(pos) = line.find(':') {
            let (name, value) = line.split_at(pos);
            let value = value[1..].trim();

            match name.to_lowercase().as_str() {
                "content-length" => {
                    content_length = value.parse().ok();
                }
                "content-type" => {
                    content_type = Some(value.to_string());
                }
                _ => {
                    // Ignore unknown headers
                }
            }
        }
    }

    // Read content
    let length = content_length.ok_or(TransportError::MissingContentLength)?;
    let mut content = vec![0u8; length];
    reader.read_exact(&mut content)?;

    let content_str = String::from_utf8(content)
        .map_err(|e| TransportError::ParseError(format!("Invalid UTF-8: {}", e)))?;

    Ok(RawMessage {
        content_type,
        content: content_str,
    })
}

// =============================================================================
// MESSAGE READER/WRITER
// =============================================================================

/// Message reader from any byte source.
pub struct MessageReader<R> {
    reader: BufReader<R>,
}

impl<R: Read> MessageReader<R> {
    /// Create a new message reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
        }
    }

    /// Read the next message.
    pub fn read(&mut self) -> TransportResult<RawMessage> {
        read_message(&mut self.reader)
    }
}

/// Message writer to any byte sink.
pub struct MessageWriter<W> {
    writer: W,
}

impl<W: Write> MessageWriter<W> {
    /// Create a new message writer.
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Write a message.
    pub fn write(&mut self, msg: &RawMessage) -> TransportResult<()> {
        let encoded = msg.encode();
        self.writer.write_all(&encoded)?;
        self.writer.flush()?;
        Ok(())
    }
}

// =============================================================================
// JSON SERIALIZATION HELPERS
// =============================================================================

/// Simple JSON builder for constructing responses.
#[derive(Debug, Clone, Default)]
pub struct JsonBuilder {
    parts: Vec<String>,
}

impl JsonBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn object() -> JsonObjectBuilder {
        JsonObjectBuilder::new()
    }

    pub fn array() -> JsonArrayBuilder {
        JsonArrayBuilder::new()
    }

    pub fn null() -> String {
        "null".to_string()
    }

    pub fn bool(v: bool) -> String {
        if v {
            "true".to_string()
        } else {
            "false".to_string()
        }
    }

    pub fn number<N: std::fmt::Display>(n: N) -> String {
        n.to_string()
    }

    pub fn string(s: &str) -> String {
        let escaped = s
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        format!("\"{}\"", escaped)
    }
}

/// JSON object builder.
#[derive(Debug, Clone, Default)]
pub struct JsonObjectBuilder {
    fields: Vec<(String, String)>,
}

impl JsonObjectBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field(mut self, name: &str, value: String) -> Self {
        self.fields.push((name.to_string(), value));
        self
    }

    pub fn field_if_some<T, F>(self, name: &str, opt: Option<T>, f: F) -> Self
    where
        F: FnOnce(T) -> String,
    {
        if let Some(v) = opt {
            self.field(name, f(v))
        } else {
            self
        }
    }

    pub fn field_str(self, name: &str, value: &str) -> Self {
        self.field(name, JsonBuilder::string(value))
    }

    pub fn field_str_if_some(self, name: &str, opt: Option<&str>) -> Self {
        self.field_if_some(name, opt, |s| JsonBuilder::string(s))
    }

    pub fn field_bool(self, name: &str, value: bool) -> Self {
        self.field(name, JsonBuilder::bool(value))
    }

    pub fn field_number<N: std::fmt::Display>(self, name: &str, value: N) -> Self {
        self.field(name, JsonBuilder::number(value))
    }

    pub fn build(self) -> String {
        let fields: Vec<String> = self
            .fields
            .into_iter()
            .map(|(k, v)| format!("\"{}\":{}", k, v))
            .collect();
        format!("{{{}}}", fields.join(","))
    }
}

/// JSON array builder.
#[derive(Debug, Clone, Default)]
pub struct JsonArrayBuilder {
    items: Vec<String>,
}

impl JsonArrayBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn item(mut self, value: String) -> Self {
        self.items.push(value);
        self
    }

    pub fn items<I: IntoIterator<Item = String>>(mut self, values: I) -> Self {
        self.items.extend(values);
        self
    }

    pub fn build(self) -> String {
        format!("[{}]", self.items.join(","))
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_message_encode() {
        let msg = RawMessage::new(r#"{"jsonrpc":"2.0","id":1}"#.to_string());
        let encoded = msg.encode();
        let expected = b"Content-Length: 24\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1}";
        assert_eq!(encoded, expected);
    }

    #[test]
    fn test_json_object_builder() {
        let json = JsonObjectBuilder::new()
            .field_str("method", "initialize")
            .field_number("id", 1)
            .build();
        assert_eq!(json, r#"{"method":"initialize","id":1}"#);
    }

    #[test]
    fn test_json_array_builder() {
        let json = JsonArrayBuilder::new()
            .item(JsonBuilder::number(1))
            .item(JsonBuilder::number(2))
            .item(JsonBuilder::number(3))
            .build();
        assert_eq!(json, "[1,2,3]");
    }

    #[test]
    fn test_json_string_escaping() {
        let json = JsonBuilder::string("hello\nworld\\test\"quote");
        assert_eq!(json, r#""hello\nworld\\test\"quote""#);
    }

    #[test]
    fn test_read_message() {
        let input = b"Content-Length: 13\r\n\r\n{\"test\":true}";
        let mut reader = std::io::BufReader::new(&input[..]);
        let msg = read_message(&mut reader).unwrap();
        assert_eq!(msg.content, r#"{"test":true}"#);
    }
}
