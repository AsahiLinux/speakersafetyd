// SPDX-License-Identifier: MIT
// (C) 2022 The Asahi Linux Contributors
/*!
    Handles speaker safety on Apple Silicon machines. This code is designed to
    fail safe. The speaker should not be enabled until this daemon has successfully
    initialised. If at any time we run into an unrecoverable error (we shouldn't),
    we gracefully bail and use an IOCTL to shut off the speakers.
*/
use std::collections::BTreeMap;
use std::fs::read_to_string;
use std::io;
use std::path::PathBuf;
use std::{thread::sleep, time};

use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use configparser::ini::Ini;
use log;
use log::{debug, error, info, trace, warn};
use simple_logger::SimpleLogger;

mod helpers;
mod types;

static VERSION: &str = "0.0.1";

const DEFAULT_CONFIG_PATH: &str = "share/speakersafetyd";

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Options {
    /// Path to the configuration file base directory
    #[arg(short, long)]
    config_path: Option<PathBuf>,

    /// Increase the log level
    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,
}

fn get_machine() -> String {
    read_to_string("/proc/device-tree/compatible")
        .expect("Could not read device tree compatible")
        .strip_prefix("apple,")
        .expect("Unexpected compatible format")
        .split_once("\0")
        .expect("Unexpected compatible format")
        .0
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .to_string()
}

fn get_speakers(config: &Ini) -> Vec<String> {
    config
        .sections()
        .iter()
        .filter_map(|a| a.strip_prefix("Speaker/"))
        .map(|a| a.to_string())
        .collect()
}

struct SpeakerGroup {
    speakers: Vec<types::Speaker>,
    gain: f32,
}

impl Default for SpeakerGroup {
    fn default() -> Self {
        Self {
            speakers: Default::default(),
            gain: f32::NAN,
        }
    }
}

fn main() {
    let args = Options::parse();

    SimpleLogger::new()
        .with_level(args.verbose.log_level_filter())
        .without_timestamps()
        .init()
        .unwrap();
    info!("Starting up");

    let mut config_path = args.config_path.unwrap_or_else(|| {
        let mut path = PathBuf::new();
        path.push(option_env!("PREFIX").unwrap_or("/usr/local"));
        path.push(DEFAULT_CONFIG_PATH);
        path
    });
    info!("Config base: {:?}", config_path);

    let model: String = get_machine();
    info!("Model: {}", model);

    config_path.push(&model);
    config_path.set_extension("conf");
    info!("Config file: {:?}", config_path);

    let device = format!("hw:{}", model.to_ascii_uppercase());
    info!("Device: {}", device);

    let mut cfg: Ini = Ini::new_cs();
    cfg.load(config_path).expect("Failed to read config file");

    let globals = types::Globals::parse(&cfg);

    let speaker_names = get_speakers(&cfg);
    let speaker_count = speaker_names.len();
    info!("Found {} speakers", speaker_count);

    info!("Opening control device");
    let ctl: alsa::ctl::Ctl = helpers::open_card(&device);

    let mut groups: BTreeMap<usize, SpeakerGroup> = BTreeMap::new();

    for i in speaker_names {
        let speaker: types::Speaker = types::Speaker::new(&globals, &i, &cfg, &ctl);

        groups
            .entry(speaker.group)
            .or_default()
            .speakers
            .push(speaker);
    }

    assert!(
        groups
            .values()
            .map(|a| a.speakers.len())
            .fold(0, |a, b| a + b)
            == speaker_count
    );
    assert!(2 * speaker_count <= globals.channels);

    let pcm_name = format!("{},{}", device, globals.visense_pcm);
    // Set up PCM to buffer in V/ISENSE
    let pcm: alsa::pcm::PCM = helpers::open_pcm(&pcm_name, globals.channels.try_into().unwrap(), 0);
    let mut buf = Vec::new();
    buf.resize(globals.period * globals.channels, 0i16);

    let io = pcm.io_i16().unwrap();

    let hwp = pcm.hw_params_current().unwrap();
    let sample_rate = hwp.get_rate().unwrap();

    info!("Sample rate: {}", sample_rate);

    loop {
        // Block while we're reading into the buffer
        io.readi(&mut buf).unwrap();

        for (idx, group) in groups.iter_mut() {
            let gain = group
                .speakers
                .iter_mut()
                .map(|s| s.run_model(&buf, sample_rate as f32))
                .reduce(f32::min)
                .unwrap();
            if gain != group.gain {
                if group.gain == 0. {
                    warn!("Speaker group {} gain limited to {}", idx, gain);
                }
                group.speakers.iter_mut().for_each(|s| s.update(&ctl, gain));
                group.gain = gain;
            }
        }
    }
}
