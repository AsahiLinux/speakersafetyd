// SPDX-License-Identifier: MIT
// (C) 2022 The Asahi Linux Contributors

use alsa::ctl::Ctl;
use configparser::ini::Ini;
use log::{debug, info};
use std::ffi::{CStr, CString};

use crate::helpers;

/**
    Struct with fields necessary for manipulating an ALSA elem.

    The val field is created using a wrapper so that we can handle
    any errors.
*/
pub struct Elem {
    elem_name: String,
    id: alsa::ctl::ElemId,
    val: alsa::ctl::ElemValue,
}

impl Elem {
    pub fn new(name: String, card: &Ctl, t: alsa::ctl::ElemType) -> Elem {
        // CString::new() cannot borrow a String. We want name for the elem
        // for error identification though, so it can't consume name directly.
        let borrow: String = name.clone();

        let mut new_elem: Elem = {
            Elem {
                elem_name: name,
                id: alsa::ctl::ElemId::new(alsa::ctl::ElemIface::Mixer),
                val: helpers::new_elemvalue(t),
            }
        };

        let cname: CString = CString::new(borrow).unwrap();
        let cstr: &CStr = cname.as_c_str();

        new_elem.id.set_name(cstr);
        new_elem.val.set_id(&new_elem.id);
        helpers::lock_el(card, &new_elem.id, &new_elem.elem_name);
        helpers::read_ev(card, &mut new_elem.val, &new_elem.elem_name);

        return new_elem;
    }

    pub fn read_int(&mut self, card: &Ctl) -> i32 {
        helpers::read_ev(card, &mut self.val, &self.elem_name);

        self.val
            .get_integer(0)
            .expect(&format!("Could not read {}", self.elem_name))
    }

    pub fn write_int(&mut self, card: &Ctl, value: i32) {
        self.val
            .set_integer(0, value)
            .expect(&format!("Could not set {}", self.elem_name));
        helpers::write_ev(card, &mut self.val, &self.elem_name);
    }
}

/**
    Mixer struct representing the controls associated with a given
    Speaker. Populated with the important ALSA controls at runtime.

    level:  mixer volume control
    vsense: VSENSE switch
    isense: ISENSE switch

*/
struct Mixer {
    drv: String,
    level: Elem,
    amp_gain: Elem,
}

impl Mixer {
    // TODO: implement turning on V/ISENSE
    fn new(name: &str, card: &Ctl, globals: &Globals) -> Mixer {
        let prefix = if name == "Mono" {
            "".to_string()
        } else {
            name.to_owned() + " "
        };

        let mut vs = Elem::new(
            prefix.clone() + &globals.ctl_vsense,
            card,
            alsa::ctl::ElemType::Boolean,
        );

        vs.val.set_boolean(0, true);
        helpers::write_ev(card, &vs.val, &vs.elem_name);
        helpers::read_ev(card, &mut vs.val, &vs.elem_name);
        assert!(vs.val.get_boolean(0).unwrap());

        let mut is = Elem::new(
            prefix.clone() + &globals.ctl_isense,
            card,
            alsa::ctl::ElemType::Boolean,
        );

        is.val.set_boolean(0, true);
        helpers::write_ev(card, &is.val, &is.elem_name);
        helpers::read_ev(card, &mut vs.val, &vs.elem_name);
        assert!(vs.val.get_boolean(0).unwrap());

        let mut ret = Mixer {
            drv: name.to_owned(),
            level: Elem::new(
                prefix.clone() + &globals.ctl_volume,
                card,
                alsa::ctl::ElemType::Integer,
            ),
            amp_gain: Elem::new(
                prefix + &globals.ctl_amp_gain,
                card,
                alsa::ctl::ElemType::Integer,
            ),
        };

        /*
         * Set amp gain to max available (kernel should've clamped).
         * alsa-rs only has bindings for range in dB, so we go through
         * that.
         */

        let (_min, max) =
            helpers::get_range_db(card, &mut ret.amp_gain.id, &ret.amp_gain.elem_name);
        let max_int = card
            .convert_from_db(&mut ret.amp_gain.id, max, alsa::Round::Floor)
            .unwrap();

        ret.amp_gain.val.set_integer(0, max_int.try_into().unwrap());

        helpers::write_ev(card, &ret.amp_gain.val, &ret.amp_gain.elem_name);

        ret
    }

    fn get_amp_gain(&mut self, card: &Ctl) -> f32 {
        helpers::read_ev(card, &mut self.amp_gain.val, &self.amp_gain.elem_name);

        let val = self
            .amp_gain
            .val
            .get_integer(0)
            .expect(&format!("Could not read amp gain for {}", self.drv));

        helpers::int_to_db(card, &self.amp_gain.id, val).to_db()
    }

    /*
    fn get_lvl(&mut self, card: &Ctl) -> f32 {
        helpers::read_ev(card, &mut self.level.val, &self.level.elem_name);

        let val = self
            .level
            .val
            .get_integer(0)
            .expect(&format!("Could not read level for {}", self.drv));

        helpers::int_to_db(card, &self.level.id, val).to_db()
    }
    */

