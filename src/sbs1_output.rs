//! SBS-1/BaseStation format output for port 30004 compatibility
//! 
//! This module implements the SBS-1 (BaseStation) CSV format protocol originally 
//! developed by Kinetic Avionics for their SBS-1 receiver. This format is widely
//! used by aircraft tracking applications and provides structured data output.
//!
//! ## Format Specification
//! 
//! The SBS-1 format consists of comma-separated values with the following structure:
//! MSG,{transmission_type},{session_id},{aircraft_id},{hex_ident},{flight_id},{date_generated},{time_generated},{date_logged},{time_logged},{callsign},{altitude},{ground_speed},{track},{lat},{lon},{vertical_rate},{squawk},{alert},{emergency},{spi},{is_on_ground}
//! 
//! ## References
//! - BaseStation Protocol Reference: http://woodair.net/sbs/article/barebones42_socket_data.htm
//! - SBS-1 Data Format: http://www.homepages.mcb.net/bones/SBS/Article/Barebones42_Socket_Data.htm
//! - Aviation Data Exchange Protocol: RTCA DO-260B (ADS-B standards)
//! 
//! ## Message Types
//! - MSG,1: Aircraft identification
//! - MSG,3: Airborne position  
//! - MSG,4: Airborne velocity
//! - MSG,5: Surveillance altitude
//! - MSG,6: Squawk change
//! - MSG,7: Air-to-air altitude
//! - MSG,8: All-call reply

use crate::decoder::DecoderMetaData;
use anyhow::Result;
use std::time::UNIX_EPOCH;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// An SBS-1/BaseStation format message containing ADS-B data
#[derive(Debug, Clone)]
pub struct Sbs1Message {
    pub message_type: u8,
    pub transmission_type: u8,
    pub session_id: u32,
    pub aircraft_id: u32,
    pub hex_ident: String,
    pub flight_id: u32,
    pub date_generated: String,
    pub time_generated: String,
    pub date_logged: String,
    pub time_logged: String,
    pub callsign: Option<String>,
    pub altitude: Option<i32>,
    pub ground_speed: Option<f64>,
    pub track: Option<f64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub vertical_rate: Option<i16>,
    pub squawk: Option<u16>,
    pub alert: bool,
    pub emergency: bool,
    pub spi: bool,
    pub is_on_ground: bool,
}

impl Sbs1Message {
    /// Create a new SBS-1 message from ADS-B packet bytes
    pub fn from_adsb_packet(data: &[u8], metadata: &DecoderMetaData) -> Option<Self> {
        // Parse the ADS-B message to extract relevant data
        if data.len() < 14 {
            return None;
        }

        // Extract ICAO address (bytes 1-3)
        let icao = format!("{:02X}{:02X}{:02X}", data[1], data[2], data[3]);
        
        // Get timestamp components
        let timestamp = metadata.timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        
        let datetime = chrono::DateTime::from_timestamp(timestamp.as_secs() as i64, 0)
            .unwrap_or_default();
        
        let date_str = datetime.format("%Y/%m/%d").to_string();
        let time_str = datetime.format("%H:%M:%S.%3f").to_string();

        // Determine message type based on ADS-B data
        if data.len() >= 5 {
            let type_code = (data[4] >> 3) & 0x1F;
            
            match type_code {
                1..=4 => {
                    // Aircraft identification message (MSG,1)
                    Some(Self {
                        message_type: 1,
                        transmission_type: 1,
                        session_id: 1,
                        aircraft_id: 1,
                        hex_ident: icao,
                        flight_id: 1,
                        date_generated: date_str.clone(),
                        time_generated: time_str.clone(),
                        date_logged: date_str,
                        time_logged: time_str,
                        callsign: None, // Will be extracted by caller if available
                        altitude: None,
                        ground_speed: None,
                        track: None,
                        latitude: None,
                        longitude: None,
                        vertical_rate: None,
                        squawk: None,
                        alert: false,
                        emergency: false,
                        spi: false,
                        is_on_ground: false,
                    })
                }
                9..=18 => {
                    // Airborne position message (MSG,3)
                    Some(Self {
                        message_type: 3,
                        transmission_type: 3,
                        session_id: 1,
                        aircraft_id: 1,
                        hex_ident: icao,
                        flight_id: 1,
                        date_generated: date_str.clone(),
                        time_generated: time_str.clone(),
                        date_logged: date_str,
                        time_logged: time_str,
                        callsign: None,
                        altitude: None, // Will be extracted by caller if available
                        ground_speed: None,
                        track: None,
                        latitude: None, // Will be extracted by caller if available
                        longitude: None, // Will be extracted by caller if available
                        vertical_rate: None,
                        squawk: None,
                        alert: false,
                        emergency: false,
                        spi: false,
                        is_on_ground: type_code >= 5 && type_code <= 8,
                    })
                }
                19 => {
                    // Airborne velocity message (MSG,4)
                    Some(Self {
                        message_type: 4,
                        transmission_type: 4,
                        session_id: 1,
                        aircraft_id: 1,
                        hex_ident: icao,
                        flight_id: 1,
                        date_generated: date_str.clone(),
                        time_generated: time_str.clone(),
                        date_logged: date_str,
                        time_logged: time_str,
                        callsign: None,
                        altitude: None,
                        ground_speed: None, // Will be extracted by caller if available
                        track: None, // Will be extracted by caller if available
                        latitude: None,
                        longitude: None,
                        vertical_rate: None, // Will be extracted by caller if available
                        squawk: None,
                        alert: false,
                        emergency: false,
                        spi: false,
                        is_on_ground: false,
                    })
                }
                _ => {
                    // Generic message for other types
                    Some(Self {
                        message_type: 5,
                        transmission_type: 5,
                        session_id: 1,
                        aircraft_id: 1,
                        hex_ident: icao,
                        flight_id: 1,
                        date_generated: date_str.clone(),
                        time_generated: time_str.clone(),
                        date_logged: date_str,
                        time_logged: time_str,
                        callsign: None,
                        altitude: None,
                        ground_speed: None,
                        track: None,
                        latitude: None,
                        longitude: None,
                        vertical_rate: None,
                        squawk: None,
                        alert: false,
                        emergency: false,
                        spi: false,
                        is_on_ground: false,
                    })
                }
            }
        } else {
            None
        }
    }

