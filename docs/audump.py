import struct, sys, os.path, plistlib, pprint

LABELS = {
    "spp3": {
        0: {
            0: "thermal protection enabled",
            1: "displacement protection enabled",
            2: "thermal/power control gain attack time (s)",
            3: "thermal/power control gain release time (s)",
            4: "ambient temperature",
            5: "SafeTlim",
            6: "SafeTlimTimeMin",
            7: "SafeTlimOffset",
            8: "LookaheadDelay_ms",
            9: "peak attack time (s)",
            10: "peak decay time (s)",
            11: "feedback integration time",
            12: "thermal gain (dB)",
            13: "displacement gain (dB)",
            14: "spk pwr averaging window time (s)",
            15: "modeled speaker power",
            16: "measured speaker power",
            17: "power control gain",
            18: "CPMS power control enabled",
            19: "CPMS power control closed loop",
        },
        4: {
            0: "temperature limit",
            1: "hard temp limit headroom",
            2: "T_sett_vc",
            3: "T_sett_mg",
            4: "tau_Tvc",
            5: "tau_Tmg",
            6: "ThermalFFSpeedupFactor",
            7: "temperature",
            8: "OL temperature",
            9: "Reb_ref",
            10: "Rshunt",
            11: "Rampout",
            12: "mt",
            13: "ct",
            14: "kt",
            15: "ag",
            16: "g_bw",
            17: "Q_d",
            18: "phi",
            19: "x_lim",
            20: "ThermalMeasurementMethod",
            21: "pilot tone enabled",
            22: "CL thermal feedback enabled",
            23: "TlimErrDecayTime",
            24: "TempSenseWindowTime",
            25: "TempSenseSmoothingTau",
            26: "a_t_inv",
            27: "PilotAmplHi_dB",
            28: "PilotAmplLo_dB",
            29: "PilotUpperThres",
            30: "PilotLowerThres",
            31: "PilotDecayTime",
            32: "PilotFreq",
            33: "LPMLSPreGain",
            34: "LPMLSPostGain",
            35: "LPMLSLowerCorner",
            36: "LPMLS pre clip level",
            37: "mu_Re",
            38: "mu_Le",
            39: "mu mechanical (PU)",
            40: "Max relative displacement",
            41: "abs(Min relative displacement)",
            42: "DisplacementProtectionType",
            64: "thermal gain",
            65: "displacement gain",
            66: "power control gain",
            67: "PilotDecayTimeStage2",
            68: "PilotEnableThres",
        },
    },
    "atsp": {
        0: {
            0: "Bypass",
            40: "Gain link all audio channels",
            1: "speakerType A: Amplifier sensitivity [V/Fs]",
            2: "speakerType A: VoiceCoil: DC resistance [Ohms]",
            3: "speakerType A: VoiceCoil: thermal resistance [C/Watt]",
            4: "speakerType A: Voice Coil: thermal time constant [s]",
            5: "speakerType A: Magnet: thermal resistance  [C/Watt]",
            6: "speakerType A: Magnet: thermal time constant [s]",
            7: "speakerType A: Ambient temperature, [C]",
            # The target temperature of the speakers
            8: "speakerType A: Temperature limit [C]",
            9: "speakerType A: Attack time (ms)",
            10: "speakerType A: Release time (ms)",
            11: "speakerType A: Temperature hard limit headroom [C]",
            12: "speakerType A: Gain link",
            13: "speakerType A: Audio channel assignment",
            14: "speakerType B: Amplifier sensitivity [V/Fs",
            15: "speakerType B: VoiceCoil: DC resistance [Ohms]",
            16: "speakerType B: VoiceCoil: thermal resistance [C/Watt]",
            17: "speakerType B: Voice Coil: thermal time constant [s]",
            18: "speakerType B: Magnet: thermal resistance  [C/Watt]",
            19: "speakerType B: Magnet: thermal time constant [s]",
            20: "speakerType B: Ambient temperature, [C]",
            21: "speakerType B: Temperature limit [C]",
            22: "speakerType B: Attack time (ms)",
            23: "speakerType B: Release time (ms)",
            24: "speakerType B: Temperature hard limit headroom [C]",
            25: "speakerType B: Gain link",
            26: "speakerType B: Audio channel assignment",
            27: "speakerType C: Amplifier sensitivity [V/Fs]",
            28: "speakerType C: VoiceCoil: DC resistance [Ohms]",
            29: "speakerType C: VoiceCoil: thermal resistance [C/Watt]",
            30: "speakerType C: Voice Coil: thermal time constant [s]",
            31: "speakerType C: Magnet: thermal resistance  [C/Watt]",
            32: "speakerType C: Magnet: thermal time constant [s]",
            33: "speakerType C: Ambient temperature, [C]",
            34: "speakerType C: Temperature limit [C]",
            35: "speakerType C: Attack time (ms)",
            36: "speakerType C: Release time (ms)",
            37: "speakerType C: Temperature hard limit headroom [C]",
            38: "speakerType C: Gain link",
            39: "speakerType C: Audio channel assignment",
        }
    }
}
        
