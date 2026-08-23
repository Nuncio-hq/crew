import * as React from "react";

/**
 * Local microphone level analyser for the huddle voice-activity meters.
 *
 * Extracted from the huddle provider body so the ~30 Hz level state has one
 * owner and the provider keeps its lifecycle logic readable. The returned
 * value is normalized to 0–1 and is 0 whenever the gate considers the mic
 * silent, so meters settle instead of jittering on room noise.
 */

const MIC_ANALYSER_UPDATE_INTERVAL_MS = 33;
const MIC_INITIAL_NOISE_FLOOR = 0.01;
const MIC_VOICE_GATE_ON_RMS = 0.018;
const MIC_VOICE_GATE_OFF_RMS = 0.012;
const MIC_VOICE_GATE_MARGIN_RMS = 0.012;
const MIC_LEVEL_ACTIVE_RANGE_RMS = 0.11;
const MIC_MIN_ACTIVE_LEVEL = 0.18;
const MIC_LEVEL_ATTACK = 0.58;
const MIC_ACTIVE_NOISE_FLOOR_RISE = 0.006;

function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value));
}

/** Normalized 0–1 level of `localAudioTrack`, or 0 when no mic is connected. */
export function useMicLevelAnalyser(
  localAudioTrack: MediaStreamTrack | null,
  micConnected: boolean,
): number {
  const [micLevel, setMicLevel] = React.useState(0);

  React.useEffect(() => {
    if (!localAudioTrack || !micConnected) {
      setMicLevel(0);
      return;
    }

    const ctx = new AudioContext();
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 512;
    const source = ctx.createMediaStreamSource(
      new MediaStream([localAudioTrack]),
    );
    source.connect(analyser);
    const buf = new Float32Array(analyser.fftSize);

    let raf = 0;
    let lastUpdate = 0;
    let voiceActive = false;
    let noiseFloor = MIC_INITIAL_NOISE_FLOOR;
    let smoothedLevel = 0;
    function tick(now: number) {
      raf = requestAnimationFrame(tick);
      if (now - lastUpdate < MIC_ANALYSER_UPDATE_INTERVAL_MS) return;
      lastUpdate = now;
      analyser.getFloatTimeDomainData(buf);

      let sumSquares = 0;
      for (let i = 0; i < buf.length; i += 1) {
        sumSquares += buf[i] * buf[i];
      }

      const rms = Math.sqrt(sumSquares / buf.length);
      const activeThreshold = Math.max(
        MIC_VOICE_GATE_ON_RMS,
        noiseFloor + MIC_VOICE_GATE_MARGIN_RMS,
      );
      const idleThreshold = Math.max(
        MIC_VOICE_GATE_OFF_RMS,
        noiseFloor + MIC_VOICE_GATE_MARGIN_RMS * 0.55,
      );
      voiceActive = voiceActive ? rms > idleThreshold : rms > activeThreshold;

      const floorRate =
        rms < noiseFloor
          ? 0.18
          : voiceActive
            ? MIC_ACTIVE_NOISE_FLOOR_RISE
            : 0.025;
      noiseFloor += (rms - noiseFloor) * floorRate;

      if (!voiceActive) {
        smoothedLevel = 0;
        setMicLevel(0);
        return;
      }

      const normalized = clamp01(
        (rms - noiseFloor) / MIC_LEVEL_ACTIVE_RANGE_RMS,
      );
      const targetLevel = Math.max(normalized, MIC_MIN_ACTIVE_LEVEL);
      smoothedLevel += (targetLevel - smoothedLevel) * MIC_LEVEL_ATTACK;
      setMicLevel(smoothedLevel);
    }
    raf = requestAnimationFrame(tick);

    return () => {
      cancelAnimationFrame(raf);
      source.disconnect();
      void ctx.close();
    };
  }, [localAudioTrack, micConnected]);

  return micLevel;
}
