//! WebSocket output module for real-time ADS-B data streaming to web applications
//! 
//! This module provides a WebSocket server that broadcasts BEAST mode messages to web clients.
//! It enables real-time streaming of ADS-B data to web applications with automatic client
//! connection management and message buffering.
//!
//! ## Usage
//! Web clients can connect to the WebSocket server and receive real-time BEAST mode messages:
//! ```javascript
//! const ws = new WebSocket('ws://localhost:8080/adsb');
//! ws.onmessage = function(event) {
//!     // event.data contains binary BEAST mode message
//!     const arrayBuffer = event.data;
//!     // Process BEAST message...
//! };
//! ```
//!
//! ## Message Format
//! Messages are delivered in BEAST binary format, compatible with dump1090:
//! - Message Type: 1 byte (0x31 for short, 0x32 for long frames)
//! - Timestamp: 6 bytes (microseconds since epoch)
//! - Signal Strength: 1 byte (0-255)
//! - Data: Variable length ADS-B message payload

use crate::beast_output::BeastMessage;
use crate::decoder::DecoderMetaData;
use anyhow::Result;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use tracing::{debug, error, info, warn};

/// WebSocket message containing BEAST mode data
#[derive(Debug, Clone)]
pub struct WebSocketMessage {
    pub beast_data: Vec<u8>,
}

impl WebSocketMessage {
    /// Create a WebSocket message from BEAST message
    pub fn from_beast_message(beast_msg: &BeastMessage) -> Self {
        Self {
            beast_data: beast_msg.encode(),
        }
    }

    /// Create a WebSocket message from raw ADS-B packet
    pub fn from_adsb_packet(data: &[u8], metadata: &DecoderMetaData) -> Self {
        let beast_msg = BeastMessage::from_adsb_packet(data, metadata);
        Self::from_beast_message(&beast_msg)
    }
}

/// WebSocket server for streaming ADS-B data
pub struct WebSocketServer {
    listener: TcpListener,
    receiver: broadcast::Receiver<WebSocketMessage>,
}

impl WebSocketServer {
    /// Create a new WebSocket server listening on the specified port
    pub async fn new(port: u16, receiver: broadcast::Receiver<WebSocketMessage>) -> Result<Self> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        info!("WebSocket ADS-B server listening on {}", addr);

        Ok(Self {
            listener,
            receiver,
        })
    }

    /// Run the WebSocket server, accepting connections and streaming data
    pub async fn run(self) -> Result<()> {
        // Accept new WebSocket connections
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    info!("WebSocket client connecting from {}", addr);
                    let message_receiver = self.receiver.resubscribe();

                    tokio::spawn(async move {
                        match Self::handle_websocket_connection(stream, message_receiver).await {
                            Ok(_) => {
                                info!("WebSocket client {} disconnected gracefully", addr);
                            }
                            Err(e) => {
                                debug!("WebSocket client {} disconnected: {}", addr, e);
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept WebSocket connection: {}", e);
                }
            }
        }
    }

    /// Handle a single WebSocket client connection
    async fn handle_websocket_connection(
        stream: TcpStream,
        mut message_receiver: broadcast::Receiver<WebSocketMessage>,
    ) -> Result<()> {
        let ws_stream = accept_async(stream).await?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        info!("WebSocket client connected successfully");

        // Spawn task to handle incoming WebSocket messages (ping/pong, close, etc.)
        let mut ping_task = tokio::spawn(async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Ping(_payload)) => {
                        // Respond to ping with pong - but we can't send from here
                        debug!("Received ping from WebSocket client");
                    }
                    Ok(Message::Close(_)) => {
                        debug!("WebSocket client sent close frame");
                        break;
                    }
                    Err(_) => {
                        debug!("WebSocket client connection error");
                        break;
                    }
                    _ => {
                        // Ignore other message types
                    }
                }
            }
        });

        // Main message sending loop
        loop {
            tokio::select! {
                // Handle broadcast messages
                msg = message_receiver.recv() => {
                    match msg {
                        Ok(message) => {
                            let binary_msg = Message::Binary(message.beast_data);
                            if let Err(e) = ws_sender.send(binary_msg).await {
                                debug!("Failed to send WebSocket message: {}", e);
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!("WebSocket client lagged, skipped {} messages", skipped);
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            debug!("WebSocket message channel closed");
                            break;
                        }
                    }
                }
                // Handle connection monitoring
                _ = &mut ping_task => {
                    debug!("WebSocket client connection monitoring task finished");
                    break;
                }
            }
        }

        Ok(())
    }
}

