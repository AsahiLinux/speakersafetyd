// SPDX-License-Identifier: MIT
// (C) 2022 The Asahi Linux Contributors
/*!
    Handles speaker safety on Apple Silicon machines. This code is designed to
    fail safe. The kernel keeps the speakers capped at a low volume level until
    this daemon initializes. If at any time we run into an unrecoverable error
    or a timeout, we panic and let the kernel put the speakers back into a safe
    state.
*/
use std::collections::BTreeMap;
use std::fs;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use alsa::nix::errno::Errno;
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use configparser::ini::Ini;
use log;
use log::{debug, info, warn};
use simple_logger::SimpleLogger;

mod blackbox;
mod helpers;
mod types;
mod uclamp;

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

    /// Path to the blackbox dump directory
    #[arg(short, long)]
    blackbox_path: Option<PathBuf>,

    /// Maximum gain reduction before panicing (for debugging)
    #[arg(short, long)]
    max_reduction: Option<f32>,
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

    let sigquit = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGQUIT, Arc::clone(&sigquit)).unwrap();
    // signal_hook insists on using SA_RESTART, which we don't want. Override it.
    unsafe {
        let mut act: libc::sigaction = core::mem::zeroed();
        assert!(libc::sigaction(signal_hook::consts::SIGQUIT, core::ptr::null(), &mut act) == 0);
        act.sa_flags &= !libc::SA_RESTART;
        assert!(
            libc::sigaction(
                signal_hook::consts::SIGQUIT,
                &mut act,
                core::ptr::null_mut()
            ) == 0
        );
    }

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

    let maker_titlecase = maker[0..1].to_ascii_uppercase() + &maker[1..];

    let device = format!("hw:{}{}", maker_titlecase, model.to_ascii_uppercase());
    info!("Device: {}", device);

    let mut cfg: Ini = Ini::new_cs();
    cfg.load(config_path).expect("Failed to read config file");

    let globals = types::Globals::parse(&cfg);

    if globals.uclamp_min.is_some() || globals.uclamp_max.is_some() {
        uclamp::set_uclamp(
            globals.uclamp_min.unwrap_or(0).try_into().unwrap(),
            globals.uclamp_max.unwrap_or(1024).try_into().unwrap(),
        );
    }

    let mut blackbox = args.blackbox_path.map(|p| {
        info!("Enabling blackbox, path: {:?}", p);
        blackbox::Blackbox::new(&machine, &p, &globals)
    });

    let mut blackbox_ref = AssertUnwindSafe(&mut blackbox);
    let result = catch_unwind(move || {
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
        let mut pcm: Option<alsa::pcm::PCM> =
            Some(helpers::open_pcm(&pcm_name, globals.channels.try_into().unwrap(), 0));
        let mut io = Some(pcm.as_ref().unwrap().io_i16().unwrap());

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

        let mut buf = Vec::new();
        buf.resize(globals.period * globals.channels, 0i16);

        let mut once_nominal = false;

        loop {
            if sigquit.load(Ordering::Relaxed) {
                panic!("SIGQUIT received");
            }
            // Block while we're reading into the buffer
            let read = io.as_ref().unwrap().readi(&mut buf);

            #[allow(unused_mut)]
            #[allow(unused_assignments)]
            let read = match read {
                Ok(a) => Ok(a),
                Err(e) => {
                    if sigquit.load(Ordering::Relaxed) {
                        panic!("SIGQUIT received");
                    }
                    if e.errno() == Errno::ESTRPIPE {
                        warn!("Suspend detected!");
                        /*
                        // Resume handling
                        loop {
                            match pcm.resume() {
                                Ok(_) => break Ok(0),
                                Err(e) if e.errno() == Errno::EAGAIN => continue,
                                Err(e) => break Err(e),
                            }
                        }
                        .unwrap();
                        warn!("Resume successful");
                        */
                        // Work around kernel issue: resume sometimes breaks visense
                        warn!("Reinitializing PCM to work around kernel bug...");
                        io = None;
                        pcm = None;
                        pcm = Some(helpers::open_pcm(&pcm_name, globals.channels.try_into().unwrap(), 0));
                        io = Some(pcm.as_ref().unwrap().io_i16().unwrap());
                        continue;
                    }
                    Err(e)
                }
            }
            .unwrap();

            if read != globals.period {
                warn!("Expected {} samples, got {}", globals.period, read);
            }

            if sigquit.load(Ordering::Relaxed) {
                panic!("SIGQUIT received");
            }

            let buf_read = &buf[0..read * globals.channels];

            let cur_sample_rate = sample_rate_elem.read_int(&ctl);

            if cur_sample_rate != 0 {
                if cur_sample_rate != sample_rate {
                    sample_rate = cur_sample_rate;
                    info!("Sample rate: {}", sample_rate);
                    blackbox_ref.as_mut().map(|bb| bb.reset());
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
                blackbox_ref.as_mut().map(|bb| bb.reset());
            }

            last_update = now;

            if let Some(bb) = blackbox_ref.as_mut() {
                let max_idx = *groups.iter().map(|g| g.0).max().unwrap();
                let gstates = (0..=max_idx)
                    .map(|i| groups[&i].speakers.iter().map(|s| s.s.clone()).collect())
                    .collect();
                bb.push(sample_rate, buf_read.to_vec(), gstates);
            }

            let mut all_nominal = true;
            for (idx, group) in groups.iter_mut() {
                let gain = group
                    .speakers
                    .iter_mut()
                    .map(|s| s.run_model(buf_read, sample_rate as f32))
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
                if gain != 0. {
                    all_nominal = false;
                }
                if let Some(max_reduction) = args.max_reduction {
                    if once_nominal && gain < -max_reduction {
                        panic!("Gain reduction exceeded threshold");
                    }
                }
            }

            if all_nominal {
                once_nominal = true;
            }

            unlock_elem.write_int(&ctl, UNLOCK_MAGIC);
        }
    });
    if let Err(e) = result {
        warn!("Panic!");

        let mut reason: String = "Unknown panic".into();

        if let Some(s) = e.downcast_ref::<&'static str>() {
            reason = (*s).into();
        } else if let Some(s) = e.downcast_ref::<String>() {
            reason = s.clone();
        }

        blackbox.as_mut().map(|bb| {
            if bb.preserve(reason).is_err() {
                warn!("Failed to write blackbox");
            }
        });

        resume_unwind(e);
    }
}
