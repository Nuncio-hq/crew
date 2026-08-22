//! Tests for huddle/stt.rs — split into a sibling file to keep stt.rs
//! focused. These exercise the pure speech-boundary policy: onset
//! confirmation, pre-roll, offset hysteresis, hangover, flush/reset and the
//! push-to-talk grouping rule.

use super::{
    has_enough_voiced_audio, vad_flush_allowed, SegmentDisposition, VadEndpoint, HANGOVER_FRAMES,
    MIN_VOICED_FRAMES, ONSET_CONFIRM_FRAMES, PRE_ROLL_FRAMES, SILENCE_FLUSH_FRAMES,
    VAD_FRAME_SAMPLES, VAD_OFFSET_THRESHOLD, VAD_ONSET_THRESHOLD,
};

/// A frame whose samples all carry `value`, so tests can identify which
/// frames survived into a segment by inspecting the sample values.
fn frame(value: f32) -> Vec<f32> {
    vec![value; VAD_FRAME_SAMPLES]
}

/// Feed a run of identical frames, returning the last segment produced.
fn push_run(
    endpoint: &mut VadEndpoint,
    prob: f32,
    value: f32,
    count: usize,
) -> Option<super::Segment> {
    let mut last = None;
    for _ in 0..count {
        if let Some(segment) = endpoint.push_frame(prob, &frame(value), true) {
            last = Some(segment);
        }
    }
    last
}

#[test]
fn short_vad_blips_do_not_reach_the_recognizer() {
    assert!(!has_enough_voiced_audio(1));
    assert!(!has_enough_voiced_audio(MIN_VOICED_FRAMES - 1));
    assert!(has_enough_voiced_audio(MIN_VOICED_FRAMES));
}

#[test]
fn vad_flush_allowed_truth_table() {
    // No shortcut configured: VAD pauses always end the utterance.
    assert!(vad_flush_allowed(false, false, false));
    assert!(vad_flush_allowed(false, true, false));
    // Shortcut held: an explicit "I am not done talking", even when the
    // microphone is also manually open.
    assert!(!vad_flush_allowed(true, true, false));
    assert!(!vad_flush_allowed(true, true, true));
    // Shortcut up with a manually open mic: normal VAD pause flushing.
    assert!(vad_flush_allowed(true, false, true));
    // Shortcut up and mic closed: nothing is being accepted anyway.
    assert!(vad_flush_allowed(true, false, false));
}

#[test]
fn onset_requires_consecutive_high_frames() {
    let mut endpoint = VadEndpoint::default();
    for _ in 0..ONSET_CONFIRM_FRAMES * 3 {
        endpoint.push_frame(VAD_ONSET_THRESHOLD + 0.2, &frame(1.0), true);
        endpoint.push_frame(0.0, &frame(0.0), true);
        assert!(
            !endpoint.in_speech(),
            "alternating single voiced frames must not confirm an onset"
        );
    }

    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        1.0,
        ONSET_CONFIRM_FRAMES,
    );
    assert!(
        endpoint.in_speech(),
        "a consecutive voiced run must confirm onset"
    );
}

#[test]
fn confirmed_onset_prepends_pre_roll_once() {
    let mut endpoint = VadEndpoint::default();
    // Well over a pre-roll window of silence: only the newest
    // PRE_ROLL_FRAMES may be retained.
    push_run(&mut endpoint, 0.0, 0.25, PRE_ROLL_FRAMES * 3);
    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        1.0,
        ONSET_CONFIRM_FRAMES,
    );

    let pre_roll_samples = endpoint
        .segment_samples()
        .iter()
        .filter(|&&s| s == 0.25)
        .count();
    assert_eq!(
        pre_roll_samples,
        PRE_ROLL_FRAMES * VAD_FRAME_SAMPLES,
        "confirmed onset must prepend exactly one bounded pre-roll window"
    );
    assert_eq!(
        endpoint.segment_samples().len(),
        (PRE_ROLL_FRAMES + ONSET_CONFIRM_FRAMES) * VAD_FRAME_SAMPLES,
        "the frames that proved the onset must be kept too"
    );
}

#[test]
fn offset_hysteresis_preserves_borderline_speech() {
    let mut endpoint = VadEndpoint::default();
    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        1.0,
        ONSET_CONFIRM_FRAMES,
    );
    let borderline = (VAD_ONSET_THRESHOLD + VAD_OFFSET_THRESHOLD) / 2.0;
    assert!(borderline < VAD_ONSET_THRESHOLD && borderline > VAD_OFFSET_THRESHOLD);

    // Frames below the onset threshold but above the offset threshold are
    // still speech once a segment is open — otherwise every trailing vowel
    // starts a silence countdown and the utterance is cut in half.
    push_run(&mut endpoint, borderline, 0.5, SILENCE_FLUSH_FRAMES * 2);
    assert!(endpoint.in_speech());
    assert_eq!(endpoint.silence_frames(), 0);
}

