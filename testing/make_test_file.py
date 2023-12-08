#!/usr/bin/python
import scipy, sys
import numpy as np

ch = int(sys.argv[1])
out = sys.argv[2]

FS = 48000
PILOT_DB = -30
PILOT_FREQ = 43
TEST0 = (500, 1, -10)
TEST1 = (500, 10, -15)
TEST2 = (43, 1.5, -6)
TEST3 = (1000, 1.5, -6)

def db(x):
    return 10 ** (x / 20)

def silence(t):
    return np.zeros(int(FS * t))

def sine(f, t, v):
    space = np.linspace(0, t, int(FS * t), endpoint=False)
    return np.sin(2 * np.pi * f * space) * db(v)

signal = np.concatenate((
    silence(3),
    sine(*TEST0),
    silence(2),
    sine(*TEST1),
    silence(2),
    sine(*TEST2),
    silence(2),
    sine(*TEST3),
    silence(5)
))

space = np.linspace(0, len(signal) / FS, len(signal), endpoint=False)
signal += np.sin(2 * np.pi * PILOT_FREQ * space) * db(PILOT_DB)

signal = np.concatenate((silence(60), signal))

signal = np.repeat(signal, ch).reshape((-1, ch))

scipy.io.wavfile.write(out, FS, signal.astype("float32"))
