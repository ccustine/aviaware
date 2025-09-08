use adsb_demod::DEMOD_SAMPLE_RATE;
use adsb_demod::{OutputModuleConfig, OutputModuleManager};
use adsb_demod::{BeastOutput, AvrOutput, RawOutput};
use adsb_demod::Decoder;
use adsb_demod::Demodulator;
use adsb_demod::PreambleDetector;
use adsb_demod::Tracker;
use anyhow::Result;
use clap::Parser;
use clap::command;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::Throttle;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::num_complex::Complex32;
use futuresdr::num_integer;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::tracing::warn;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Antenna
    #[arg(long)]
    antenna: Option<String>,
    /// Seify Args
    #[arg(short, long)]
    args: Option<String>,
    /// Gain
    #[arg(short, long, default_value_t = 30.0)]
    gain: f64,
    /// Sample rate
    #[arg(short, long, default_value_t = 2.2e6, value_parser = sample_rate_parser)]
    sample_rate: f64,
    /// Preamble detection threshold
    #[arg(short, long, default_value_t = 10.0)]
    preamble_threshold: f32,
    /// Use a file instead of a device
    #[arg(short, long)]
    file: Option<String>,
    /// Remove aircrafts when no packets have been received for the specified number of seconds
    #[arg(short, long)]
    lifetime: Option<u64>,
    /// Enable BEAST mode output (dump1090 compatible)
    #[arg(long, default_value_t = true)]
    beast: bool,
    /// Disable BEAST mode output
    #[arg(long, conflicts_with = "beast")]
    no_beast: bool,
    /// Port for BEAST mode output
    #[arg(long, default_value_t = 30005)]
    beast_port: u16,
    /// Enable AVR format output (dump1090 compatible with timestamps)
    #[arg(long)]
    avr: bool,
    /// Port for AVR format output
    #[arg(long, default_value_t = 30003)]
    avr_port: u16,
    /// Enable raw format output (dump1090 port 30002 compatible)
    #[arg(long, default_value_t = true)]
    raw: bool,
    /// Disable raw format output
    #[arg(long, conflicts_with = "raw")]
    no_raw: bool,
    /// Port for raw format output
    #[arg(long, default_value_t = 30002)]
    raw_port: u16,
}

fn sample_rate_parser(sample_rate_str: &str) -> Result<f64, String> {
    let sample_rate: f64 = sample_rate_str
        .parse()
        .map_err(|_| format!("`{sample_rate_str}` is not a valid sample rate"))?;
    // Sample rate must be at least 2 MHz
    if sample_rate < 2e6 {
        Err("Sample rate must be at least 2 MHz".to_string())
    } else {
        Ok(sample_rate)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut fg = Flowgraph::new();
    futuresdr::runtime::init();

    let src = match args.file {
        Some(f) => {
            let file_src_block = fg.add_block(FileSource::<Complex32>::new(f, false))?;
            let throttle_block = fg.add_block(Throttle::<Complex32>::new(args.sample_rate))?;
            fg.connect_stream(file_src_block, "out", throttle_block, "in")?;
            throttle_block
        }
        None => {
            // Load seify source
            let src = SourceBuilder::new()
                .frequency(1090e6)
                .sample_rate(args.sample_rate)
                .gain(args.gain)
                .antenna(args.antenna)
                .args(args.args)?
                .build()?;

            fg.add_block(src)?
        }
    };

    // Change sample rate to our demodulator sample rate.
    // Using a sample rate higher than the signal bandwidth allows
    // us to use a simple symbol synchronization mechanism and have
    // more clear symbol transitions.
    let gcd = num_integer::gcd(args.sample_rate as usize, DEMOD_SAMPLE_RATE);
    let interp = DEMOD_SAMPLE_RATE / gcd;
    let decim = args.sample_rate as usize / gcd;
    if interp > 100 || decim > 100 {
        warn!(
            "Warning: Interpolation/decimation factor is large. \
             Use a sampling frequency that is a divisor of {DEMOD_SAMPLE_RATE} for the best performance."
        );
    }
    let interp_block = fg.add_block(FirBuilder::resampling::<Complex32, Complex32>(
        interp, decim,
    ))?;
    fg.connect_stream(src, "out", interp_block, "in")?;

    let complex_to_mag_2 = fg.add_block(Apply::new(|i: &Complex32| i.norm_sqr()))?;
    fg.connect_stream(interp_block, "out", complex_to_mag_2, "in")?;

    let nf_est_block = fg.add_block(FirBuilder::new::<f32, f32, _>(vec![1.0f32 / 32.0; 32]))?;
    fg.connect_stream(complex_to_mag_2, "out", nf_est_block, "in")?;

    let preamble_taps: Vec<f32> = PreambleDetector::preamble_correlator_taps();
    let preamble_corr_block = fg.add_block(FirBuilder::new::<f32, f32, _>(preamble_taps))?;
    fg.connect_stream(complex_to_mag_2, "out", preamble_corr_block, "in")?;

    let preamble_detector = fg.add_block(PreambleDetector::new(args.preamble_threshold))?;
    fg.connect_stream(complex_to_mag_2, "out", preamble_detector, "in_samples")?;
    fg.connect_stream(nf_est_block, "out", preamble_detector, "in_nf")?;
    fg.connect_stream(
        preamble_corr_block,
        "out",
        preamble_detector,
        "in_preamble_corr",
    )?;

    let adsb_demod = fg.add_block(Demodulator::new())?;
    fg.connect_stream(preamble_detector, "out", adsb_demod, "in")?;

    let adsb_decoder = fg.add_block(Decoder::new(false))?;
    fg.connect_message(adsb_demod, "out", adsb_decoder, "in")?;

    // Set up dynamic output module system
    let mut output_manager = OutputModuleManager::new();

    // Start enabled output modules
    if args.beast && !args.no_beast {
        let config = OutputModuleConfig::new("beast", args.beast_port).with_buffer_capacity(1024);
        match BeastOutput::new(config).await {
            Ok(module) => {
                println!("BEAST mode server started on port {}", args.beast_port);
                output_manager.add_module(Box::new(module));
            }
            Err(e) => {
                eprintln!("Failed to start BEAST server: {}", e);
            }
        }
    }

    if args.avr {
        let config = OutputModuleConfig::new("avr", args.avr_port).with_buffer_capacity(1024);
        match AvrOutput::new(config).await {
            Ok(module) => {
                println!("AVR format server started on port {}", args.avr_port);
                output_manager.add_module(Box::new(module));
            }
            Err(e) => {
                eprintln!("Failed to start AVR server: {}", e);
            }
        }
    }

    if args.raw && !args.no_raw {
        let config = OutputModuleConfig::new("raw", args.raw_port).with_buffer_capacity(1024);
        match RawOutput::new(config).await {
            Ok(module) => {
                println!("Raw format server started on port {}", args.raw_port);
                output_manager.add_module(Box::new(module));
            }
            Err(e) => {
                eprintln!("Failed to start raw server: {}", e);
            }
        }
    }

    // Create tracker with dynamic output module system
    let prune_after = args.lifetime.map(Duration::from_secs);
    let tracker = Tracker::new_with_modules(prune_after, output_manager);
    
    let adsb_tracker = fg.add_block(tracker)?;
    fg.connect_message(adsb_decoder, "out", adsb_tracker, "in")?;

    println!("Please open the map in the browser: http://127.0.0.1:1337/");
    Runtime::new().run(fg)?;

    Ok(())
}
