// SPDX-License-Identifier: MIT
// (C) 2022 The Asahi Linux Contributors

use std::ffi::{CString, CStr};
use half::f16;
use configparser::ini::Ini;
use alsa::ctl::Ctl;

use crate::helpers;

/**
    Struct with fields necessary for manipulating an ALSA elem.

    The val field is created using a wrapper so that we can handle
    any errors. This is also necessary so that we can create one of type
    Bytes for the V/ISENSE elems.
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
    vsense: VSENSE as reported by the driver (V, readonly)
    isense: ISENSE as reported by the driver (A, readonly)

*/
struct Mixer {
    drv: String,
    level: Elem,
    vsense: Elem,
    isense: Elem,
}

trait ALSACtl {
    fn new(name: &str, card: &Ctl) -> Self;

    fn get_vsense(&mut self, card: &Ctl) -> f16;
    fn get_isense(&mut self, card: &Ctl) -> f16;
    fn get_lvl(&mut self, card: &Ctl) -> f32;
    fn set_lvl(&mut self, card: &Ctl, lvl: f32);
}

impl ALSACtl for Mixer {
    // TODO: wire up real V/ISENSE elems (pending driver support)
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

    /**
        MOCK IMPLEMENTATIONS

        V/ISENSE are 16-bit floats sent in a 32-bit TDM slot by the codec.
        This is expressed by the driver as a byte array, with rightmost 16
        bits as padding.

        TODO: Condense into a single function and pass in a borrowed Elem
    */
    fn get_vsense(&mut self, card: &Ctl) -> f16 {
        helpers::read_ev(card, &mut self.vsense.val, &self.vsense.elem_name);
        let val: &[u8] = match self.vsense.val.get_bytes() {
            Some(inner) => inner,
            None => {
                println!("Could not read VSENSE from {}", self.drv);
                helpers::fail();
                std::process::exit(1);
            }
        };


        let vs = f16::from_ne_bytes([val[0], val[1]]);

        return vs;
    }

    fn get_isense(&mut self, card: &Ctl) -> f16 {
        helpers::read_ev(card, &mut self.isense.val, &self.isense.elem_name);
        let val: &[u8] = match self.vsense.val.get_bytes() {
            Some(inner) => inner,
            None => {
                println!("Could not read ISENSE from {}", self.drv);
                helpers::fail();
                std::process::exit(1);
            }
        };


        let is = f16::from_ne_bytes([val[0], val[1]]);

        return is;
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
    tau_coil: f64,
    tr_coil: f64,
    temp_limit: f64,
}


pub trait SafetyMonitor {
    fn new(driver_name: &str, config: &Ini, card: &Ctl) -> Self;

    fn run(&mut self, card: &Ctl);
}

impl SafetyMonitor for Speaker {
    fn new(driver_name: &str, config: &Ini, card: &Ctl) -> Speaker {
        let new_speaker: Speaker = { Speaker {
            name: driver_name.to_string(),
            alsa_iface: ALSACtl::new(&driver_name, card),
            tau_coil: helpers::parse_float(config, driver_name, "tau_coil"),
            tr_coil: helpers::parse_float(config, driver_name, "tr_coil"),
            temp_limit: helpers::parse_float(config, driver_name, "temp_limit"),
        }};

        return new_speaker;
    }

    // I'm not sure on the maths here for determining when to start dropping the volume.
    fn run(&mut self, card: &Ctl) {
        //let v: f16 = self.alsa_iface.get_vsense(card);
        //let i: f16 = self.alsa_iface.get_isense(card);
        let lvl: f32 = self.alsa_iface.get_lvl(card);

        // Technically, this is the temp ~tau_coil seconds in the future
        //let temp: f64 = ((v * i).to_f64()) * self.tr_coil;

        // if temp < self.temp_limit && lvl < 0f32 {
        //     println!("Voice coil for {} below temp limit, ramping back up.", self.name);
        //
        //     // For every degree below temp_limit, raise level by 0.5 dB
        //     let new_lvl: f32 = lvl + ((self.temp_limit - temp) as f32 * 0.5);
        //     self.alsa_iface.set_lvl(card, new_lvl);
        // }
        //
        // if temp > self.temp_limit {
        //     println!("Voice coil at {}*C in {} on {}! Dropping volume!", temp, self.tau_coil, self.name);
        //
        //     // For every degree above temp_limit, drop the level by 1.5 dB
        //     let new_lvl: f32 = lvl - ((temp - self.temp_limit) as f32 * 1.5);
        //     self.alsa_iface.set_lvl(card, new_lvl);
        // }

        // TEMPORARY PROOF THAT THIS WORKS!

        println!("Volume on {} is currently {} dB. Setting to -18 dB.", self.name, lvl);

        let new_lvl: f32 = -18.0;
        self.alsa_iface.set_lvl(card, new_lvl);

        println!("Volume on {} is now {} dB", self.name, self.alsa_iface.get_lvl(card));

    }

}