    /// Set aircraft identification data
    pub fn with_identification(mut self, callsign: Option<String>) -> Self {
        self.callsign = callsign;
        self
    }

    /// Set position data
    pub fn with_position(mut self, altitude: Option<i32>, latitude: Option<f64>, longitude: Option<f64>) -> Self {
        self.altitude = altitude;
        self.latitude = latitude;
        self.longitude = longitude;
        self
    }

    /// Set velocity data
    pub fn with_velocity(mut self, ground_speed: Option<f64>, track: Option<f64>, vertical_rate: Option<i16>) -> Self {
        self.ground_speed = ground_speed;
        self.track = track;
        self.vertical_rate = vertical_rate;
        self
    }

    /// Set squawk data
    pub fn with_squawk(mut self, squawk: Option<u16>) -> Self {
        self.squawk = squawk;
        self
    }

    /// Encode the message in SBS-1 CSV format
    /// Format: MSG,{transmission_type},{session_id},{aircraft_id},{hex_ident},{flight_id},{date_generated},{time_generated},{date_logged},{time_logged},{callsign},{altitude},{ground_speed},{track},{lat},{lon},{vertical_rate},{squawk},{alert},{emergency},{spi},{is_on_ground}
    pub fn encode(&self) -> String {
        format!(
            "MSG,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\r\n",
            self.transmission_type,
            self.session_id,
            self.aircraft_id,
            self.hex_ident,
            self.flight_id,
            self.date_generated,
            self.time_generated,
            self.date_logged,
            self.time_logged,
            self.callsign.as_deref().unwrap_or(""),
            self.altitude.map_or(String::new(), |a| a.to_string()),
            self.ground_speed.map_or(String::new(), |gs| format!("{:.1}", gs)),
            self.track.map_or(String::new(), |t| format!("{:.1}", t)),
            self.latitude.map_or(String::new(), |lat| format!("{:.6}", lat)),
            self.longitude.map_or(String::new(), |lon| format!("{:.6}", lon)),
            self.vertical_rate.map_or(String::new(), |vr| vr.to_string()),
            self.squawk.map_or(String::new(), |sq| format!("{:04}", sq)),
            if self.alert { "1" } else { "0" },
            if self.emergency { "1" } else { "0" },
            if self.spi { "1" } else { "0" },
            if self.is_on_ground { "1" } else { "0" }
        )
    }
}

/// SBS-1 format TCP server
pub struct Sbs1Server {
    listener: TcpListener,
    receiver: broadcast::Receiver<Sbs1Message>,
}

impl Sbs1Server {
    /// Create a new SBS-1 server listening on the specified port
    pub async fn new(port: u16, receiver: broadcast::Receiver<Sbs1Message>) -> Result<Self> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        info!("SBS-1 BaseStation server listening on {}", addr);

        Ok(Self { listener, receiver })
    }

    /// Run the SBS-1 server, accepting connections and streaming data
    pub async fn run(self) -> Result<()> {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    info!("SBS-1 client connected from {}", addr);
                    let mut receiver = self.receiver.resubscribe();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, &mut receiver).await {
                            debug!("SBS-1 client {} disconnected: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept SBS-1 connection: {}", e);
                }
            }
        }
    }

    /// Handle a single SBS-1 client connection
    async fn handle_client(
        mut stream: TcpStream,
        receiver: &mut broadcast::Receiver<Sbs1Message>,
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
                    warn!("SBS-1 client lagged, skipped {} messages", skipped);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    debug!("SBS-1 message channel closed");
                    return Ok(());
                }
            }
        }
    }
}

/// SBS-1 format message broadcaster
pub struct Sbs1Broadcaster {
    sender: broadcast::Sender<Sbs1Message>,
}

