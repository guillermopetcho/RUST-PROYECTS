#![allow(dead_code)]
use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::{SampleBuffer, SignalSpec};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decodifica cualquier archivo de audio soportado (.mp3, .mp4, .m4a, .wav, .flac, .ogg)
/// a un vector plano de punto flotante en estéreo intercalado [L, R, L, R, ...],
/// aplicando remuestreo lineal si la frecuencia original no coincide con target_sample_rate.
pub fn load_and_decode_file<P: AsRef<Path>>(
    path: P,
    target_sample_rate: u32,
) -> Result<(Vec<f32>, u32)> {
    let path = path.as_ref();
    let file = File::open(path)
        .with_context(|| format!("No se pudo abrir el archivo: {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let metadata_opts = MetadataOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .with_context(|| format!("Formato no reconocido o archivo dañado: {}", path.display()))?;

    let mut format = probed.format;

    // Buscar la pista de audio principal (saltando flujos de video si es un MP4)
    let (track_id, src_sample_rate, mut decoder) = {
        let mut found = None;
        for t in format.tracks() {
            if t.codec_params.codec == CODEC_TYPE_NULL {
                continue;
            }
            if let Some(sr) = t.codec_params.sample_rate {
                if let Ok(dec) = symphonia::default::get_codecs()
                    .make(&t.codec_params, &DecoderOptions::default())
                {
                    found = Some((t.id, sr, dec));
                    break;
                }
            }
        }
        found.ok_or_else(|| {
            anyhow!(
                "No se encontró ninguna pista de audio reproducible en {}",
                path.display()
            )
        })?
    };

    let mut raw_stereo_samples: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(_) => {
                break;
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let channels = decoded.spec().channels.count();

                if sample_buf.is_none() {
                    let spec = SignalSpec::new(
                        decoded.capacity() as u32,
                        decoded.spec().channels,
                    );
                    sample_buf = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
                }

                if let Some(buf) = sample_buf.as_mut() {
                    buf.copy_interleaved_ref(decoded);
                    let samples = buf.samples();

                    if channels == 1 {
                        // Mono -> Duplicar a Estéreo
                        for &s in samples {
                            raw_stereo_samples.push(s);
                            raw_stereo_samples.push(s);
                        }
                    } else if channels == 2 {
                        raw_stereo_samples.extend_from_slice(samples);
                    } else {
                        // Multicanal (>2 canales): mezclar los primeros 2
                        for chunk in samples.chunks_exact(channels) {
                            raw_stereo_samples.push(chunk[0]);
                            raw_stereo_samples.push(chunk[1]);
                        }
                    }
                }
            }
            Err(Error::DecodeError(_)) => {
                // Saltar paquete dañado individual sin abortar
                continue;
            }
            Err(Error::IoError(_)) => break,
            Err(e) => return Err(e.into()),
        }
    }

    if raw_stereo_samples.is_empty() {
        return Err(anyhow!("El archivo decodificó 0 muestras: {}", path.display()));
    }

    // Remuestrear si la frecuencia de origen no coincide con la de destino
    if src_sample_rate != target_sample_rate && target_sample_rate > 0 {
        let resampled = resample_stereo_linear(&raw_stereo_samples, src_sample_rate, target_sample_rate);
        Ok((resampled, target_sample_rate))
    } else {
        Ok((raw_stereo_samples, src_sample_rate))
    }
}

/// Remuestreador por interpolación lineal de alta eficiencia para PCM estéreo intercalado
pub fn resample_stereo_linear(
    input: &[f32],
    src_rate: u32,
    dst_rate: u32,
) -> Vec<f32> {
    if src_rate == dst_rate || input.is_empty() {
        return input.to_vec();
    }

    let src_frames = input.len() / 2;
    let ratio = src_rate as f64 / dst_rate as f64;
    let dst_frames = ((src_frames as f64) / ratio).round() as usize;

    let mut output = Vec::with_capacity(dst_frames * 2);

    for out_frame in 0..dst_frames {
        let src_pos = out_frame as f64 * ratio;
        let idx0 = src_pos.floor() as usize;
        let frac = (src_pos - idx0 as f64) as f32;

        if idx0 + 1 < src_frames {
            let l0 = input[idx0 * 2];
            let r0 = input[idx0 * 2 + 1];
            let l1 = input[(idx0 + 1) * 2];
            let r1 = input[(idx0 + 1) * 2 + 1];

            output.push(l0 + frac * (l1 - l0));
            output.push(r0 + frac * (r1 - r0));
        } else if idx0 < src_frames {
            output.push(input[idx0 * 2]);
            output.push(input[idx0 * 2 + 1]);
        } else {
            output.push(0.0);
            output.push(0.0);
        }
    }

    output
}
