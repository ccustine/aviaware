//! AVR format output for dump1090 compatibility
//! 
//! This module implements the AVR text protocol used by dump1090 to stream
//! ADS-B messages over TCP port 30002. AVR format provides human-readable
//! output with timestamps and signal levels for debugging and integration.

use crate::decoder::DecoderMetaData;
use anyhow::Result;
use std::time::UNIX_EPOCH;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// An AVR format message containing ADS-B data
#[derive(Debug, Clone)]
pub struct AvrMessage {
    pub timestamp: u64,
    pub signal_level: i32,
    pub data: Vec<u8>,
}

impl AvrMessage {
    /// Create a new AVR message from ADS-B packet bytes
    pub fn from_adsb_packet(data: &[u8], metadata: &DecoderMetaData) -> Self {
        // Convert SystemTime to seconds since epoch
        let timestamp = metadata
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Convert correlation strength to signal level (dBFS approximation)
        // Map 0-50 correlation to -50 to 0 dBFS range
        let signal_level = ((metadata.preamble_correlation.min(50.0).max(0.0) / 50.0) * 50.0 - 50.0) as i32;

        Self {
            timestamp,
            signal_level,
            data: data.to_vec(),
        }
    }

    /// Encode the message in AVR text format
    /// Format: "*{hex_data};\n" with optional @{timestamp} {signal_level}\n prefix
    pub fn encode(&self) -> String {
        let hex_data = self.data
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<String>();
        
        // AVR format with timestamp and signal level
        format!("@{:010X}\n*{};\n", self.timestamp, hex_data)
    }

    /// Encode in simple format (just hex data)
    pub fn encode_simple(&self) -> String {
        let hex_data = self.data
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<String>();
        
        format!("*{};\n", hex_data)
    }
}

/// AVR format TCP server
pub struct AvrServer {
    listener: TcpListener,
    receiver: broadcast::Receiver<AvrMessage>,
}

impl AvrServer {
    /// Create a new AVR server listening on the specified port
    pub async fn new(port: u16, receiver: broadcast::Receiver<AvrMessage>) -> Result<Self> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        info!("AVR format server listening on {}", addr);

        Ok(Self { listener, receiver })
    }

    /// Run the AVR server, accepting connections and streaming data
    pub async fn run(self) -> Result<()> {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    info!("AVR client connected from {}", addr);
                    let mut receiver = self.receiver.resubscribe();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, &mut receiver).await {
                            debug!("AVR client {} disconnected: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept AVR connection: {}", e);
                }
            }
        }
    }

    /// Handle a single AVR client connection
    async fn handle_client(
        mut stream: TcpStream,
        receiver: &mut broadcast::Receiver<AvrMessage>,
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
                    warn!("AVR client lagged, skipped {} messages", skipped);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    debug!("AVR message channel closed");
                    return Ok(());
                }
            }
        }
    }
}

/// AVR format message broadcaster
pub struct AvrBroadcaster {
    sender: broadcast::Sender<AvrMessage>,
}

impl AvrBroadcaster {
    /// Create a new AVR broadcaster with the specified channel capacity
    pub fn new(capacity: usize) -> (Self, broadcast::Receiver<AvrMessage>) {
        let (sender, receiver) = broadcast::channel(capacity);
        (Self { sender }, receiver)
    }

    /// Broadcast an ADS-B packet as an AVR message
    pub fn broadcast_packet(&self, data: &[u8], metadata: &DecoderMetaData) -> Result<()> {
        let message = AvrMessage::from_adsb_packet(data, metadata);
        
        match self.sender.send(message) {
            Ok(receiver_count) => {
                debug!("Broadcasted AVR message to {} clients", receiver_count);
                Ok(())
            }
            Err(_) => {
                // No receivers, which is fine
                Ok(())
            }
        }
    }

    /// Get the number of active AVR clients
    pub fn client_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

/// AVR output module implementing the OutputModule trait
pub struct AvrOutput {
    name: String,
    port: u16,
    broadcaster: AvrBroadcaster,
    is_running: bool,
}

impl AvrOutput {
    /// Create a new AVR output module
    pub async fn new(config: crate::output_module::OutputModuleConfig) -> Result<Self> {
        let (broadcaster, receiver) = AvrBroadcaster::new(config.buffer_capacity);
        
        // Start the server
        let server = AvrServer::new(config.port, receiver).await?;
        tokio::spawn(async move {
            if let Err(e) = server.run().await {
                error!("AVR server error: {}", e);
            }
        });

        Ok(Self {
            name: config.name,
            port: config.port,
            broadcaster,
            is_running: true,
        })
    }
}

#[async_trait::async_trait]
impl crate::output_module::OutputModule for AvrOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "AVR text format with timestamps for dump1090 compatibility (port 30003)"
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn broadcast_packet(&self, data: &[u8], metadata: &DecoderMetaData) -> Result<()> {
        self.broadcaster.broadcast_packet(data, metadata)
    }

    fn client_count(&self) -> usize {
        self.broadcaster.client_count()
    }

    fn is_running(&self) -> bool {
        self.is_running
    }

    fn stop(&mut self) -> Result<()> {
        self.is_running = false;
        Ok(())
    }
}

/// Builder for AVR output modules
pub struct AvrOutputBuilder;

impl AvrOutputBuilder {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl crate::output_module::OutputModuleBuilder for AvrOutputBuilder {
    fn module_type(&self) -> &str {
        "avr"
    }

    fn description(&self) -> &str {
        "AVR text format with timestamps for dump1090 compatibility"
    }

    fn default_port(&self) -> u16 {
        30003
    }

    async fn build(&self, config: crate::output_module::OutputModuleConfig) -> Result<Box<dyn crate::output_module::OutputModule>> {
        let module = AvrOutput::new(config).await?;
        Ok(Box::new(module))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_avr_message_encoding() {
        let data = vec![0x8D, 0x40, 0x62, 0x1D, 0x58, 0x41, 0x38, 0x80, 0x2C, 0x8F, 0x7E, 0x4D, 0x0C, 0x3C];
        let metadata = DecoderMetaData {
            preamble_index: 12345,
            preamble_correlation: 15.5,
            crc_passed: true,
            timestamp: SystemTime::now(),
        };
        
        let message = AvrMessage::from_adsb_packet(&data, &metadata);
        let encoded = message.encode();
        
        // Should contain timestamp prefix, hex data, and proper format
        assert!(encoded.starts_with('@'));
        assert!(encoded.contains("*8D40621D5841382C8F7E4D0C3C;"));
        assert!(encoded.ends_with('\n'));
    }

    #[test]
    fn test_avr_simple_encoding() {
        let data = vec![0x8D, 0x40, 0x62, 0x1D];
        let metadata = DecoderMetaData {
            preamble_index: 12345,
            preamble_correlation: 25.0,
            crc_passed: true,
            timestamp: SystemTime::now(),
        };
        
        let message = AvrMessage::from_adsb_packet(&data, &metadata);
        let encoded = message.encode_simple();
        
        assert_eq!(encoded, "*8D40621D;\n");
    }

    #[test]
    fn test_signal_level_conversion() {
        let data = vec![0x8D];
        let metadata = DecoderMetaData {
            preamble_index: 0,
            preamble_correlation: 25.0, // Should map to -25 dBFS
            crc_passed: true,
            timestamp: SystemTime::now(),
        };
        
        let message = AvrMessage::from_adsb_packet(&data, &metadata);
        assert_eq!(message.signal_level, -25);
    }
}