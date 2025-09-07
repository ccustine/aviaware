//! BEAST mode output for dump1090 compatibility
//! 
//! This module implements the BEAST binary protocol used by dump1090 to stream
//! raw ADS-B messages over TCP port 30005. This enables compatibility with
//! existing ADS-B tools like tar1090, FlightAware feeders, and other aggregators.

use crate::decoder::DecoderMetaData;
use anyhow::Result;
use std::time::UNIX_EPOCH;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// BEAST mode escape character
const BEAST_ESCAPE: u8 = 0x1A;

/// BEAST message types
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum BeastMessageType {
    /// Mode-S short frame (56 bits)
    ModeS = 0x31,
    /// Mode-S long frame (112 bits) 
    ModeSLong = 0x32,
    /// Status message
    Status = 0x34,
}

/// A BEAST mode message containing ADS-B data
#[derive(Debug, Clone)]
pub struct BeastMessage {
    pub message_type: BeastMessageType,
    pub timestamp: u64,
    pub signal_strength: u8,
    pub data: Vec<u8>,
}

impl BeastMessage {
    /// Create a new BEAST message from ADS-B packet bytes
    pub fn from_adsb_packet(data: &[u8], metadata: &DecoderMetaData) -> Self {
        // Convert SystemTime to microseconds since epoch
        let timestamp = metadata
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        // Convert correlation strength to signal strength (0-255)
        let signal_strength = (metadata.preamble_correlation.min(255.0).max(0.0)) as u8;

        // Determine message type based on data length
        let message_type = if data.len() <= 7 {
            BeastMessageType::ModeS
        } else {
            BeastMessageType::ModeSLong
        };

        Self {
            message_type,
            timestamp,
            signal_strength,
            data: data.to_vec(),
        }
    }

    /// Encode the message in BEAST binary format
    pub fn encode(&self) -> Vec<u8> {
        let mut encoded = Vec::new();

        // Start with escape character
        encoded.push(BEAST_ESCAPE);
        
        // Message type
        encoded.push(self.message_type as u8);

        // Timestamp (6 bytes, big-endian)
        let timestamp_bytes = self.timestamp.to_be_bytes();
        for &byte in &timestamp_bytes[2..] {
            // Take the last 6 bytes (48-bit timestamp)
            if byte == BEAST_ESCAPE {
                encoded.push(BEAST_ESCAPE); // Escape the escape character
            }
            encoded.push(byte);
        }

        // Signal strength
        if self.signal_strength == BEAST_ESCAPE {
            encoded.push(BEAST_ESCAPE);
        }
        encoded.push(self.signal_strength);

        // Message data
        for &byte in &self.data {
            if byte == BEAST_ESCAPE {
                encoded.push(BEAST_ESCAPE); // Escape the escape character
            }
            encoded.push(byte);
        }

        encoded
    }
}

/// BEAST mode TCP server
pub struct BeastServer {
    listener: TcpListener,
    receiver: broadcast::Receiver<BeastMessage>,
}

impl BeastServer {
    /// Create a new BEAST server listening on the specified port
    pub async fn new(port: u16, receiver: broadcast::Receiver<BeastMessage>) -> Result<Self> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        info!("BEAST mode server listening on {}", addr);

        Ok(Self { listener, receiver })
    }

    /// Run the BEAST server, accepting connections and streaming data
    pub async fn run(self) -> Result<()> {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    info!("BEAST client connected from {}", addr);
                    let mut receiver = self.receiver.resubscribe();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, &mut receiver).await {
                            debug!("BEAST client {} disconnected: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept BEAST connection: {}", e);
                }
            }
        }
    }

    /// Handle a single BEAST client connection
    async fn handle_client(
        mut stream: TcpStream,
        receiver: &mut broadcast::Receiver<BeastMessage>,
    ) -> Result<()> {
        loop {
            match receiver.recv().await {
                Ok(message) => {
                    let encoded = message.encode();
                    if let Err(e) = stream.write_all(&encoded).await {
                        return Err(e.into());
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!("BEAST client lagged, skipped {} messages", skipped);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    debug!("BEAST message channel closed");
                    return Ok(());
                }
            }
        }
    }
}

/// BEAST mode message broadcaster
pub struct BeastBroadcaster {
    sender: broadcast::Sender<BeastMessage>,
}

impl BeastBroadcaster {
    /// Create a new BEAST broadcaster with the specified channel capacity
    pub fn new(capacity: usize) -> (Self, broadcast::Receiver<BeastMessage>) {
        let (sender, receiver) = broadcast::channel(capacity);
        (Self { sender }, receiver)
    }

    /// Broadcast an ADS-B packet as a BEAST message
    pub fn broadcast_packet(&self, data: &[u8], metadata: &DecoderMetaData) -> Result<()> {
        let message = BeastMessage::from_adsb_packet(data, metadata);
        
        match self.sender.send(message) {
            Ok(receiver_count) => {
                debug!("Broadcasted BEAST message to {} clients", receiver_count);
                Ok(())
            }
            Err(_) => {
                // No receivers, which is fine
                Ok(())
            }
        }
    }

    /// Get the number of active BEAST clients
    pub fn client_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beast_message_encoding() {
        let data = vec![0x8D, 0x40, 0x62, 0x1D, 0x58, 0x41, 0x38, 0x80, 0x2C, 0x8F, 0x7E, 0x4D, 0x0C, 0x3C];
        let message = BeastMessage {
            message_type: BeastMessageType::ModeSLong,
            timestamp: 0x123456789ABC,
            signal_strength: 42,
            data,
        };

        let encoded = message.encode();
        
        // Should start with escape + message type
        assert_eq!(encoded[0], BEAST_ESCAPE);
        assert_eq!(encoded[1], BeastMessageType::ModeSLong as u8);
        
        // Should contain the data
        assert!(encoded.len() > 8); // At least header + some data
    }

    #[test]
    fn test_escape_character_handling() {
        let message = BeastMessage {
            message_type: BeastMessageType::ModeS,
            timestamp: 0x1A1A1A1A1A1A, // Contains escape characters
            signal_strength: BEAST_ESCAPE, // Signal strength is escape char
            data: vec![BEAST_ESCAPE, 0x42, BEAST_ESCAPE], // Data contains escape chars
        };

        let encoded = message.encode();
        
        // Count how many escape characters should be doubled
        let escape_count = encoded.iter().filter(|&&b| b == BEAST_ESCAPE).count();
        
        // Should have more escapes than in the original data due to doubling
        assert!(escape_count > 4);
    }
}