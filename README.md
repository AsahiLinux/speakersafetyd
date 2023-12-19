## speakersafetyd - a software Smart Amp implementation
speakersafetyd is a userspace daemon written in Rust that implements an
analogue of the Texas Instruments Smart Amp speaker protection model.

Apple Silicon Macs mostly use the Texas Instruments TAS2764 amp chip (codec
in ALSA parlance), which provides sense lines for the voltage and current across
the voice coil of the connected speaker. These codecs are designed to be used
in embedded applications where device firmware takes this information and uses
it to protect the speaker from damage. Apple instead implement this as machine-specific
plugins to the userspace half of CoreAudio. An increasing number of other
vendors in both the desktop and embedded/Android worlds are choosing to go down a similar
route, folding this functionality into proprietary driver/userspace blobs that usually
also bundle niceties like EQ (we have a solution for this too, see [asahi-audio](https://github.com/AsahiLinux/asahi-audio)).
This puts users at serious risk of permanently destroying their expensive devices if they choose
to run custom software, such as Asahi Linux or an Open Source Android ROM.

speakersafetyd is the first (as far as we know) FOSS implementation of a speaker
protection model. It solves the problem described above by allowing parties interested
in compatible devices to quickly and easily implement a speaker protection model for those
devices. Only Apple Silicon Macs under Linux are currently supported,
however the model applies to all loudspeakers. The daemon itself should be easy enough to
adapt for any device that provides V/ISENSE data in a manner similar to TAS2764.

### Dependencies
* Rust stable
* alsa-lib
* An Apple Silicon Mac running Asahi Linux

### Some background on Smart Amps
The cheap component speaker elements used in modern devices like
Bluetooth speakers, TVs, laptops, etc. are very fragile. In order
to eke the highest possible sound quality out of them, they need to be
driven *hard*. This leaves us with a dilemma - how do we drive these
speakers hard enoguh to get a loud, high-quality output but not hard
enough to destroy them?

A speaker's electromechanical characteristics can be modelled
and boiled down to a set of parameters - the Thiele/Small Parameters.
These can be used to predict what the speaker will do with certain
inputs. When we add measured properties like the time constant of
the speaker's voice coil's and magnet's temperature curve, we can
accurately model a speaker's temperature for any given voltage/current
across the coil. When the speaker is getting too hot, we just reduce the
power going to it until it cools down.

This lets us fearlessly drive the speakers as hard as they physically can
be without being permanently damaged. This is extremely useful, as without it
the output level on these devices would have to be hard limited to a very low
level that is known to be safe for the worst possible input. Instead, we can
simply duck the output in those cases and allow the speakers to oeprate at
full power where possible.

Many integrated amplifier chips implement this functionality in hardware, as well
as additional advanced DSP features like compressors and limiters. Texas Instruments
call their implementation "Smart Amp." Integrators need only communicate the parameter set
to the chip for the speaker it is connected to, and it does the rest. Many do not however,
and instead only provide facilities for measuring the voltage and current across the speaker's
voice coil. It is up to the implementer to capture this data and do something with it.

speakersafetyd is (as far as we know) the first and only FOSS implementation of the
Smart Amp protection model.