#[test]
fn below_offset_threshold_starts_silence() {
    let mut endpoint = VadEndpoint::default();
    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        1.0,
        ONSET_CONFIRM_FRAMES,
    );
    push_run(&mut endpoint, VAD_OFFSET_THRESHOLD - 0.1, 0.0, 3);
    assert!(endpoint.in_speech());
    assert_eq!(endpoint.silence_frames(), 3);
}

#[test]
fn silence_flush_retains_only_hangover_audio() {
    let mut endpoint = VadEndpoint::default();
    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        1.0,
        MIN_VOICED_FRAMES,
    );
    let segment = push_run(&mut endpoint, 0.0, -1.0, SILENCE_FLUSH_FRAMES)
        .expect("silence longer than the flush window must end the segment");

    let voiced = segment.samples.iter().filter(|&&s| s == 1.0).count();
    let trailing = segment.samples.iter().filter(|&&s| s == -1.0).count();
    assert_eq!(voiced, MIN_VOICED_FRAMES * VAD_FRAME_SAMPLES);
    assert_eq!(
        trailing,
        HANGOVER_FRAMES * VAD_FRAME_SAMPLES,
        "only the hangover window of trailing silence belongs in the segment"
    );
    assert_eq!(segment.voiced_frames, MIN_VOICED_FRAMES);
}

#[test]
fn flush_boundary_never_double_includes_audio() {
    let mut endpoint = VadEndpoint::default();
    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        1.0,
        MIN_VOICED_FRAMES,
    );
    push_run(&mut endpoint, 0.0, -1.0, SILENCE_FLUSH_FRAMES).expect("first flush");
    assert!(!endpoint.in_speech());
    assert!(endpoint.segment_samples().is_empty());

    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        0.75,
        MIN_VOICED_FRAMES,
    );
    let second = push_run(&mut endpoint, 0.0, -0.5, SILENCE_FLUSH_FRAMES).expect("second flush");
    assert!(
        !second.samples.iter().any(|&s| s == 1.0 || s == -1.0),
        "audio from the first utterance must not leak into the second"
    );
}

#[test]
fn reset_prevents_pre_roll_from_leaking_between_segments() {
    let mut endpoint = VadEndpoint::default();
    push_run(&mut endpoint, 0.0, 0.25, PRE_ROLL_FRAMES);
    // TTS playback / cooldown is a hard boundary: buffered pre-roll is the
    // app's own audio and must never reappear in the next utterance.
    endpoint.reset();
    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        1.0,
        ONSET_CONFIRM_FRAMES,
    );
    assert!(
        !endpoint.segment_samples().contains(&0.25),
        "reset must drop buffered pre-roll audio"
    );
}

#[test]
fn held_push_to_talk_never_silence_flushes() {
    let mut endpoint = VadEndpoint::default();
    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        1.0,
        MIN_VOICED_FRAMES,
    );
    for _ in 0..SILENCE_FLUSH_FRAMES * 4 {
        assert!(
            endpoint.push_frame(0.0, &frame(0.0), false).is_none(),
            "a held shortcut groups the whole hold into one utterance"
        );
    }
    assert!(endpoint.in_speech());

    // Key release flushes explicitly.
    let segment = endpoint.take_segment().expect("release flushes the hold");
    assert_eq!(segment.voiced_frames, MIN_VOICED_FRAMES);
    assert!(!endpoint.in_speech());
}

#[test]
fn short_segment_reaches_the_visible_drop_path() {
    let mut endpoint = VadEndpoint::default();
    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        1.0,
        ONSET_CONFIRM_FRAMES,
    );
    let segment = push_run(&mut endpoint, 0.0, 0.0, SILENCE_FLUSH_FRAMES)
        .expect("even a blip must produce a segment the caller can account for");
    assert!(segment.voiced_frames < MIN_VOICED_FRAMES);
    assert_eq!(
        segment.disposition(),
        SegmentDisposition::DropShort,
        "short segments must be dropped on a visible, logged path"
    );

    let mut endpoint = VadEndpoint::default();
    push_run(
        &mut endpoint,
        VAD_ONSET_THRESHOLD + 0.2,
        1.0,
        MIN_VOICED_FRAMES,
    );
    let segment = push_run(&mut endpoint, 0.0, 0.0, SILENCE_FLUSH_FRAMES).expect("flush");
    assert_eq!(segment.disposition(), SegmentDisposition::Decode);
}