impl Sbs1Broadcaster {
    /// Create a new SBS-1 broadcaster with the specified channel capacity
    pub fn new(capacity: usize) -> (Self, broadcast::Receiver<Sbs1Message>) {
        let (sender, receiver) = broadcast::channel(capacity);
        (Self { sender }, receiver)
    }

    /// Broadcast an ADS-B packet as an SBS-1 message
    pub fn broadcast_packet(&self, data: &[u8], metadata: &DecoderMetaData) -> Result<()> {
        if let Some(message) = Sbs1Message::from_adsb_packet(data, metadata) {
            match self.sender.send(message) {
                Ok(receiver_count) => {
                    debug!("Broadcasted SBS-1 message to {} clients", receiver_count);
                    Ok(())
                }
                Err(_) => {
                    // No receivers, which is fine
                    Ok(())
                }
            }
        } else {
            // Unable to parse message, skip
            Ok(())
        }
    }

    /// Get the number of active SBS-1 clients
    pub fn client_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

/// SBS-1 output module implementing the OutputModule trait
pub struct Sbs1Output {
    name: String,
    port: u16,
    broadcaster: Sbs1Broadcaster,
    is_running: bool,
}

impl Sbs1Output {
    /// Create a new SBS-1 output module
    pub async fn new(config: crate::output_module::OutputModuleConfig) -> Result<Self> {
        let (broadcaster, receiver) = Sbs1Broadcaster::new(config.buffer_capacity);
        
        // Start the server
        let server = Sbs1Server::new(config.port, receiver).await?;
        tokio::spawn(async move {
            if let Err(e) = server.run().await {
                error!("SBS-1 server error: {}", e);
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
impl crate::output_module::OutputModule for Sbs1Output {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "SBS-1/BaseStation CSV format for port 30004 compatibility"
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

/// Builder for SBS-1 output modules
pub struct Sbs1OutputBuilder;

impl Sbs1OutputBuilder {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl crate::output_module::OutputModuleBuilder for Sbs1OutputBuilder {
    fn module_type(&self) -> &str {
        "sbs1"
    }

    fn description(&self) -> &str {
        "SBS-1/BaseStation CSV format for port 30004 compatibility"
    }

    fn default_port(&self) -> u16 {
        30004
    }

    async fn build(&self, config: crate::output_module::OutputModuleConfig) -> Result<Box<dyn crate::output_module::OutputModule>> {
        let module = Sbs1Output::new(config).await?;
        Ok(Box::new(module))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_sbs1_message_encoding() {
        let message = Sbs1Message {
            message_type: 3,
            transmission_type: 3,
            session_id: 1,
            aircraft_id: 1,
            hex_ident: "ABC123".to_string(),
            flight_id: 1,
            date_generated: "2024/01/01".to_string(),
            time_generated: "12:00:00.000".to_string(),
            date_logged: "2024/01/01".to_string(),
            time_logged: "12:00:00.000".to_string(),
            callsign: Some("TEST123".to_string()),
            altitude: Some(35000),
            ground_speed: Some(450.5),
            track: Some(270.0),
            latitude: Some(40.123456),
            longitude: Some(-74.654321),
            vertical_rate: Some(-800),
            squawk: Some(1200),
            alert: false,
            emergency: false,
            spi: false,
            is_on_ground: false,
        };
        
        let encoded = message.encode();
        assert!(encoded.starts_with("MSG,3,1,1,ABC123,1,2024/01/01,12:00:00.000,2024/01/01,12:00:00.000,TEST123,35000,450.5,270.0,40.123456,-74.654321,-800,1200,0,0,0,0"));
        assert!(encoded.ends_with("\r\n"));
    }

    #[test]
    fn test_sbs1_message_from_adsb_packet() {
        let data = vec![0x8D, 0x40, 0x62, 0x1D, 0x58, 0x41, 0x38, 0x80, 0x2C, 0x8F, 0x7E, 0x4D, 0x0C, 0x3C];
        let metadata = DecoderMetaData {
            preamble_index: 12345,
            preamble_correlation: 15.5,
            crc_passed: true,
            timestamp: SystemTime::now(),
        };
        
        let message = Sbs1Message::from_adsb_packet(&data, &metadata);
        assert!(message.is_some());
        
        let msg = message.unwrap();
        assert_eq!(msg.hex_ident, "40621D");
        assert_eq!(msg.transmission_type, 3); // Airborne position
    }

    #[test]
    fn test_sbs1_identification_message() {
        let data = vec![0x8D, 0x40, 0x62, 0x1D, 0x20, 0x41, 0x38, 0x80, 0x2C, 0x8F, 0x7E, 0x4D, 0x0C, 0x3C];
        let metadata = DecoderMetaData {
            preamble_index: 12345,
            preamble_correlation: 15.5,
            crc_passed: true,
            timestamp: SystemTime::now(),
        };
        
        let message = Sbs1Message::from_adsb_packet(&data, &metadata);
        assert!(message.is_some());
        
        let msg = message.unwrap();
        assert_eq!(msg.transmission_type, 1); // Aircraft identification
    }
}