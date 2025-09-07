//! Raw format output for dump1090 compatibility
//! 
//! This module implements the raw text protocol used by dump1090's port 30002.
//! This format provides simple hex message output without timestamps or metadata,
//! exactly matching dump1090's raw output format: *{hexdata};\n

use crate::decoder::DecoderMetaData;
use anyhow::Result;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// A raw format message containing ADS-B data
#[derive(Debug, Clone)]
pub struct RawMessage {
    pub data: Vec<u8>,
}

impl RawMessage {
    /// Create a new raw message from ADS-B packet bytes
    pub fn from_adsb_packet(data: &[u8], _metadata: &DecoderMetaData) -> Self {
        Self {
            data: data.to_vec(),
        }
    }

    /// Encode the message in raw format
    /// Format: "*{hex_data};\n" - exactly like dump1090 port 30002
    pub fn encode(&self) -> String {
        let hex_data = self.data
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<String>();
        
        // Raw format: just hex data with * prefix and ; suffix
        format!("*{};\n", hex_data)
    }
}

/// Raw format TCP server
pub struct RawServer {
    listener: TcpListener,
    receiver: broadcast::Receiver<RawMessage>,
}

impl RawServer {
    /// Create a new raw server listening on the specified port
    pub async fn new(port: u16, receiver: broadcast::Receiver<RawMessage>) -> Result<Self> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        info!("Raw format server listening on {}", addr);

        Ok(Self { listener, receiver })
    }

    /// Run the raw server, accepting connections and streaming data
    pub async fn run(self) -> Result<()> {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    info!("Raw client connected from {}", addr);
                    let mut receiver = self.receiver.resubscribe();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, &mut receiver).await {
                            debug!("Raw client {} disconnected: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept raw connection: {}", e);
                }
            }
        }
    }

    /// Handle a single raw client connection
    async fn handle_client(
        mut stream: TcpStream,
        receiver: &mut broadcast::Receiver<RawMessage>,
    ) -> Result<()> {
        loop {
            match receiver.recv().await {
                Ok(message) => {
                    let encoded = message.encode();
                    if let Err(e) = stream.write_all(encoded.as_bytes()).await {
                        return Err(e.into());
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!("Raw client lagged, skipped {} messages", skipped);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    debug!("Raw message channel closed");
                    return Ok(());
                }
            }
        }
    }
}

/// Raw format message broadcaster
pub struct RawBroadcaster {
    sender: broadcast::Sender<RawMessage>,
}

impl RawBroadcaster {
    /// Create a new raw broadcaster with the specified channel capacity
    pub fn new(capacity: usize) -> (Self, broadcast::Receiver<RawMessage>) {
        let (sender, receiver) = broadcast::channel(capacity);
        (Self { sender }, receiver)
    }

    /// Broadcast an ADS-B packet as a raw message
    pub fn broadcast_packet(&self, data: &[u8], metadata: &DecoderMetaData) -> Result<()> {
        let message = RawMessage::from_adsb_packet(data, metadata);
        
        match self.sender.send(message) {
            Ok(receiver_count) => {
                debug!("Broadcasted raw message to {} clients", receiver_count);
                Ok(())
            }
            Err(_) => {
                // No receivers, which is fine
                Ok(())
            }
        }
    }

    /// Get the number of active raw clients
    pub fn client_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_raw_message_encoding() {
        let data = vec![0x8D, 0x40, 0x62, 0x1D, 0x58, 0x41, 0x38, 0x80, 0x2C, 0x8F, 0x7E, 0x4D, 0x0C, 0x3C];
        let metadata = DecoderMetaData {
            preamble_index: 12345,
            preamble_correlation: 15.5,
            crc_passed: true,
            timestamp: SystemTime::now(),
        };
        
        let message = RawMessage::from_adsb_packet(&data, &metadata);
        let encoded = message.encode();
        
        // Should match dump1090 raw format exactly
        assert_eq!(encoded, "*8D40621D5841382C8F7E4D0C3C;\n");
    }

    #[test]
    fn test_raw_format_simple() {
        let data = vec![0x8D, 0x45, 0x1E, 0x8B];
        let metadata = DecoderMetaData {
            preamble_index: 0,
            preamble_correlation: 25.0,
            crc_passed: true,
            timestamp: SystemTime::now(),
        };
        
        let message = RawMessage::from_adsb_packet(&data, &metadata);
        let encoded = message.encode();
        
        assert_eq!(encoded, "*8D451E8B;\n");
    }

    #[test]
    fn test_raw_message_no_metadata_dependency() {
        // Raw format should not depend on metadata content
        let data = vec![0xAB, 0xCD];
        let metadata = DecoderMetaData {
            preamble_index: 999999,
            preamble_correlation: 0.0,
            crc_passed: false,
            timestamp: SystemTime::now(),
        };
        
        let message = RawMessage::from_adsb_packet(&data, &metadata);
        let encoded = message.encode();
        
        // Only the data matters, not the metadata
        assert_eq!(encoded, "*ABCD;\n");
    }
}