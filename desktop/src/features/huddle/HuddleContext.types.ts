import type { AudioInputDevice } from "./lib/useAudioDevices";
import type { VoiceInputMode } from "./lib/useHuddlePttState";

/**
 * High-frequency huddle audio levels, kept out of {@link HuddleContextValue}.
 *
 * The Rust playout loop emits speaker levels every 50 ms and the local mic
 * analyser updates at ~30 Hz. Carrying them on the main huddle context made
 * every `useHuddle()` consumer re-render at that rate; only meters subscribe
 * here.
 */
export interface HuddleLevelsValue {
  /** Local microphone level, normalized to 0–1. */
  micLevel: number;
  /** Pubkeys the backend currently considers to be speaking. */
  activeSpeakers: string[];
  /** Per-participant levels, normalized to 0–1. */
  speakerLevels: Record<string, number>;
}

export interface HuddleContextValue {
  localAudioTrack: MediaStreamTrack | null;
  isStarting: boolean;
  huddleError: string | null;
  clearHuddleError: () => void;
  micConnected: boolean;
  isMuted: boolean;
  toggleMute: () => void;
  /** Interrupt this agent only if it still owns the active utterance. */
  interruptAgentSpeech: (agentPubkey: string) => Promise<void>;
  pttActive: boolean;
  voiceInputMode: VoiceInputMode;
  setVoiceInputMode: (mode: VoiceInputMode) => Promise<void>;
  audioDevices: AudioInputDevice[];
  selectedDeviceId: string;
  setSelectedDeviceId: (id: string) => void;
  micGain: number;
  setMicGain: (value: number) => void;
  outputDevices: { name: string; is_default: boolean }[];
  selectedOutputDevice: string;
  setSelectedOutputDevice: (name: string) => void;
  activeEphemeralChannelId: string | null;
  showHuddleInMainApp: (ephemeralChannelId: string) => void;
  viewHuddleChannel: (ephemeralChannelId: string) => void;
  startHuddle: (
    parentChannelId: string,
    memberPubkeys: string[],
    channelName?: string,
  ) => Promise<void>;
  joinHuddle: (
    parentChannelId: string,
    ephemeralChannelId: string,
    huddleThreadEventId?: string,
  ) => Promise<void>;
  leaveHuddle: () => Promise<boolean>;
}