    fn set_lvl(&mut self, card: &Ctl, lvl: f32) {
        let new_val: i32 = helpers::db_to_int(card, &self.level.id, lvl);

        match self.level.val.set_integer(0, new_val) {
            Some(_) => {}
            None => {
                panic!("Could not set level for {}", self.drv);
            }
        };

        helpers::write_ev(card, &self.level.val, &self.level.elem_name);
    }
}

#[derive(Clone)]
pub struct Globals {
    pub visense_pcm: usize,
    pub channels: usize,
    pub period: usize,
    pub t_ambient: f32,
    pub t_window: f32,
    pub t_hysteresis: f32,
    pub ctl_vsense: String,
    pub ctl_isense: String,
    pub ctl_amp_gain: String,
    pub ctl_volume: String,
    pub uclamp_min: Option<usize>,
    pub uclamp_max: Option<usize>,
}

impl Globals {
    pub fn parse(config: &Ini) -> Self {
        Self {
            visense_pcm: helpers::parse_int(config, "Globals", "visense_pcm"),
            channels: helpers::parse_int(config, "Globals", "channels"),
            period: helpers::parse_int(config, "Globals", "period"),
            t_ambient: helpers::parse_float(config, "Globals", "t_ambient"),
            t_window: helpers::parse_float(config, "Globals", "t_window"),
            t_hysteresis: helpers::parse_float(config, "Globals", "t_hysteresis"),
            ctl_vsense: helpers::parse_string(config, "Controls", "vsense"),
            ctl_isense: helpers::parse_string(config, "Controls", "isense"),
            ctl_amp_gain: helpers::parse_string(config, "Controls", "amp_gain"),
            ctl_volume: helpers::parse_string(config, "Controls", "volume"),
            uclamp_min: helpers::parse_opt_int(config, "Globals", "uclamp_min"),
            uclamp_max: helpers::parse_opt_int(config, "Globals", "uclamp_max"),
        }
    }
}

/**
    Struct representing a driver. Parameters are parsed out of a config
    file, which is loaded at runtime based on the machine's DT compatible
    string.

    name:        driver name as it appears in ALSA
    alsa_iface:  Mixer struct with handles to the driver's control elements
    r_dc:        dc resistance of the voice coil (ohms)
    tau_coil:    voice coil ramp time constant (seconds)
    tau_magnet:  magnet ramp time constant (seconds)
    tr_coil:     thermal resistance of voice coil (*C/W)
    t_limit:  absolute max temp of the voice coil (*C)

    Borrows the handle to the control interface to do calculations.
*/
#[derive(Debug, Default, Copy, Clone)]
pub struct SpeakerState {
    pub t_coil: f64,
    pub t_magnet: f64,

    pub t_coil_hyst: f32,
    pub t_magnet_hyst: f32,

    pub min_gain: f32,
    pub gain: f32,
}

pub struct Speaker {
    pub name: String,
    pub group: usize,
    alsa_iface: Mixer,
    tau_coil: f32,
    tau_magnet: f32,
    tr_coil: f32,
    tr_magnet: f32,
    t_limit: f32,
    t_headroom: f32,
    z_nominal: f32,
    is_scale: f32,
    vs_scale: f32,
    is_chan: usize,
    vs_chan: usize,

    g: Globals,
    pub s: SpeakerState,
}

impl Speaker {
    pub fn new(globals: &Globals, name: &str, config: &Ini, ctl: &Ctl, cold_boot: bool) -> Speaker {
        info!("Speaker [{}]:", name);

        let section = "Speaker/".to_owned() + name;
        let mut new_speaker: Speaker = Speaker {
            name: name.to_string(),
            alsa_iface: Mixer::new(&name, ctl, globals),
            group: helpers::parse_int(config, &section, "group"),
            tau_coil: helpers::parse_float(config, &section, "tau_coil"),
            tau_magnet: helpers::parse_float(config, &section, "tau_magnet"),
            tr_coil: helpers::parse_float(config, &section, "tr_coil"),
            tr_magnet: helpers::parse_float(config, &section, "tr_magnet"),
            t_limit: helpers::parse_float(config, &section, "t_limit"),
            t_headroom: helpers::parse_float(config, &section, "t_headroom"),
            z_nominal: helpers::parse_float(config, &section, "z_nominal"),
            is_scale: helpers::parse_float(config, &section, "is_scale"),
            vs_scale: helpers::parse_float(config, &section, "vs_scale"),
            is_chan: helpers::parse_int(config, &section, "is_chan"),
            vs_chan: helpers::parse_int(config, &section, "vs_chan"),
            g: globals.clone(),
            s: Default::default(),
        };

        let s = &mut new_speaker.s;

        s.t_coil = if cold_boot {
            // Assume warm but not warm enough to limit
            (new_speaker.t_limit - globals.t_window) as f64 - 1f64
        } else {
            // Worst case startup assumption
            new_speaker.t_limit as f64
        };
        s.t_magnet = globals.t_ambient as f64
            + (s.t_coil - globals.t_ambient as f64)
                * (new_speaker.tr_magnet / (new_speaker.tr_magnet + new_speaker.tr_coil)) as f64;

        let max_dt = new_speaker.t_limit - globals.t_ambient;
        let max_pwr = max_dt / (new_speaker.tr_magnet + new_speaker.tr_coil);

        let amp_gain = new_speaker.alsa_iface.get_amp_gain(ctl);

        // Worst-case peak power is 2x RMS power
        let peak_pwr = 10f32.powf(amp_gain / 10.) / new_speaker.z_nominal * 2.;

        s.min_gain = ((max_pwr / peak_pwr).log10() * 10.).min(0.);

        assert!(new_speaker.is_chan < globals.channels);
        assert!(new_speaker.vs_chan < globals.channels);
        assert!(new_speaker.t_limit - globals.t_window > globals.t_ambient);

        info!("  Group: {}", new_speaker.group);
        info!("  Max temperature: {:.1} °C", new_speaker.t_limit);
        info!("  Amp gain: {} dBV", amp_gain);
        info!("  Max power: {:.2} W", max_pwr);
        info!("  Peak power: {} W", peak_pwr);
        info!("  Min gain: {:.2} dB", s.min_gain);

        new_speaker
    }

