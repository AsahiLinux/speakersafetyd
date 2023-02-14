// SPDX-License-Identifier: MIT
// (C) 2022 The Asahi Linux Contributors
/**
    Handles speaker safety on Apple Silicon machines. This code is designed to
    fail safe. The speaker should not be enabled until this daemon has successfully
    initialised. If at any time we run into an unrecoverable error (we shouldn't),
    we gracefully bail and use an IOCTL to shut off the speakers.
*/
use std::io;
use std::fs::read_to_string;
use std::{thread::sleep, time};

use configparser::ini::Ini;

mod types;
mod helpers;

use crate::types::SafetyMonitor;

static ASAHI_DEVICE: &str = "hw:0";
static VISENSE_PCM: &str = "hw:0,2";

// Will eventually be /etc/speakersafetyd/ or similar
static CONFIG_DIR: &str = "./";
static SUPPORTED: [&str; 1] = [
    "j314",
];

const BUF_SZ: usize = 128 * 6 * 2;

fn get_machine() -> String {
    let _compat: io::Result<String> = match read_to_string("/proc/device-tree/compatible") {
        Ok(compat) => {
            return compat[6..10].to_string();
        },
        Err(e) => {
            println!("Could not read devicetree compatible: {}", e);
            std::process::exit(1);
        }
    };

}


fn get_drivers(config: &Ini) -> Vec<String> {

    let drivers = config.sections();

    return drivers;
}


fn main() {
    let model: String = get_machine();
    let mut cfg: Ini = Ini::new_cs();
    let mut speakers: Vec<types::Speaker> = Vec::new();
    let card: alsa::ctl::Ctl = helpers::open_card(&ASAHI_DEVICE);

    if SUPPORTED.contains(&model.as_str()) {
        cfg.load(CONFIG_DIR.to_owned() + &model + ".conf").unwrap();
    } else {
        println!("Unsupported machine {}", model);
        std::process::exit(1);
    }

    let list_drivers = get_drivers(&cfg);

    for i in list_drivers {
        let new_speaker: types::Speaker = types::SafetyMonitor::new(&i, &cfg, &card);
        speakers.push(new_speaker);
    }

    let num_chans: u32 = speakers.len().try_into().unwrap();

    // Set up PCM to buffer in V/ISENSE
    let cap: alsa::pcm::PCM = helpers::open_pcm(&VISENSE_PCM, &num_chans);
    let mut buf = [0i16; BUF_SZ]; // 128 samples from V and I for 6 channels
    let io = cap.io_i16().unwrap();

    loop {
        // Block while we're reading into the buffer
        io.readi(&mut buf).unwrap();
        for i in &mut speakers {
            i.run(&card, &buf);
        }
    }
}
