// SPDX-License-Identifier: MIT
// (C) 2022 The Asahi Linux Contributors

use configparser::ini::Ini;
use alsa::mixer::MilliBel;
use alsa;

/**
    Failsafe: Limit speaker volume massively and bail.

    TODO: enable TAS safe mode with IOCTL.
*/
pub fn fail() {
    println!("A catastrophic error has occurred.");
    std::process::exit(1);
}

pub fn open_card(card: &str) -> alsa::ctl::Ctl {
    let ctldev: alsa::ctl::Ctl = match alsa::ctl::Ctl::new(card, false) {
        Ok(ctldev) => ctldev,
        Err(e) => {
            println!("{}: Could not open sound card! Error: {}", card, e);
            fail();
            std::process::exit(1);
        },
    };

    return ctldev;
}

pub fn open_pcm(dev: &str, chans: &u32) -> alsa::pcm::PCM {
    let pcm = alsa::pcm::PCM::new(dev, alsa::Direction::Capture, false)
        .unwrap();
    {
        let params = alsa::pcm::HwParams::any(&pcm).unwrap();

        params.set_channels(*chans).unwrap();
        params.set_rate(44100, alsa::ValueOr::Nearest).unwrap();
        params.set_format(alsa::pcm::Format::s16()).unwrap();
        params.set_access(alsa::pcm::Access::RWNonInterleaved).unwrap();
        pcm.hw_params(&params).unwrap();
    }

    return pcm;
}

/**
    Wrapper around configparser::ini::Ini.getint()
    to safely unwrap the Result<Option<f64>, E> returned by
    it.
*/
pub fn parse_int(config: &Ini, section: &str, key: &str) -> i64 {
    let _result: Option<i64> = match config.getint(section, key) {
            Ok(result) => match result{
                Some(inner) => {
                    let integer: i64 = inner;
                    return integer;
                },
                None => {
                    println!("{}: Failed to parse {}", section, key);
                    fail();
                    std::process::exit(1);
                },
            },
            Err(e) => {
                println!("{}: Invalid value for {}. Error: {}", section, key, e);
                fail();
                std::process::exit(1);
            },
    };

}

/**
    Wrapper around configparser::ini::Ini.getfloat()
    to safely unwrap the Result<Option<f64>, E> returned by
    it.
*/
pub fn parse_float(config: &Ini, section: &str, key: &str) -> f32 {
    let _result: Option<f64> = match config.getfloat(section, key) {
            Ok(result) => match result{
                Some(inner) => {
                    let float: f32 = inner as f32;
                    return float;
                },
                None => {
                    println!("{}: Failed to parse {}", section, key);
                    fail();
                    std::process::exit(1);
                },
            },
            Err(e) => {
                println!("{}: Invalid value for {}. Error: {}", section, key, e);
                fail();
                std::process::exit(1);
            },
    };

}

/**
    Wrapper around alsa::ctl::ElemValue::new(). Lets us bail on errors and
    pass in the Bytes type for V/ISENSE
*/
pub fn new_elemvalue(t: alsa::ctl::ElemType) -> alsa::ctl::ElemValue {
    let val = match alsa::ctl::ElemValue::new(t) {
        Ok(val) => val,
        Err(_e) => {
            println!("Could not open a handle to an element!");
            fail();
            std::process::exit(1);
        },
    };

    return val;
}


/**
    Wrapper for alsa::ctl::Ctl::elem_read().
*/
pub fn read_ev(card: &alsa::ctl::Ctl, ev: &mut alsa::ctl::ElemValue, name: &str) {
    let _val = match card.elem_read(ev) { // alsa:Result<()>
            Ok(val) => val,
            Err(e) => {
                println!("Could not read elem value {}. alsa-lib error: {:?}", name, e);
                fail();
                std::process::exit(1);
            },
        };
}

/**
    Wrapper for alsa::ctl::Ctl::elem_write().
*/
pub fn write_ev(card: &alsa::ctl::Ctl, ev: &alsa::ctl::ElemValue, name: &str) {
    let _val = match card.elem_write(ev) { // alsa:Result<()>
            Ok(val) => val,
            Err(e) => {
                println!("Could not write elem value {}. alsa-lib error: {:?}", name, e);
                fail();
                std::process::exit(1);
            },
        };
}

pub fn int_to_db(card: &alsa::ctl::Ctl, id: &alsa::ctl::ElemId, val: i32) -> MilliBel {
    let db = match card.convert_to_db(id, val.into()) {
        Ok(inner) => inner,
        Err(e) => {
            println!("Could not convert val {} to dB! alsa-lib error: {:?}", val, e);
            fail();
            std::process::exit(1);
        },
    };

    return db;
}

pub fn db_to_int(card: &alsa::ctl::Ctl, id: &alsa::ctl::ElemId, val: f32) -> i32 {
    let mb: MilliBel = MilliBel((val * 100.0) as i64);
    let new_int = match card.convert_from_db(id, mb, alsa::Round::Floor) {
        Ok(inner) => inner as i32,
        Err(e) => {
            println!("Could not convert MilliBel {:?} to int! alsa-lib error: {:?}", val, e);
            fail();
            std::process::exit(1);
        },
    };

    return new_int;
}
