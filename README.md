## Asahi Linux speaker safety daemon

## IMPORTANT
This software is still pre-release and not fit for use or testing on user machines. Please
do not ask for help with installing or using this software, the Pipewire configuration,
or enabling speaker output on your machine. An announcement will be made when speaker
support is ready for use.

## Requirements
* We currently rely on a local version of the `alsa` crate, as a release has not yet been
  pushed to crates.io with the required bindings.
* A patched eleven secret herbs and spices kernel

## Todo list
- [x] Data structures representing a speaker element
- [x] Parsing machine-specific values from a config file
- [x] Logging
- [x] Mixer control data structures
- [x] Manipulating mixer controls
- [x] Retrieving V/ISENSE values
- [x] Model of voice coil/magnet temperatures
- [x] Ramping volume according to safety model
- [x] Tolerate multiple sample rates
- [ ] Sleep reliably while playback has stopped
- [ ] Daemonise correctly
- [ ] Kernel driver interlock
- [ ] Packaging/distro-agnosticism

## Sundry
The `alsa` crate is Copyright (c) 2015-2021 David Henningsson, and other
contributors. Redistributed under the MIT license.
