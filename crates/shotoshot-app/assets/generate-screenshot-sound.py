"""Rebuild Shotoshot's original procedural camera shutter using only Python stdlib.

No recording, reference waveform, or third-party asset is read. A dry impact,
brief curtain friction, and softer return impact make a compact mechanical cue.
Inharmonic resonances decay within milliseconds: there is no notification tone.
Run this file to regenerate screenshot.wav beside it; generation is offline.
"""

import math
from pathlib import Path
import random
import struct
import wave


SAMPLE_RATE = 44_100
DURATION = 0.210
PEAK = 0.48  # Headroom for an unobtrusive feedback sound (-6.4 dBFS).
SEED = 0x53484F54
# (onset seconds, gain, noise decay seconds): release, latch, curtain return.
IMPACTS = ((0.0, 1.0, 0.009), (0.013, 0.27, 0.006), (0.083, 0.63, 0.013))
# (frequency Hz, gain, decay seconds): deliberately unrelated cavity modes.
MODES = ((367, 0.40, 0.007), (943, 0.24, 0.005), (2189, 0.12, 0.003))
FRICTION_START = 0.012
FRICTION_DURATION = 0.170
FRICTION_GAIN = 0.16


def synthesize() -> list[int]:
    """Return deterministic PCM16 samples, with a smooth attack and tail."""
    rng = random.Random(SEED)
    samples = []
    low = 0.0
    body = 0.0
    for index in range(round(SAMPLE_RATE * DURATION)):
        time = index / SAMPLE_RATE
        noise = rng.uniform(-1.0, 1.0)
        # Two simple low-pass poles produce a dry, band-limited material sound.
        low += 0.53 * (noise - low)
        body += 0.075 * (low - body)
        texture = low - body
        value = 0.0
        for onset, gain, decay in IMPACTS:
            age = time - onset
            if age < 0:
                continue
            attack = 1.0 - math.exp(-age / 0.00025)
            impact = 1.25 * texture * math.exp(-age / decay)
            for frequency, level, damping in MODES:
                impact += level * math.sin(math.tau * frequency * age) * math.exp(-age / damping)
            value += gain * attack * impact
        travel = (time - FRICTION_START) / FRICTION_DURATION
        if 0.0 < travel < 1.0:
            # A shallow friction bed joins the impacts without a sustained pitch.
            envelope = math.sin(math.pi * travel) ** 1.4
            value += FRICTION_GAIN * texture * envelope * math.exp(-2.5 * travel)
        # Fade the final 12 ms without appending silent frames.
        value *= min(1.0, max(0.0, (DURATION - time) / 0.012))
        samples.append(value)
    scale = PEAK * 32767 / max(abs(value) for value in samples)
    pcm = [round(value * scale) for value in samples]
    # Quantization can turn the inaudible tail into zeros; omit that padding.
    while pcm and pcm[-1] == 0:
        pcm.pop()
    return pcm


def main() -> None:
    samples = synthesize()
    output = Path(__file__).with_name("screenshot.wav")
    with wave.open(str(output), "wb") as stream:
        stream.setnchannels(1)
        stream.setsampwidth(2)
        stream.setframerate(SAMPLE_RATE)
        stream.writeframes(struct.pack(f"<{len(samples)}h", *samples))
    peak = max(abs(sample) for sample in samples) / 32768
    rms = math.sqrt(sum(sample * sample for sample in samples) / len(samples)) / 32768
    print(f"{output.name}: PCM16 mono {SAMPLE_RATE} Hz, {len(samples) / SAMPLE_RATE:.3f} s, "
          f"peak {20 * math.log10(peak):.2f} dBFS, RMS {20 * math.log10(rms):.2f} dBFS")


if __name__ == "__main__":
    main()