/// WebSocket message broadcaster
pub struct WebSocketBroadcaster {
    sender: broadcast::Sender<WebSocketMessage>,
}

impl WebSocketBroadcaster {
    /// Create a new WebSocket broadcaster with the specified channel capacity
    pub fn new(capacity: usize) -> (Self, broadcast::Receiver<WebSocketMessage>) {
        let (sender, receiver) = broadcast::channel(capacity);
        (Self { sender }, receiver)
    }

    /// Broadcast an ADS-B packet as a WebSocket message
    pub fn broadcast_packet(&self, data: &[u8], metadata: &DecoderMetaData) -> Result<()> {
        let message = WebSocketMessage::from_adsb_packet(data, metadata);
        match self.sender.send(message) {
            Ok(receiver_count) => {
                debug!("Broadcasted WebSocket message to {} clients", receiver_count);
                Ok(())
            }
            Err(_) => {
                // No receivers, which is fine
                Ok(())
            }
        }
    }

    /// Get the number of active WebSocket clients
    pub fn client_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

/// WebSocket output module implementing the OutputModule trait
pub struct WebSocketOutput {
    name: String,
    port: u16,
    broadcaster: WebSocketBroadcaster,
    is_running: bool,
}

impl WebSocketOutput {
    /// Create a new WebSocket output module
    pub async fn new(config: crate::output_module::OutputModuleConfig) -> Result<Self> {
        let (broadcaster, receiver) = WebSocketBroadcaster::new(config.buffer_capacity);
        
        // Start the WebSocket server
        let server = WebSocketServer::new(config.port, receiver).await?;
        tokio::spawn(async move {
            if let Err(e) = server.run().await {
                error!("WebSocket server error: {}", e);
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
impl crate::output_module::OutputModule for WebSocketOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "WebSocket server for real-time ADS-B data streaming to web applications"
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

/// Builder for WebSocket output modules
pub struct WebSocketOutputBuilder;

impl WebSocketOutputBuilder {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl crate::output_module::OutputModuleBuilder for WebSocketOutputBuilder {
    fn module_type(&self) -> &str {
        "websocket"
    }

    fn description(&self) -> &str {
        "WebSocket server for real-time ADS-B data streaming to web applications"
    }

    fn default_port(&self) -> u16 {
        8080
    }

    async fn build(&self, config: crate::output_module::OutputModuleConfig) -> Result<Box<dyn crate::output_module::OutputModule>> {
        let module = WebSocketOutput::new(config).await?;
        Ok(Box::new(module))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beast_output::BeastMessage;
    use std::time::SystemTime;

    #[test]
    fn test_websocket_message_from_beast() {
        let beast_msg = BeastMessage {
            message_type: crate::beast_output::BeastMessageType::ModeSLong,
            timestamp: 1234567890,
            signal_strength: 200,
            data: vec![0x8D, 0x40, 0x62, 0x1D, 0x58, 0x41, 0x38, 0x80, 0x2C, 0x8F, 0x7E, 0x4D, 0x0C, 0x3C],
        };
        
        let ws_message = WebSocketMessage::from_beast_message(&beast_msg);
        assert!(!ws_message.beast_data.is_empty());
        assert_eq!(ws_message.beast_data[0], 0x1A); // BEAST escape character
        assert_eq!(ws_message.beast_data[1], 0x32); // Mode-S long frame type
    }

    #[test]
    fn test_websocket_message_from_adsb_packet() {
        let data = vec![0x8D, 0x40, 0x62, 0x1D, 0x58, 0x41, 0x38, 0x80, 0x2C, 0x8F, 0x7E, 0x4D, 0x0C, 0x3C];
        let metadata = DecoderMetaData {
            preamble_index: 12345,
            preamble_correlation: 15.5,
            crc_passed: true,
            timestamp: SystemTime::now(),
        };
        
        let ws_message = WebSocketMessage::from_adsb_packet(&data, &metadata);
        assert!(!ws_message.beast_data.is_empty());
        assert_eq!(ws_message.beast_data[0], 0x1A); // BEAST escape character
    }
}