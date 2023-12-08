import json, sys, os.path, configparser
import numpy as np
import matplotlib.pyplot as plt
from scipy.signal import butter, sosfilt, freqz

CONFDIR = os.path.join(os.path.dirname(__file__), "../conf")

# This information is not in the blackbox file
DEFAULT_AMP_GAIN = 15.50
AMP_GAIN = {
    "apple,j180": 13.0,
    "apple,j313": 16.0,
    "apple,j274": 18.0,
    "apple,j375": 18.0,
    "apple,j473": 18.0,
    "apple,j474": 18.0,
    "apple,j475": 18.0,
}

def db(x):
    return 10 ** (x / 20)

def smooth(a, n=3):
    l = len(a)
    ret = np.cumsum(a, dtype=float)
    ret[n:] = ret[n:] - ret[:-n]
    ret = ret[n - 1:] / n
    pad = l - len(ret)
    return np.pad(ret, (pad//2, (pad + 1)//2), "edge")

def butter_lowpass(cutoff, fs, order=5):
    return butter(order, cutoff, fs=fs, btype='low', output="sos", analog=False)

def butter_highpass(cutoff, fs, order=5):
    return butter(order, cutoff, fs=fs, btype='high', output="sos", analog=False)

def butter_lowpass_filter(data, cutoff, fs, order=5):
    sos = butter_lowpass(cutoff, fs, order=order)
    y = sosfilt(sos, data)
    return y

def butter_highpass_filter(data, cutoff, fs, order=5):
    sos = butter_highpass(cutoff, fs, order=order)
    y = sosfilt(sos, data)
    return y

def pilot_filter(data, fs):
    data = butter_lowpass_filter(data, 100, fs, 6)
    return butter_highpass_filter(data, 10, fs, 3)

class Model:
    def __init__(self, idx, an, name, conf):
        self.idx = idx
        self.an = an
        self.name = name
        self.conf = conf
        self.tr_coil = float(conf["tr_coil"])
        self.tr_magnet = float(conf["tr_magnet"])
        self.tau_coil = float(conf["tau_coil"])
        self.tau_magnet = float(conf["tau_magnet"])
        self.t_limit = float(conf["t_limit"])
        self.t_headroom = float(conf["t_headroom"])
        self.z_nominal = float(conf["z_nominal"])
        self.z_shunt = float(conf.get("z_shunt", 0))
        self.is_scale = float(conf["is_scale"])
        self.vs_scale = float(conf["vs_scale"])
        self.a_t_20c = float(conf["a_t_20c"])
        self.a_t_35c = float(conf["a_t_35c"])

        self.is_chan = int(conf["is_chan"])
        self.vs_chan = int(conf["vs_chan"])

        self.t_ambient = an.fdr["t_ambient"]

        self.t_coil = an.fdr["blocks"][0]["speakers"][self.idx]["t_coil"]
        self.t_magnet = an.fdr["blocks"][0]["speakers"][self.idx]["t_magnet"]

        self.m_x = []
        self.m_t_coil_tg = []
        self.m_t_coil = []
        self.m_t_magnet_tg = []
        self.m_t_magnet = []

        self.l_x = []
        self.l_t_coil = []
        self.l_t_magnet = []

    def run_model(self):
        off = 0
        t = 0
        for blk in self.an.fdr["blocks"]:
            sr = blk["sample_rate"]
            cnt = blk["sample_count"]
            data = blk["speakers"][self.idx]

            isense = self.an.cvr[off:off+cnt, self.is_chan] * self.is_scale
            vsense = self.an.cvr[off:off+cnt, self.vs_chan] * self.vs_scale

            dt = 1 / self.an.sr
            alpha_coil = dt / (dt + self.tau_coil)
            alpha_magnet = dt / (dt + self.tau_magnet)

            self.l_x.append(t)
            self.l_t_coil.append(data["t_coil"])
            self.l_t_magnet.append(data["t_magnet"])
            for x, (i, v) in enumerate(zip(isense, vsense)):
                self.m_x.append(t + x / sr)

                p = i * v

                tvc_tgt = self.t_magnet + p * self.tr_coil
                self.t_coil = tvc_tgt * alpha_coil + self.t_coil * (1 - alpha_coil)
                tmag_tgt = self.t_ambient + p * self.tr_magnet
                self.t_magnet = tmag_tgt * alpha_magnet + self.t_magnet * (1 - alpha_magnet)

                self.m_t_coil_tg.append(tvc_tgt)
                self.m_t_coil.append(self.t_coil)
                self.m_t_magnet_tg.append(tmag_tgt)
                self.m_t_magnet.append(self.t_magnet)

            t += cnt / sr
            off += cnt

    def analyze(self, outfile):
        plt.clf()

        fig, ax1 = plt.subplots(figsize=(30,15))

        ax1.set_title(self.name)

        ax1.set_xlabel('time (s)')
        ax1.set_ylabel('temperature')
        ax1.tick_params(axis='y')
        ax2 = ax1.twinx()  # instantiate a second axes that shares the same x-axis

        color = 'tab:red'
        ax2.set_ylabel('power', color=color)
        ax2.tick_params(axis='y', labelcolor=color)

        ax1.plot(self.m_x, self.m_t_coil, "r")
        # ax1.plot(self.m_x, smooth(self.m_t_coil_tg, 1000), "y")
        ax1.plot(self.m_x, self.m_t_magnet, "b")
        # ax1.plot(self.m_x, smooth(self.m_t_magnet_tg, 1000), "c")
        ax1.plot(self.l_x, self.l_t_coil, "om")
        ax1.plot(self.l_x, self.l_t_magnet, "og")

        i = self.an.cvr[:, self.is_chan] * self.is_scale
        v = self.an.cvr[:, self.vs_chan] * self.vs_scale

        sr = self.an.fdr["sample_rate"]
        ilp = pilot_filter(i, sr)
        vlp = pilot_filter(v, sr)

        p = butter_lowpass_filter(i * v, 10, sr, 1)
        p = smooth(p, 4000)
        plp = butter_lowpass_filter(ilp * vlp, 10, sr, 1)
        plp = smooth(plp, 4000)
        vlprms_sq = butter_lowpass_filter(vlp * vlp, 10, sr, 1)
        vlprms_sq = smooth(vlprms_sq, 4000)
        r = vlprms_sq / plp

        # ax2.plot(self.m_x, p, "b")

        rref = np.average(r[1 * sr:2 * sr])

        # Clear out the first second, since it tends to contain garbage
        r[:1*sr] = rref
        #r = butter_lowpass_filter(r - rref, 2, sr, 2) + rref
        a = self.a_t_35c # XXX why are there two values at different temperatures?
        tref = self.l_t_coil[0]
        t = ((r - self.z_shunt) / (rref - self.z_shunt) - 1) / a + tref

        ax1.plot(self.m_x, t, "k")
        ax2.plot(self.m_x, p, "r")
        # ax2.plot(self.m_x, plp, "g")
        # ax2.plot(self.m_x, vlprms_sq, "b")

        for level in (-1000, -6, -10, -15):
            gain = AMP_GAIN.get(self.an.fdr["machine"], DEFAULT_AMP_GAIN)
            pbase = (db(gain - 30) ** 2) / (self.z_nominal + self.z_shunt)
            ptest = (db(gain + level) ** 2) / (self.z_nominal + self.z_shunt)
            p = pbase + ptest
            ax2.axhline(y=p, color='r', linestyle='--')

        fig.tight_layout()  # otherwise the right y-label is slightly clipped

        plt.savefig(outfile)


class Analyzer:
    def __init__(self, base):
        self.fdr = json.load(open(base + ".fdr"))
        data = open(base + ".cvr", "rb").read()
        cvr = np.frombuffer(data, dtype="int16").astype("float") / 32768

        maker, model = self.fdr["machine"].split(",")
        cf = os.path.join(CONFDIR, maker, model + ".conf")
        print(f"Using config file: {cf}")
        self.conf = configparser.ConfigParser()
        self.conf.read(cf)

        ch = int(self.conf["Globals"]["channels"])
        samples = len(cvr) // ch
        self.cvr = cvr.reshape((samples, ch))
        print(f"Got {samples} samples ({ch} channels)")

        assert ch == self.fdr["channels"]
        self.sr = self.fdr["sample_rate"]

        speaker_configs = []

        for key in self.conf.sections():
            if not key.startswith("Speaker/"):
                continue
            print(key)
            name = key.split("/")[1]
            speaker_configs.append((name, self.conf[key]))

        # Match the order that speakersafetyd uses (by group)
        speaker_configs.sort(key=lambda x: int(x[1]["group"]))

        self.speakers = []
        for i, cfg in enumerate(speaker_configs):
            self.speakers.append(Model(i, self, cfg[0], cfg[1]))

    def analyze(self):
        for i, spk in enumerate(self.speakers):
            print(f"Processing speaker {i}")
            spk.run_model()
            spk.analyze(f"speaker_{i}.png")
            # break

if __name__ == "__main__":
    a = Analyzer(sys.argv[1])
    a.analyze()
