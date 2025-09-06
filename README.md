# AviAware 🛩️

A high-performance ADS-B (Automatic Dependent Surveillance–Broadcast) decoder written in Rust using the FutureSDR framework. AviAware receives 1090 MHz radio signals from aircraft transponders, demodulates and decodes ADS-B messages in real-time, and displays aircraft positions on an interactive web-based map.

## ✨ Features

- **Real-time ADS-B Decoding**: Processes live 1090 MHz signals from aircraft transponders
- **Interactive Web Map**: Live aircraft tracking with position, heading, and metadata display
- **Multiple SDR Support**: Compatible with RTL-SDR, SoapySDR, and Aaronia devices
- **High-Performance Pipeline**: Built on FutureSDR's efficient signal processing framework
- **Configurable Parameters**: Adjustable gain, thresholds, and aircraft lifetime management
- **File Replay**: Support for analyzing pre-recorded signal files
- **Aircraft Tracking**: Maintains position history and velocity information for each aircraft

## 🚀 Quick Start

### Prerequisites

- **Rust**: Install from [rustup.rs](https://rustup.rs/)
- **SDR Hardware**: RTL-SDR dongle or SoapySDR-compatible device
- **ADS-B Antenna**: 1090 MHz antenna (quarter-wave ~6.9cm or specialized ADS-B antenna)

### Basic Usage

1. **Build and run** (release mode recommended for performance):
   ```bash
   cargo run --release
   ```

2. **Open the web interface**: Navigate to http://127.0.0.1:1337/ in your browser

3. **View aircraft**: Real-time aircraft positions will appear on the map as they're detected

### Command Line Options

```bash
cargo run --release -- [OPTIONS]

Options:
      --antenna <ANTENNA>           Antenna selection for SDR device
  -a, --args <ARGS>                 Additional arguments for SDR device
  -g, --gain <GAIN>                 RF gain in dB [default: 30]
  -s, --sample-rate <SAMPLE_RATE>   Sample rate in Hz [default: 2200000]
  -p, --preamble-threshold <PREAMBLE_THRESHOLD>  
                                    Preamble detection threshold [default: 10]
  -f, --file <FILE>                 Use recorded file instead of live SDR
  -l, --lifetime <LIFETIME>         Remove aircraft after N seconds of inactivity
  -h, --help                        Print help information
  -V, --version                     Print version information
```

## 📡 Hardware Setup

### SDR Device Configuration

**RTL-SDR (Default):**
```bash
# Use RTL-SDR with custom gain
cargo run --release -- --gain 40.0

# Specify RTL-SDR device
cargo run --release -- --args "driver=rtlsdr,serial=12345"
```

**SoapySDR:**
```bash
# Use with SoapySDR device
cargo run --release -- --args "driver=soapysdr"
```

**Aaronia Devices:**
```bash
# Build with Aaronia support
cargo build --release --features aaronia_http
```

### Antenna Recommendations

- **Quarter-wave monopole**: ~6.9 cm vertical wire
- **Commercial ADS-B antenna**: Optimized for 1090 MHz
- **FlightAware Pro Stick Plus**: Popular RTL-SDR with built-in filter

## 🔧 Configuration

### Runtime Configuration (`config.toml`)

```toml
log_level = "info"                # Logging verbosity (error, warn, info, debug, trace)
ctrlport_enable = true           # Enable web server
ctrlport_bind = "127.0.0.1:1337" # Web server bind address
frontend_path = "dist"           # Path to web interface files
```

### Performance Tuning

**For optimal performance:**
- Use sample rates that are divisors of 4 MHz (e.g., 2 MHz, 2.4 MHz)
- Adjust gain to minimize noise while maintaining sensitivity
- Consider using `--lifetime` to manage memory usage for long-running sessions

**Example optimized command:**
```bash
cargo run --release -- --gain 35.0 --sample-rate 2000000 --preamble-threshold 12.0 --lifetime 300
```

## 🗺️ Web Interface

The web interface provides:

- **Real-time Map**: OpenStreetMap-based display with aircraft positions
- **Aircraft Icons**: Rotated plane icons showing heading direction
- **Auto-zoom**: Automatically adjusts view to show all aircraft
- **Position History**: Trail showing recent aircraft movements
- **Aircraft Details**: ICAO codes, callsigns, altitude, and velocity information

### Interface Features

- **Manual zoom control**: Disables auto-zoom when user interacts with map
- **Responsive design**: Works on desktop and mobile browsers
- **Real-time updates**: 1-second refresh interval for live tracking

## 📊 Architecture

### Signal Processing Pipeline

```
SDR Device → Resampling → Magnitude → Noise Floor Estimation
                                  ↓
Web Interface ← Tracker ← Decoder ← Demodulator ← Preamble Detector
```

### Core Components

- **`PreambleDetector`**: Correlates incoming samples with ADS-B preamble patterns
- **`Demodulator`**: Extracts digital bits from detected ADS-B frames
- **`Decoder`**: Parses ADS-B messages using the `adsb_deku` library
- **`Tracker`**: Maintains aircraft state, position tracking, and CPR frame processing

### Data Flow

1. **Signal Acquisition**: SDR device captures 1090 MHz RF signals
2. **Preprocessing**: Resampling to 4 MHz, magnitude calculation, noise estimation
3. **Detection**: Preamble correlation and threshold-based frame detection
4. **Demodulation**: PPM (Pulse Position Modulation) symbol extraction
5. **Decoding**: ADS-B message parsing and validation
6. **Tracking**: Aircraft state management and position calculation
7. **Visualization**: Real-time web interface updates

## 🧪 Development & Testing

### File Replay Mode

Test with recorded signals:
```bash
# Record signals (using external tools like rtl_sdr)
rtl_sdr -f 1090000000 -s 2000000 samples.cf32

# Replay for testing
cargo run --release -- --file samples.cf32
```

### Building Features

```bash
# Default build (SoapySDR support)
cargo build --release

# RTL-SDR support
cargo build --release --features rtlsdr

# Aaronia device support  
cargo build --release --features aaronia_http
```

### Development Build

```bash
# Debug build (slower but with debug info)
cargo run

# Check code quality
cargo clippy
cargo test
```

## 🛠️ Dependencies

### Core Libraries

- **[FutureSDR](https://github.com/FutureSDR/FutureSDR)**: High-performance signal processing framework
- **[adsb_deku](https://github.com/rsadsb/adsb_deku)**: ADS-B message parsing and validation
- **[seify](https://github.com/FutureSDR/seify)**: SDR hardware abstraction layer

### Hardware Support

- **RTL-SDR**: Popular low-cost USB SDR dongles
- **SoapySDR**: Universal SDR hardware interface
- **Aaronia**: Professional spectrum analyzer integration

## 🔍 Troubleshooting

### Common Issues

**No aircraft detected:**
- Check antenna connection and positioning
- Verify 1090 MHz reception in your area
- Adjust gain settings (try 20-50 dB range)
- Ensure ADS-B traffic exists nearby (use online trackers to verify)

**Poor performance:**
- Use release build (`--release` flag)
- Choose optimal sample rate (divisor of 4 MHz)
- Reduce preamble threshold for weaker signals
- Check SDR device driver installation

**Web interface not loading:**
- Verify server is running on http://127.0.0.1:1337/
- Check firewall settings
- Ensure `dist/` directory contains web files

### Performance Optimization

**High CPU usage:**
- Use lower sample rates
- Increase preamble threshold
- Enable aircraft lifetime pruning
- Consider hardware with better SDR support

## 📜 License

This project uses various open-source libraries. Check individual dependency licenses for details.

## 🤝 Contributing

Contributions welcome! Please feel free to submit issues, feature requests, or pull requests.

## 🔗 Related Projects

- **[dump1090](https://github.com/flightaware/dump1090)**: Popular ADS-B decoder
- **[tar1090](https://github.com/wiedehopf/tar1090)**: Web interface for dump1090
- **[OpenSky Network](https://opensky-network.org/)**: Crowdsourced ADS-B data

---

*Built with ❤️ in Rust for the aviation community*