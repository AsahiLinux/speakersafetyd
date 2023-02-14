// SPDX-License-Identifier: MIT
// (C) 2022 The Asahi Linux Contributors

use std::ffi::{CString, CStr};
use configparser::ini::Ini;
use alsa::ctl::Ctl;

use crate::helpers;

/**
    Struct with fields necessary for manipulating an ALSA elem.

    The val field is created using a wrapper so that we can handle
    any errors.
*/
struct Elem {
    elem_name: String,
    id: alsa::ctl::ElemId,
    val: alsa::ctl::ElemValue,
}

trait ALSAElem {
    fn new(name: String, card: &Ctl, t: alsa::ctl::ElemType) -> Self;
}

impl ALSAElem for Elem {
        fn new(name: String, card: &Ctl, t: alsa::ctl::ElemType) -> Elem {
        // CString::new() cannot borrow a String. We want name for the elem
        // for error identification though, so it can't consume name directly.
        let borrow: String = name.clone();

        let mut new_elem: Elem = { Elem {
            elem_name: name,
            id: alsa::ctl::ElemId::new(alsa::ctl::ElemIface::Mixer),
            val: helpers::new_elemvalue(t),
        }};

        let cname: CString = CString::new(borrow).unwrap();
        let cstr: &CStr = cname.as_c_str();

        new_elem.id.set_name(cstr);
        new_elem.val.set_id(&new_elem.id);
        helpers::read_ev(card, &mut new_elem.val, &new_elem.elem_name);

       return new_elem;
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
    vsense: Elem,
    isense: Elem,
}

trait ALSACtl {
    fn new(name: &str, card: &Ctl) -> Self;

    fn get_lvl(&mut self, card: &Ctl) -> f32;
    fn set_lvl(&mut self, card: &Ctl, lvl: f32);
}

impl ALSACtl for Mixer {
    // TODO: implement turning on V/ISENSE
    fn new(name: &str, card: &Ctl) -> Mixer {
        let new_mixer: Mixer = { Mixer {
            drv: name.to_owned(),
            level: ALSAElem::new(name.to_owned() + " Speaker Volume", card,
                                 alsa::ctl::ElemType::Integer),
            vsense: ALSAElem::new(name.to_owned() + " VSENSE Switch", card,
                                  alsa::ctl::ElemType::Boolean),
            isense: ALSAElem::new(name.to_owned() + " ISENSE Switch", card,
                                  alsa::ctl::ElemType::Boolean),
        }};

        return new_mixer;
    }

    fn get_lvl(&mut self, card: &Ctl) -> f32 {
        helpers::read_ev(card, &mut self.level.val, &self.level.elem_name);

        let val: i32 = match self.level.val.get_integer(0) {
            Some(inner) => inner,
            None => {
                println!("Could not read level from {}", self.drv);
                helpers::fail();
                std::process::exit(1);
            },
        };

        let db: f32 = helpers::int_to_db(card, &self.level.id, val).to_db();

        return db;
    }

