// SPDX-License-Identifier: MIT
// (C) 2022 The Asahi Linux Contributors
/*!
    Handles speaker safety on Apple Silicon machines. This code is designed to
    fail safe. The speaker should not be enabled until this daemon has successfully
    initialised. If at any time we run into an unrecoverable error (we shouldn't),
    we gracefully bail and use an IOCTL to shut off the speakers.
*/
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use configparser::ini::Ini;
use log;
use log::{debug, info, warn};
use simple_logger::SimpleLogger;

mod helpers;
mod types;

const DEFAULT_CONFIG_PATH: &str = "share/speakersafetyd";

const UNLOCK_MAGIC: i32 = 0xdec1be15u32 as i32;

const FLAGFILE: &str = "/run/speakersafetyd.flag";

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
    fs::read_to_string("/proc/device-tree/compatible")
        .expect("Could not read device tree compatible")
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

    let machine: String = get_machine();
    info!("Machine: {}", machine);

    let (maker, model) = machine
        .split_once(",")
        .expect("Unexpected machine name format");

    config_path.push(&maker);
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

    let flag_path = Path::new(FLAGFILE);

    let cold_boot = match flag_path.try_exists() {
        Ok(true) => {
            info!("Startup mode: Warm boot");
            false
        }
        Ok(false) => {
            info!("Startup mode: Cold boot");
            if fs::write(flag_path, b"started").is_err() {
                warn!("Failed to write flag file, continuing as warm boot");
                false
            } else {
                true
            }
        }
        Err(_) => {
            warn!("Failed to test flag file, continuing as warm boot");
            false
        }
    };

    let mut groups: BTreeMap<usize, SpeakerGroup> = BTreeMap::new();

    for i in speaker_names {
        let speaker: types::Speaker = types::Speaker::new(&globals, &i, &cfg, &ctl, cold_boot);

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

    let mut sample_rate_elem = types::Elem::new(
        "Speaker Sample Rate".to_string(),
        &ctl,
        alsa::ctl::ElemType::Integer,
    );
    let mut sample_rate = sample_rate_elem.read_int(&ctl);

    let mut unlock_elem = types::Elem::new(
        "Speaker Volume Unlock".to_string(),
        &ctl,
        alsa::ctl::ElemType::Integer,
    );

    unlock_elem.write_int(&ctl, UNLOCK_MAGIC);

    for (_idx, group) in groups.iter_mut() {
        if cold_boot {
            // Preset the gains to no reduction on cold boot
            group.speakers.iter_mut().for_each(|s| s.update(&ctl, 0.0));
            group.gain = 0.0;
        } else {
            // Leave the gains at whatever the kernel limit is, use anything
            // random for group.gain so the gains will update on the first cycle.
            group.gain = -999.0;
        }
    }

    let mut last_update = Instant::now();

    loop {
        // Block while we're reading into the buffer
        io.readi(&mut buf).unwrap();

        let cur_sample_rate = sample_rate_elem.read_int(&ctl);

        if cur_sample_rate != 0 {
            if cur_sample_rate != sample_rate {
                sample_rate = cur_sample_rate;
                info!("Sample rate: {}", sample_rate);
            }
        }

        if sample_rate == 0 {
            panic!("Invalid sample rate");
        }

        let now = Instant::now();
        let dt = (now - last_update).as_secs_f64();
        assert!(dt > 0f64);

        let pt = globals.period as f64 / sample_rate as f64;
        /* If we skipped at least 4 periods, run catchup for that minus one */
        if dt > (4f64 * pt) {
            let skip = dt - pt;
            debug!("Skipping {:.2} seconds", skip);
            for (_, group) in groups.iter_mut() {
                group.speakers.iter_mut().for_each(|s| s.skip_model(skip));
            }
        }

        last_update = now;

        for (idx, group) in groups.iter_mut() {
            let gain = group
                .speakers
                .iter_mut()
                .map(|s| s.run_model(&buf, sample_rate as f32))
                .reduce(f32::min)
                .unwrap();
            if gain != group.gain {
                if gain == 0. {
                    info!("Speaker group {} gain nominal", idx);
                } else {
                    info!("Speaker group {} gain limited to {:.2} dBFS", idx, gain);
                }
                group.speakers.iter_mut().for_each(|s| s.update(&ctl, gain));
                group.gain = gain;
            }
        }

        unlock_elem.write_int(&ctl, UNLOCK_MAGIC);
    }
}
