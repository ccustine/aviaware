# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AviAware is an ADS-B (Automatic Dependent Surveillance–Broadcast) decoder written in Rust using the FutureSDR framework. It receives radio signals from aircraft transponders at 1090 MHz, demodulates and decodes the ADS-B messages, and displays aircraft positions on a web-based map interface.

## Build and Run Commands

```bash
# Development build and run (recommended for actual use)
cargo run

# Build only
cargo build

# Build and run in release mode
cargo run --release

# Run with custom parameters (see listen-adsb binary for all options)
cargo run -- --gain 40.0 --preamble-threshold 12.0
```

## Architecture

### Core Components

- **Preamble Detector** (`src/preamble_detector.rs`): Detects ADS-B message preambles in the signal
- **Demodulator** (`src/demodulator.rs`): Demodulates detected frames into bit streams  
- **Decoder** (`src/decoder.rs`): Decodes bit streams into ADS-B packets using the `adsb_deku` library
- **Tracker** (`src/tracker.rs`): Maintains aircraft state and position tracking with optional aircraft lifetime management

### Signal Processing Flow

The application uses FutureSDR's flowgraph architecture:
1. **Source**: SDR device (via seify) or file input
2. **Resampling**: Converts input sample rate to 4 MHz demodulator rate
3. **Signal Processing**: Magnitude calculation, noise floor estimation, preamble correlation
4. **Detection**: Preamble detection based on correlation and threshold
5. **Demodulation**: Symbol extraction and packet demodulation
6. **Decoding**: ADS-B message parsing
7. **Tracking**: Aircraft state management and web serving

### Web Interface

- **Frontend**: Located in `dist/` directory with HTML, JavaScript, and Leaflet map
- **Server**: Embedded HTTP server serves the map interface on `127.0.0.1:1337`
- **Configuration**: Server settings in `config.toml`

## Configuration

Edit `config.toml` to modify:
- `log_level`: Logging verbosity
- `ctrlport_bind`: Web server bind address and port  
- `frontend_path`: Path to web interface files

## Dependencies

- **FutureSDR**: Signal processing framework (version 0.0.38 from crates.io)
- **adsb_deku**: ADS-B message parsing
- **Seify**: SDR hardware abstraction layer
- **Clap**: Command-line argument parsing

## Hardware Support

Supports SDR devices through FutureSDR's seify backend including:
- RTL-SDR dongles (with `rtlsdr` feature)
- SoapySDR-compatible devices (default `soapy` feature)
- Aaronia devices (with `aaronia_http` feature)

## Running from File

For testing/replay:
```bash
cargo run --release -- --file samples.cf32
```

Input files should be Complex32 format at any sample rate ≥ 2 MHz.

## Output Formats

AviAware supports multiple output formats for compatibility with various ADS-B tools and applications:

### BEAST Format (Port 30005)
- **Default**: Enabled by default
- **Description**: Binary format compatible with dump1090's BEAST mode
- **Usage**: `--beast` / `--no-beast`, `--beast-port <PORT>`
- **Protocol**: Binary messages with timestamps and signal levels
- **Compatibility**: dump1090, tar1090, FlightRadar24 feeders

### Raw Format (Port 30002)
- **Default**: Enabled by default
- **Description**: Simple hex format compatible with dump1090's raw output
- **Usage**: `--raw` / `--no-raw`, `--raw-port <PORT>`
- **Protocol**: `*{hexdata};\n` format
- **Compatibility**: dump1090 port 30002, simple monitoring tools

### AVR Format (Port 30003)
- **Default**: Disabled by default
- **Description**: Text format with timestamps and signal levels
- **Usage**: `--avr`, `--avr-port <PORT>`
- **Protocol**: `@{timestamp}\n*{hexdata};\n` format
- **Compatibility**: dump1090 AVR format, debugging tools

### SBS-1/BaseStation Format (Port 30004)
- **Default**: Disabled by default
- **Description**: CSV format compatible with BaseStation and SBS-1 receivers
- **Usage**: `--sbs1`, `--sbs1-port <PORT>`
- **Protocol**: Comma-separated values with aircraft data
- **Compatibility**: BaseStation, Virtual Radar Server, PlanePlotter
- **References**: 
  - [BaseStation Protocol](http://woodair.net/sbs/article/barebones42_socket_data.htm)
  - [SBS-1 Data Format](http://www.homepages.mcb.net/bones/SBS/Article/Barebones42_Socket_Data.htm)

## Example Usage

```bash
# Run with defaults (BEAST + Raw)
cargo run

# Enable all output formats
cargo run -- --avr --sbs1

# Custom ports to avoid conflicts
cargo run -- --beast-port 40005 --raw-port 40002 --sbs1-port 40004

# Enable only SBS-1 output
cargo run -- --no-beast --no-raw --sbs1
```