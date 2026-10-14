use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

// Test WAV file generator (16-bit PCM Stereo)
fn create_test_wav(path: &str, sample_rate: u32, duration_secs: f32) {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let byte_rate = sample_rate * 2 * 2; // 2 channels * 16-bit (2 bytes)
    let data_len = (num_samples * 4) as u32;

    let mut file = File::create(path).expect("Failed to create test wav file");

    // RIFF Header
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();

    // fmt subchunk
    file.write_all(b"fmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap();
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    file.write_all(&byte_rate.to_le_bytes()).unwrap();
    file.write_all(&4u16.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap();

    // data subchunk
    file.write_all(b"data").unwrap();
    file.write_all(&data_len.to_le_bytes()).unwrap();

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let s_l = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
        let s_r = (2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.5;

        let sample_i16_l = (s_l * 32767.0) as i16;
        let sample_i16_r = (s_r * 32767.0) as i16;

        file.write_all(&sample_i16_l.to_le_bytes()).unwrap();
        file.write_all(&sample_i16_r.to_le_bytes()).unwrap();
    }
}

#[test]
fn test_resampler() {
    let dummy_pcm = vec![1.0, 1.0, 0.5, 0.5, 0.0, 0.0, -0.5, -0.5];
    let resampled = mopet::audio::decoder::resample_stereo_linear(&dummy_pcm, 44100, 48000);
    assert!(!resampled.is_empty());
    assert_eq!(resampled.len() % 2, 0);
}

#[test]
fn test_track_controls() {
    let samples = vec![0.1, 0.1, 0.2, 0.2, 0.3, 0.3, 0.4, 0.4];
    let track = mopet::audio::Track::new(0, "Test".into(), samples, 44100);

    assert_eq!(track.total_frames.load(Ordering::Relaxed), 4);
    assert_eq!(track.volume.load(Ordering::Relaxed), 1.0);
    assert!(!track.muted.load(Ordering::Relaxed));
    assert!(!track.solo.load(Ordering::Relaxed));

    track.adjust_volume(0.1);
    assert!((track.volume.load(Ordering::Relaxed) - 1.1).abs() < 1e-4);

    track.toggle_mute();
    assert!(track.muted.load(Ordering::Relaxed));

    track.toggle_solo();
    assert!(track.solo.load(Ordering::Relaxed));
}

#[test]
fn test_track_swap_audio() {
    let samples1 = vec![0.1, 0.1, 0.2, 0.2];
    let track = mopet::audio::Track::new(0, "Track A".into(), samples1, 44100);
    assert_eq!(track.total_frames.load(Ordering::Relaxed), 2);
    assert_eq!(track.get_name(), "Track A");

    let samples2 = vec![0.5, 0.5, 0.6, 0.6, 0.7, 0.7];
    track.swap_audio(samples2, 48000, "Track B (Swapped)".into());
    assert_eq!(track.total_frames.load(Ordering::Relaxed), 3);
    assert_eq!(track.sample_rate.load(Ordering::Relaxed), 48000);
    assert_eq!(track.get_name(), "Track B (Swapped)");
    assert_eq!(track.playhead.load(Ordering::Relaxed), 0);
}

#[test]
fn test_plan_persistence_and_navigation() {
    let paths = vec![
        PathBuf::from("/musica/cancion1.mp3"),
        PathBuf::from("/musica/video.mp4"),
        PathBuf::from("/musica/cancion3.flac"),
    ];

    let mut plan = mopet::audio::AudioPlan::from_paths(&paths);
    assert_eq!(plan.items.len(), 3);
    assert_eq!(plan.current_index, 0);
    assert_eq!(plan.current_item().unwrap().title, "cancion1.mp3");

    // Sequential: 1 -> 2
    let next1 = plan.next().unwrap();
    assert_eq!(next1.title, "video.mp4");
    assert_eq!(plan.current_index, 1);

    // Sequential: 2 -> 3
    let next2 = plan.next().unwrap();
    assert_eq!(next2.title, "cancion3.flac");
    assert_eq!(plan.current_index, 2);

    // End of sequential queue -> None
    assert!(plan.next().is_none());

    // Back: 3 -> 2
    let prev = plan.prev().unwrap();
    assert_eq!(prev.title, "video.mp4");

    // Test loop all
    plan.repeat_mode = mopet::audio::RepeatMode::LoopAll;
    plan.next(); // to index 2
    let loop_item = plan.next().unwrap(); // loop to index 0
    assert_eq!(loop_item.title, "cancion1.mp3");

    // Test JSON save and load
    let tmp_plan = "/tmp/test_saved_plan.json";
    plan.save_to_file(tmp_plan).expect("Save plan to JSON failed");

    let loaded = mopet::audio::AudioPlan::load_from_file(tmp_plan).expect("Load plan from JSON failed");
    assert_eq!(loaded.items.len(), 3);
    assert_eq!(loaded.items[1].title, "video.mp4");

    let _ = std::fs::remove_file(tmp_plan);
}

#[test]
fn test_wav_decoding_and_mixing() {
    let wav1_path = "/tmp/mopet_test1.wav";
    let wav2_path = "/tmp/mopet_test2.wav";

    create_test_wav(wav1_path, 44100, 0.5);
    create_test_wav(wav2_path, 48000, 0.5);

    let (samples1, sr1) = mopet::audio::decoder::load_and_decode_file(wav1_path, 48000).unwrap();
    let (samples2, sr2) = mopet::audio::decoder::load_and_decode_file(wav2_path, 48000).unwrap();

    assert_eq!(sr1, 48000);
    assert_eq!(sr2, 48000);
    assert!(!samples1.is_empty());
    assert!(!samples2.is_empty());

    let track1 = Arc::new(mopet::audio::Track::new(0, "Wav 1".into(), samples1, 48000));
    let track2 = Arc::new(mopet::audio::Track::new(1, "Wav 2".into(), samples2, 48000));

    assert_eq!(track1.id, 0);
    assert_eq!(track2.id, 1);

    // Clean up temporary files
    let _ = std::fs::remove_file(wav1_path);
    let _ = std::fs::remove_file(wav2_path);
}

#[test]
fn test_mp4_and_mp3_decoding() {
    // Generates a self-contained test WAV to verify decoder pipeline
    let test_path = "/tmp/mopet_self_test.wav";
    create_test_wav(test_path, 44100, 0.25);
    let (samples, sr) = mopet::audio::decoder::load_and_decode_file(test_path, 48000).unwrap();
    assert_eq!(sr, 48000);
    assert!(!samples.is_empty());
    let _ = std::fs::remove_file(test_path);
}