"""
ATSP protection behavior:

Max gain reduction is 20dB.
"Temperature limit" is the target temperature
If temperature exceeds limit + "Temperature hard limit headroom",
protection goes into panic mode and triggers 20dB reduction.

For settings:
    amp = 12 r = 4
    rVc = 50 aVc = 2 rMg = 1 aMg = 1
    Ta = 50 Tlim = 150 Theadroom = 5

We see this limiter behavior:
In      Out
0       -9.7
-8      -9.7
-9      -9.6
-9.5    -9.7
-9.8    -9.9
-10     -10

In other words, it behaves like a hard limit / compressor with infinite ratio.

Theadroom has no influence on the gain reduction, it just affects stability
(temperature does exceed Tlim transiently, if the transient is > Theadroom
 it panics). Too low a Theadroom leads to unstable behavior.
"""


def dump_audata(labels, data):
    top = {}
    while data:
        hdr = data[:0xc]
        data = data[0xc:]
        typ, grp, cnt = struct.unpack(">III", hdr)
        d = {}
        for i in range(cnt):
            blk = data[:0x8]
            data = data[0x8:]
            key, val = struct.unpack(">If", blk)
            if typ in labels:
                if key in labels[typ]:
                    key = labels[typ][key]
            d[key] = val
        top[(typ, grp)] = d
    pprint.pprint(top, stream=sys.stderr)
    return top

def process_spp3(e):
    # Grab the plist file, which is mostly redundant but contains
    # some details not in the au preset
    for i in prop["Boxes"]:
        if i["Name"] == e["displayname"]:
            for p in i["Properties"]:
                if p["Number"] == 64003:
                    path = os.path.join(base, "DSP", p["Path"].split("/DSP/")[1])

    pl = plistlib.load(open(path, "rb"))

    d = dump_audata(LABELS["spp3"], e["aupreset"]["data"])
    spkrs = ""
    channels = 0
    gbl = d[(0, 0)]
    for (typ, ch), p in sorted(d.items()):
        if typ != 4:
            continue
        chp = pl["ChannelSpecificParams"][f"Channel{ch}"]
        channels += 2
        spkrs += f"""

[Speaker/{chp["SpeakerName"]}]
group = {chp["SpeakerGroup"]}
tr_coil = {p["T_sett_vc"]:.2f}
tr_magnet = {p["T_sett_mg"]:.2f}
tau_coil = {p["tau_Tvc"]:.2f}
tau_magnet = {p["tau_Tmg"]:.2f}
t_limit = {p["temperature limit"]:.1f}
t_headroom = {p["hard temp limit headroom"]:.1f}
z_nominal = {p["Reb_ref"]:.2f}
z_shunt = {p["Rshunt"]:.2f}
is_scale = 3.75
vs_scale = 14
is_chan = {2 * ch}
vs_chan = {2 * ch + 1}"""

    print(f"""\
[Globals]
visense_pcm = 2
t_ambient = {gbl["ambient temperature"]}
t_hysteresis = 5.0
t_window = 20.0
channels = {channels}
period = 4096
link_gains = True

[Controls]
vsense = VSENSE Switch
isense = ISENSE Switch
amp_gain = Amp Gain Volume
volume = Speaker Volume{spkrs}""")

def process_atsp(e):
    # print(e)
    d = dump_audata(LABELS["atsp"], e["aupreset"]["data"])[(0,0)]
    t_ambient = None
    
    spkrs = ""
    channels = 0
    for gid, gn in enumerate("ABC"):
        p = f"speakerType {gn}: "
        ch = int(d[p + "Audio channel assignment"])
        if not ch:
            continue
        if ch == 0xffff:
            ch = 1
        
        ambient = d[p + "Ambient temperature, [C]"]
        assert t_ambient is None or t_ambient == ambient
        t_ambient = ambient

        for i in range(16):
            if ch & (1 << i):
                channels += 2
                spkrs += f"""
                
[Speaker/{gn}_ch{i}]
group = {gid}
tr_coil = {d[p + "VoiceCoil: thermal resistance [C/Watt]"]:.2f}
tr_magnet = {d[p + "Magnet: thermal resistance  [C/Watt]"]:.2f}
tau_coil = {d[p + "Voice Coil: thermal time constant [s]"]:.2f}
tau_magnet = {d[p + "Magnet: thermal time constant [s]"]:.2f}
t_limit = {d[p + "Temperature limit [C]"]:.1f}
t_headroom = {d[p + "Temperature hard limit headroom [C]"]:.1f}
z_nominal = {d[p + "VoiceCoil: DC resistance [Ohms]"]:.2f}
is_scale = 3.75
vs_scale = 14
is_chan = {2 * i}
vs_chan = {2 * i + 1}"""

    print(f"""\
[Globals]
visense_pcm = 2
t_ambient = {t_ambient}
t_hysteresis = 5.0
t_window = 20.0
channels = {channels}
period = 4096
link_gains = {bool(d["Gain link all audio channels"])}

[Controls]
vsense = VSENSE Switch
isense = ISENSE Switch
amp_gain = Amp Gain Volume
volume = Speaker Volume{spkrs}""")

if __name__ == "__main__":
    base = sys.argv[1]

    au = plistlib.load(open(os.path.join(base, "DSP/Strips/builtin_speaker_out_general.austrip"), "rb"))
    try:
        prop = plistlib.load(open(os.path.join(base, "DSP/Strips/builtin_speaker_out_general.propstrip"), "rb"))
    except:
        prop = None

    for s in au["strips"]:
        for e in s["effects"]:
            if e["unit"]["subtype"].to_bytes(4) == b"spp3":
                process_spp3(e)
            if e["unit"]["subtype"].to_bytes(4) == b"atsp":
                process_atsp(e)

