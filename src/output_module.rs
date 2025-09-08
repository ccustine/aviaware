//! Dynamic output module system for ADS-B data streaming
//! 
//! This module provides a trait-based architecture for output modules that allows
//! for dynamic registration and management of different output formats (BEAST, AVR, Raw, etc.)
//! without requiring code changes when adding new modules.

use crate::decoder::DecoderMetaData;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Configuration for an output module
#[derive(Debug, Clone)]
pub struct OutputModuleConfig {
    /// The name/identifier of the module
    pub name: String,
    /// The port to bind the server to
    pub port: u16,
    /// The buffer capacity for the broadcast channel
    pub buffer_capacity: usize,
    /// Whether this module is enabled
    pub enabled: bool,
    /// Additional module-specific configuration
    pub extra: HashMap<String, String>,
}

impl OutputModuleConfig {
    pub fn new(name: impl Into<String>, port: u16) -> Self {
        Self {
            name: name.into(),
            port,
            buffer_capacity: 1024,
            enabled: true,
            extra: HashMap::new(),
        }
    }

    pub fn with_buffer_capacity(mut self, capacity: usize) -> Self {
        self.buffer_capacity = capacity;
        self
    }

    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

/// Trait for output modules that can receive and broadcast ADS-B data
#[async_trait]
pub trait OutputModule: Send + Sync {
    /// Get the name/identifier of this output module
    fn name(&self) -> &str;

    /// Get a description of what this module does
    fn description(&self) -> &str;

    /// Get the port this module is listening on
    fn port(&self) -> u16;

    /// Broadcast an ADS-B packet to all connected clients
    fn broadcast_packet(&self, data: &[u8], metadata: &DecoderMetaData) -> Result<()>;

    /// Get the number of currently connected clients
    fn client_count(&self) -> usize;

    /// Check if the module is currently running
    fn is_running(&self) -> bool;

    /// Stop the module and close all connections
    fn stop(&mut self) -> Result<()>;
}

/// Trait for creating output modules dynamically
#[async_trait]
pub trait OutputModuleBuilder: Send + Sync {
    /// Get the name of the module type this builder creates
    fn module_type(&self) -> &str;

    /// Get a description of the module type
    fn description(&self) -> &str;

    /// Get the default port for this module type
    fn default_port(&self) -> u16;

    /// Create a new instance of the output module
    async fn build(&self, config: OutputModuleConfig) -> Result<Box<dyn OutputModule>>;

    /// Check if this builder supports the given format name
    fn supports_format(&self, format: &str) -> bool {
        format.eq_ignore_ascii_case(self.module_type())
    }
}

/// Registry for managing available output module builders
pub struct OutputModuleRegistry {
    builders: HashMap<String, Box<dyn OutputModuleBuilder>>,
}

impl OutputModuleRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            builders: HashMap::new(),
        }
    }

    /// Register a new output module builder
    pub fn register<B: OutputModuleBuilder + 'static>(&mut self, builder: B) {
        let module_type = builder.module_type().to_lowercase();
        self.builders.insert(module_type, Box::new(builder));
    }

    /// Get all available module types
    pub fn available_types(&self) -> Vec<&str> {
        self.builders.keys().map(|s| s.as_str()).collect()
    }

    /// Get a builder for the specified module type
    pub fn get_builder(&self, module_type: &str) -> Option<&dyn OutputModuleBuilder> {
        self.builders.get(&module_type.to_lowercase()).map(|b| b.as_ref())
    }

    /// Create a module instance from configuration
    pub async fn create_module(&self, config: OutputModuleConfig) -> Result<Box<dyn OutputModule>> {
        let module_type = config.name.to_lowercase();
        match self.get_builder(&module_type) {
            Some(builder) => builder.build(config).await,
            None => Err(anyhow::anyhow!("Unknown output module type: {}", config.name)),
        }
    }

    /// Get default configuration for a module type
    pub fn default_config(&self, module_type: &str) -> Option<OutputModuleConfig> {
        self.get_builder(module_type).map(|builder| {
            OutputModuleConfig::new(module_type, builder.default_port())
        })
    }

    /// Create a registry with all built-in module types registered
    pub fn with_builtin_modules() -> Self {
        let registry = Self::new();
        
        // Note: Built-in module registration will be handled by the application
        // when the builders are available. This is left as a placeholder.
        
        registry
    }
}

impl Default for OutputModuleRegistry {
    fn default() -> Self {
        Self::with_builtin_modules()
    }
}

/// Manager for active output modules
pub struct OutputModuleManager {
    modules: Vec<Box<dyn OutputModule>>,
}

impl OutputModuleManager {
    /// Create a new module manager
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
        }
    }

    /// Add a module to the manager
    pub fn add_module(&mut self, module: Box<dyn OutputModule>) {
        self.modules.push(module);
    }

    /// Broadcast a packet to all active modules
    pub fn broadcast_to_all(&self, data: &[u8], metadata: &DecoderMetaData) {
        for module in &self.modules {
            if let Err(e) = module.broadcast_packet(data, metadata) {
                tracing::warn!("Failed to broadcast to module '{}': {}", module.name(), e);
            }
        }
    }

    /// Get the total number of connected clients across all modules
    pub fn total_client_count(&self) -> usize {
        self.modules.iter().map(|m| m.client_count()).sum()
    }

    /// Get a list of all active modules with their client counts
    pub fn module_status(&self) -> Vec<(String, u16, usize, bool)> {
        self.modules
            .iter()
            .map(|m| (m.name().to_string(), m.port(), m.client_count(), m.is_running()))
            .collect()
    }

    /// Stop all modules
    pub fn stop_all(&mut self) -> Result<()> {
        for module in &mut self.modules {
            if let Err(e) = module.stop() {
                tracing::warn!("Failed to stop module '{}': {}", module.name(), e);
            }
        }
        self.modules.clear();
        Ok(())
    }

    /// Get the number of active modules
    pub fn module_count(&self) -> usize {
        self.modules.len()
    }
}

impl Default for OutputModuleManager {
    fn default() -> Self {
        Self::new()
    }
}