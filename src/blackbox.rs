use crate::types::SpeakerState;
use chrono;
use log::warn;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::Path;
use std::slice;

use json::object;

struct Block {
    sample_rate: i32,
    state: Vec<Vec<SpeakerState>>,
    data: Vec<i16>,
}

pub struct Blackbox {
    machine: String,
    globals: crate::types::Globals,
    path: Box<Path>,
    blocks: Vec<Block>,
}

/// Maximum number of blocks in the ring buffer (around 30 seconds at 4096/48000)
const MAX_BLOCKS: usize = 330;

impl Blackbox {
    pub fn new(machine: &str, path: &Path, globals: &crate::types::Globals) -> Blackbox {
        Blackbox {
            machine: machine.into(),
            globals: globals.clone(),
            path: path.into(),
            blocks: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.blocks.clear();
    }

    pub fn push(&mut self, sample_rate: i32, data: Vec<i16>, state: Vec<Vec<SpeakerState>>) {
        while self.blocks.len() >= MAX_BLOCKS {
            self.blocks.remove(0);
        }
        self.blocks.push(Block {
            sample_rate,
            state,
            data,
        })
    }

    pub fn preserve(&mut self, reason: String) -> io::Result<()> {
        if self.blocks.is_empty() {
            warn!("Blackbox is empty, nothing to save");
            return Ok(());
        }

        let now = chrono::Local::now().to_rfc3339();
        let meta_name = self.path.join(now.clone() + ".meta");
        let data_name = self.path.join(now.clone() + ".raw");

        warn!("Preserving blackbox {}", now);

        let mut metafd = File::create(meta_name)?;
        let mut datafd = File::create(data_name)?;

        for blk in self.blocks.iter() {
            // meh unsafe
            let slice_u8: &[u8] = unsafe {
                slice::from_raw_parts(
                    blk.data.as_ptr() as *const u8,
                    blk.data.len() * std::mem::size_of::<u16>(),
                )
            };
            datafd.write_all(slice_u8)?;
        }

        let mut meta = object! {
            message: reason,
            machine: self.machine.clone(),
            sample_rate: self.blocks[0].sample_rate,
            channels: self.globals.channels,
            t_ambient: self.globals.t_ambient,
            t_safe_max: self.globals.t_safe_max,
            t_hysteresis: self.globals.t_hysteresis,
            state: null
        };

        let mut state = json::JsonValue::new_array();

        for group in self.blocks[0].state.iter() {
            for speaker in group.iter() {
                let _ = state.push(object! {
                    t_coil: speaker.t_coil,
                    t_magnet: speaker.t_magnet,
                    t_coil_hyst: speaker.t_coil_hyst,
                    t_magnet_hyst: speaker.t_magnet_hyst,
                    min_gain: speaker.min_gain,
                    gain: speaker.gain,
                });
            }
        }

        meta["state"] = state;

        metafd.write_all(meta.dump().as_bytes())?;

        Ok(())
    }
}