    fn set_lvl(&mut self, card: &Ctl, lvl: f32) {

        let new_val: i32 = helpers::db_to_int(card, &self.level.id, lvl);

        match self.level.val.set_integer(0, new_val) {
            Some(_) => {},
            None => {
                println!("Could not set level for {}", self.drv);
                helpers::fail();
                std::process::exit(1);
            },
        };

        helpers::write_ev(card, &self.level.val, &self.level.elem_name);

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
    temp_limit:  absolute max temp of the voice coil (*C)

    Borrows the handle to the control interface to do calculations.
*/
pub struct Speaker {
    name: String,
    alsa_iface: Mixer,
    tau_coil: f32,
    tau_magnet: f32,
    tr_coil: f32,
    temp_limit: f32,
    vs_chan: i64,
    is_chan: i64,
}


pub trait SafetyMonitor {
    fn new(driver_name: &str, config: &Ini, card: &Ctl) -> Self;
    fn power_now(&mut self, vs: &[i16], is: &[i16]) -> f32;
    fn run(&mut self, card: &Ctl, buf: &[i16; 128 * 6 * 2]);
}

impl SafetyMonitor for Speaker {
    fn new(driver_name: &str, config: &Ini, card: &Ctl) -> Speaker {
        let new_speaker: Speaker = { Speaker {
            name: driver_name.to_string(),
            alsa_iface: ALSACtl::new(&driver_name, card),
            tau_coil: helpers::parse_float(config, driver_name, "tau_coil"),
            tau_magnet: helpers::parse_float(config, driver_name, "tau_magnet"),
            tr_coil: helpers::parse_float(config, driver_name, "tr_coil"),
            temp_limit: helpers::parse_float(config, driver_name, "temp_limit"),
            vs_chan: helpers::parse_int(config, driver_name, "vs_chan"),
            is_chan: helpers::parse_int(config, driver_name, "is_chan"),

        }};

        return new_speaker;
    }

    fn power_now(&mut self, vs: &[i16], is: &[i16]) -> f32 {
        let v_avg: f32 = (vs.iter().sum::<i16>() as f32 / vs.len() as f32) * (14 / (2 ^ 15)) as f32;
        let i_avg: f32 = (is.iter().sum::<i16>() as f32 / is.len() as f32) * (14 / (2 ^ 15)) as f32;

        return v_avg * i_avg;
    }

    // I'm not sure on the maths here for determining when to start dropping the volume.
    fn run(&mut self, card: &Ctl, buf: &[i16; 128 * 6 * 2]) {
        let lvl: f32 = self.alsa_iface.get_lvl(card);
        let vsense = &buf[(128 * self.vs_chan as usize - 1) .. (128 * self.vs_chan as usize - 1) + 128];
        let isense = &buf[(128 * self.is_chan as usize - 1) .. (128 * self.is_chan as usize - 1) + 128];

        // Estimate temperature of VC and magnet
        let temp0: f32 = 35f32;
        let mut temp_vc: f32 = temp0;
        let mut temp_magnet: f32 = temp0;
        let alpha_vc: f32 = 0.01 / (temp_vc + 0.01);
        let alpha_magnet: f32 = 0.01 / (temp_magnet + 0.01);

        // Power through the voice coil (average of most recent 128 samples)
        let pwr: f32 = self.power_now(&vsense, &isense);

        let vc_target: f32 = temp_magnet + pwr * self.tau_coil;
        temp_vc = vc_target * alpha_vc + temp_vc * (1.0 - alpha_vc);
        println!("Current voice coil temp: {:.2}*C", temp_vc);

        let magnet_target: f32 = temp0 + pwr * self.tau_magnet;
        temp_magnet = magnet_target  * alpha_magnet + temp_magnet * (1.0 - alpha_magnet);
        println!("Current magnet temp: {:.2}*C", temp_magnet);

        if temp_vc < self.temp_limit {
            println!("Voice coil for {} below temp limit, ramping back up.", self.name);
            // For every degree below temp_limit, raise level by 0.5 dB
            let new_lvl: f32 = lvl + ((self.temp_limit - temp_vc) as f32 * 0.5);
            self.alsa_iface.set_lvl(card, new_lvl);
        }

        if temp_vc > (self.temp_limit - 15f32) {
            println!("Voice coil at {}*C on {}! Dropping volume!", temp_vc, self.name);
            // For every degree above temp_limit, drop the level by 1.5 dB
            let new_lvl: f32 = lvl - ((temp_vc - self.temp_limit) as f32 * 1.5);
            self.alsa_iface.set_lvl(card, new_lvl);
        }

        println!("Volume on {} is currently {} dB. Setting to -18 dB.", self.name, lvl);

        let new_lvl: f32 = -18.0;
        self.alsa_iface.set_lvl(card, new_lvl);

        println!("Volume on {} is now {} dB", self.name, self.alsa_iface.get_lvl(card));
    }
}