    pub fn run_model(&mut self, buf: &[i16], sample_rate: f32) -> f32 {
        let s = &mut self.s;

        let step = 1. / sample_rate;
        let alpha_coil = (step / (self.tau_coil + step)) as f64;
        let alpha_magnet = (step / (self.tau_magnet + step)) as f64;

        let mut pwr_sum = 0f32;

        for sample in buf.chunks(self.g.channels) {
            assert!(sample.len() == self.g.channels);

            let v = sample[self.vs_chan] as f32 / 32768.0 * self.vs_scale;
            let i = sample[self.is_chan] as f32 / 32768.0 * self.is_scale;
            let p = v * i;

            let t_coil_target = s.t_magnet + (p * self.tr_coil) as f64;
            let t_magnet_target = (self.g.t_ambient + p * self.tr_magnet) as f64;

            s.t_coil = t_coil_target * alpha_coil + s.t_coil * (1. - alpha_coil);
            s.t_magnet = t_magnet_target * alpha_magnet + s.t_magnet * (1. - alpha_magnet);

            if s.t_coil > (self.t_limit + self.t_headroom) as f64 {
                panic!(
                    "{}: Coil temperature limit exceeded ({} > {})",
                    self.name, s.t_coil, self.t_limit
                );
            }
            if s.t_magnet > (self.t_limit + self.t_headroom) as f64 {
                panic!(
                    "{}: Magnet temperature limit exceeded ({} > {})",
                    self.name, s.t_magnet, self.t_limit
                );
            }

            pwr_sum += p;
        }

        let pwr_avg: f32 = pwr_sum / ((buf.len() / self.g.channels) as f32);
        /*
         * This really shouldn't happen other than rounding error,
         * if it does there's probably something wrong with the ivsense
         * data.
         */
        if pwr_avg < -0.01 {
            panic!(
                "{}: Negative power, bad ivsense data? ({})",
                self.name, pwr_avg
            );
        }
        let pwr_avg = pwr_avg.max(0.0);

        s.t_coil_hyst = s
            .t_coil_hyst
            .max(s.t_coil as f32)
            .min(s.t_coil as f32 + self.g.t_hysteresis);
        s.t_magnet_hyst = s
            .t_magnet_hyst
            .max(s.t_magnet as f32)
            .min(s.t_magnet as f32 + self.g.t_hysteresis);

        let temp = s.t_coil_hyst.max(s.t_magnet_hyst);

        let reduction = (temp - (self.t_limit - self.g.t_window)) / self.g.t_window;
        let gain = s.min_gain * reduction.max(0.);

        s.gain = gain;

        if s.gain > -0.01 {
            s.gain = 0.;
        }

        debug!(
            "{:>15}: Coil {:>6.2} °C Magnet {:>6.2} °C Power {:>5.2} W Gain {:>6.2} dB",
            self.name, s.t_coil, s.t_magnet, pwr_avg, s.gain
        );

        s.gain
    }

    pub fn skip_model(&mut self, time: f64) {
        let s = &mut self.s;
        let t_coil = s.t_coil - self.g.t_ambient as f64;
        let t_magnet = s.t_magnet - self.g.t_ambient as f64;

        let eta = 1f64 / (1f64 - (self.tau_coil / self.tau_magnet) as f64);
        let a = (-time / self.tau_coil as f64).exp() * (t_coil - eta * t_magnet);
        let b = (-time / self.tau_magnet as f64).exp() * t_magnet;

        s.t_coil = self.g.t_ambient as f64 + a + b * eta;
        s.t_magnet = self.g.t_ambient as f64 + b;

        debug!(
            "{}: SKIP: Coil {:.2} °C Magnet {:.2} °C ({:.2} seconds)",
            self.name, s.t_coil, s.t_magnet, time
        );
    }

    pub fn update(&mut self, ctl: &Ctl, gain: f32) {
        self.alsa_iface.set_lvl(ctl, gain);
    }
}
