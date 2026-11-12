"""Makes Chime, the app's own sound for a chat that needs a look: one soft strike of a felt mallet on a bar, at A4,
ringing briefly into a quiet room. It's synthesized here from sine waves, so it's the project's own, under its license.

    python3 tools/sounds/chime.py apps/desktop/sounds/chime.wav

The same script always makes the same file, so a change to the sound is a change to this script, made again."""

import math
import struct
import sys
import wave

RATE = 44100

# Each partial of the bar, as (times the note, level, how fast it dies away per second): a strong fundamental, its
# overtones nearly four and ten times up, which die fast, as a tuned bar's do, and a soft hum an octave down.
BAR = [(1.0, 1.0, 4.0), (3.93, 0.15, 12.0), (9.6, 0.03, 25.0), (0.5, 0.05, 5.0)]
NOTE_HZ = 440.0
# How long the bar is heard ringing before the room's echo, and how long the echo lasts.
RING_S = 1.2
ROOM_S = 1.0


def strike(freq, seconds, partials, attack, shimmer):
    """A struck bar. Each partial sounds as a pair `shimmer` Hz apart, as a real one does, so it isn't quite pure. The
    mallet is felt, so the strike takes `attack` seconds to reach its loudest."""
    out = []
    for i in range(int(seconds * RATE)):
        t = i / RATE
        s = 0.0
        for ratio, level, decay in partials:
            f = freq * ratio
            pair = math.sin(2 * math.pi * f * t) + math.sin(2 * math.pi * (f + shimmer) * t)
            s += level * math.exp(-decay * t) * pair / 2
        out.append(s * min(1.0, t / attack))
    return out


def lowpass(signal, cutoff):
    k = 1 - math.exp(-2 * math.pi * cutoff / RATE)
    y, out = 0.0, []
    for x in signal:
        y += k * (x - y)
        out.append(y)
    return out


def reverb(signal, wet, seconds, predelay=0.015, damp=3000):
    """Schroeder's: four combs side by side, then two allpasses, for a quiet room."""
    dry = signal + [0.0] * int(seconds * RATE)
    pre = int(predelay * RATE)
    src = [0.0] * pre + dry[: len(dry) - pre]
    combs = [0.0] * len(src)
    for ms in (29.7, 37.1, 41.1, 43.7):
        d = int(ms / 1000 * RATE)
        g = 10 ** (-3 * (ms / 1000) / seconds)
        buf = [0.0] * len(src)
        for i in range(len(src)):
            buf[i] = src[i] + (g * buf[i - d] if i >= d else 0.0)
        combs = [c + b / 4 for c, b in zip(combs, buf)]
    for ms, g in ((5.0, 0.7), (1.7, 0.7)):
        d = int(ms / 1000 * RATE)
        out = [0.0] * len(combs)
        for i in range(len(combs)):
            out[i] = -g * combs[i] + (combs[i - d] if i >= d else 0.0) + g * (out[i - d] if i >= d else 0.0)
        combs = out
    combs = lowpass(combs, damp)
    return [d + wet * c for d, c in zip(dry, combs)]


def finish(signal, peak=0.4, fade=0.15):
    """Well below full scale, so it's never loud, and faded at the end, so it doesn't click."""
    top = max(abs(s) for s in signal) or 1.0
    out = [s / top * peak for s in signal]
    n = int(fade * RATE)
    for i in range(n):
        out[-1 - i] *= i / n
    return out


def write(path, signal):
    with wave.open(path, "wb") as f:
        f.setnchannels(1)
        f.setsampwidth(2)
        f.setframerate(RATE)
        f.writeframes(b"".join(struct.pack("<h", int(max(-1, min(1, s)) * 32767)) for s in signal))


if __name__ == "__main__":
    bar = lowpass(strike(NOTE_HZ, RING_S, BAR, attack=0.008, shimmer=0.4), 3500)
    write(sys.argv[1], finish(reverb(bar, wet=0.3, seconds=ROOM_S)))
