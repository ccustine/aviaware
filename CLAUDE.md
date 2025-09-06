# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AviAware is an ADS-B (Automatic Dependent Surveillance–Broadcast) decoder written in Rust using the FutureSDR framework. It receives radio signals from aircraft transponders at 1090 MHz, demodulates and decodes the ADS-B messages, and displays aircraft positions on a web-based map interface.

## Build and Run Commands

```bash
# Build and run in release mode (recommended for actual use)
cargo run --release

# Build only
cargo build --release

# Development build and run
cargo run

# Run with custom parameters (see listen-adsb binary for all options)
cargo run --release -- --gain 40.0 --preamble-threshold 12.0
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