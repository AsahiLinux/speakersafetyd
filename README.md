## Asahi Linux speaker safety daemon

This is still very much a work in progress, is probably not "proper" Rust,
and almost definitely makes competent developers extremely sad.

We currently rely on a local version of the `alsa` crate, pending the merge of
bindings to `snd_ctl_elem_value_{read,write}` and `snd_ctl_elem_set_id`.

## What works
* Parsing config file
* All borrows seem to work fine
* Volume getting/setting

## Needs improvement
* Probably everything

## Need to implement
* Daemonise and loop
* Threading (should probably make sure it works as intended first)
* Getting V/ISENSE (pending changes to the codec drivers, we have mock implementations)
* Actually fail safe (see below)

## On failing safe
We need a way to guarantee safety on _any_ fail condition. The TAS codecs have a safe
mode which cuts all outputs down by 18 dB. This works out to being about half their
full output capabiltiy. It might be worth having the `macaudio` driver start them
explicitly in this mode, and only unlock full output capability with an IOCTL that
can be sent by `speakersafetyd` when it's sure it has started correctly. We would
then of course also need an IOCTL to do the opposite if we encounter a runtime error.

It was suggested by someone on IRC that this would be conducive to some sort of
keepalive IOCTL, where the driver would automatically put the codecs into safe mode
if it didn't hear from us for a while. This seems like it would suck to implement.

Like any SLA, it is likely that we will never be able to guarantee 100% safety for all
nonstandard setups. The reference PipeWire DSP graph plus this should be enough for 99% of
users, but I feel at some point those who insist on using Pulse or raw ALSA are just going
to have to put up with a best effort service and accept the (small) risk of this failing.

## Sundry
The `alsa` crate is Copyright (c) 2015-2021 David Henningsson, and other
contributors. Redistributed under the MIT license.
