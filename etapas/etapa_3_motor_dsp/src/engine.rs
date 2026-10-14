#![allow(dead_code)]

// Importamos macros y tipos de manejo de errores de anyhow.
use anyhow::{anyhow, Context, Result};

// Importamos traits de cpal para interactuar con dispositivos de audio, hosts y flujos.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

// Importamos el tipo Stream que representa el flujo activo de reproducción de audio en el S.O.
use cpal::Stream;

// Importamos primitivas atómicas de sincronización.
use std::sync::atomic::{AtomicBool, Ordering};

// Importamos Arc para compartir referencias inmutables del estado entre hilos.
use std::sync::Arc;

// Importamos Track y AtomicF32 desde el módulo local de pista.
use crate::track::{AtomicF32, Track};

/*
========================================================================================
[ROL DEL BLOQUE DE IMPORTACIONES]:
Trae los enlaces de bajo nivel con el sistema de audio del sistema operativo (CPAL:
ALSA en Linux, CoreAudio en macOS, WASAPI en Windows) y los tipos atómicos necesarios.
========================================================================================
*/

/// Motor de audio en tiempo real de baja latencia.
/// Gestiona la conexión física con la tarjeta de sonido y la mezcla de múltiples pistas.
pub struct AudioEngine {
    // Flujo activo de audio. Mientras este objeto permanezca vivo, el audio se sigue reproduciendo.
    _stream: Stream,

    // Frecuencia de muestreo nativa del hardware (ej. 48000 Hz).
    pub sample_rate: u32,

    // Cantidad de canales físicos de salida (ej. 2 para estéreo).
    pub channels: u16,

    // Lista de pistas activas que se mezclarán en cada ciclo de la tarjeta de sonido.
    pub tracks: Vec<Arc<Track>>,

    // Control de volumen general (Master Volume) en rango [0.0, 2.0].
    pub master_volume: Arc<AtomicF32>,

    // Bandera atómica para pausar o reanudar todo el motor de audio a la vez.
    pub master_paused: Arc<AtomicBool>,

    // Nivel pico reciente del canal izquierdo del bus Master (para alimentar el vúmetro principal).
    pub master_peak_l: Arc<AtomicF32>,

    // Nivel pico reciente del canal derecho del bus Master.
    pub master_peak_r: Arc<AtomicF32>,
}
/*
========================================================================================
[ROL DEL STRUCT AudioEngine]:
Es el corazón del reproductor. Orquesta la lista de pistas, mantiene los controles
generales (Master Volume, Master Pause) y mantiene vivo el hilo de audio del sistema.
========================================================================================
*/

impl AudioEngine {
    /// Inicializa el motor de audio conectándose al dispositivo de salida predeterminado.
    pub fn new(tracks: Vec<Arc<Track>>) -> Result<Self> {
        // Obtenemos el host de audio por defecto del sistema (ej. ALSA en Linux).
        let host = cpal::default_host();

        // Solicitamos el dispositivo de salida de audio físico predeterminado.
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("No se encontró ningún dispositivo de salida de audio en el sistema"))?;

        // Consultamos la configuración de flujo nativa que soporta el hardware.
        let supported_config = device
            .default_output_config()
            .with_context(|| "Error consultando configuración predeterminada del dispositivo")?;

        // Obtenemos la frecuencia de muestreo del hardware en Hz.
        let sample_rate = supported_config.sample_rate().0;

        // Obtenemos la cantidad de canales físicos.
        let channels = supported_config.channels();

        // Validamos que el dispositivo soporte al menos 2 canales estéreo.
        if channels < 2 {
            return Err(anyhow!("Se requiere un dispositivo con al menos 2 canales estéreo (se detectaron {})", channels));
        }

        // Instanciamos los controles maestros atómicos.
        let master_volume = Arc::new(AtomicF32::new(1.0));
        let master_paused = Arc::new(AtomicBool::new(false));
        let master_peak_l = Arc::new(AtomicF32::new(0.0));
        let master_peak_r = Arc::new(AtomicF32::new(0.0));

        // Clonamos los punteros Arc para transferirlos al closure que se ejecutará en el hilo de audio de CPAL.
        let cb_tracks = tracks.clone();
        let cb_master_vol = master_volume.clone();
        let cb_master_paused = master_paused.clone();
        let cb_master_peak_l = master_peak_l.clone();
        let cb_master_peak_r = master_peak_r.clone();

        // Obtenemos la configuración concreta del flujo.
        let stream_config = supported_config.config();

        // Closure para capturar errores asíncronos que pueda emitir el controlador de audio.
        let err_fn = |err| eprintln!("Error en el flujo de audio CPAL: {}", err);

        // Construimos el flujo según el formato de muestra del hardware (f32, i16 o u16).
        let stream = match supported_config.sample_format() {
            // Formato estándar moderno de 32 bits en punto flotante:
            cpal::SampleFormat::F32 => device.build_output_stream(
                &stream_config,
                // Closure invocado por el hardware de sonido periódicamente (Hard Real-Time):
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    process_audio_f32(
                        data,
                        &cb_tracks,
                        &cb_master_vol,
                        &cb_master_paused,
                        &cb_master_peak_l,
                        &cb_master_peak_r,
                    );
                },
                err_fn,
                None,
            )?,
            // Fallback para tarjetas antiguas que usan enteros de 16 bits con signo:
            cpal::SampleFormat::I16 => device.build_output_stream(
                &stream_config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let mut temp = vec![0.0f32; data.len()];
                    process_audio_f32(
                        &mut temp,
                        &cb_tracks,
                        &cb_master_vol,
                        &cb_master_paused,
                        &cb_master_peak_l,
                        &cb_master_peak_r,
                    );
                    for (out, &sample) in data.iter_mut().zip(temp.iter()) {
                        *out = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
                    }
                },
                err_fn,
                None,
            )?,
            // Fallback para dispositivos que usan enteros de 16 bits sin signo:
            cpal::SampleFormat::U16 => device.build_output_stream(
                &stream_config,
                move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                    let mut temp = vec![0.0f32; data.len()];
                    process_audio_f32(
                        &mut temp,
                        &cb_tracks,
                        &cb_master_vol,
                        &cb_master_paused,
                        &cb_master_peak_l,
                        &cb_master_peak_r,
                    );
                    for (out, &sample) in data.iter_mut().zip(temp.iter()) {
                        *out = ((sample.clamp(-1.0, 1.0) * 0.5 + 0.5) * 65535.0) as u16;
                    }
                },
                err_fn,
                None,
            )?,
            _ => return Err(anyhow!("Formato de muestra no soportado por el dispositivo")),
        };

        // Iniciamos la reproducción del flujo de audio.
        stream.play().with_context(|| "No se pudo iniciar el flujo de audio")?;

        // Retornamos la estructura AudioEngine con el flujo corriendo.
        Ok(Self {
            _stream: stream,
            sample_rate,
            channels,
            tracks,
            master_volume,
            master_paused,
            master_peak_l,
            master_peak_r,
        })
    }

    /// Alterna la pausa global del reproductor (Master Pause).
    pub fn toggle_master_pause(&self) {
        let current = self.master_paused.load(Ordering::Relaxed);
        self.master_paused.store(!current, Ordering::Relaxed);
    }

    /// Ajusta el volumen general del reproductor (Master Volume).
    pub fn adjust_master_volume(&self, delta: f32) {
        let cur = self.master_volume.load(Ordering::Relaxed);
        let new_vol = (cur + delta).clamp(0.0, 2.0);
        self.master_volume.store(new_vol, Ordering::Relaxed);
    }

    /// Reinicia la posición de reproducción de todas las pistas al segundo 00:00.
    pub fn restart_all(&self) {
        for track in &self.tracks {
            track.playhead.store(0, Ordering::Relaxed);
        }
    }

    /// Desplaza la posición de reproducción de todas las pistas una cantidad relativa de segundos (+5s o -5s).
    pub fn seek_all_rel(&self, delta_secs: f32) {
        let sr = self.sample_rate as f32;
        let delta_frames = (delta_secs * sr) as isize;

        for track in &self.tracks {
            let cur_frame = track.playhead.load(Ordering::Relaxed);
            let total = track.total_frames.load(Ordering::Relaxed);
            let new_frame = (cur_frame as isize + delta_frames).max(0) as usize;
            let clamped = if total > 0 { new_frame.min(total - 1) } else { 0 };
            track.playhead.store(clamped, Ordering::Relaxed);
        }
    }
}
/*
========================================================================================
[ROL DEL BLOQUE impl AudioEngine]:
Configura el ciclo de vida del dispositivo y proporciona métodos de alto nivel
para pausar todo, modificar el volumen general o rebobinar/adelantar todas las pistas.
========================================================================================
*/

/// Callback de audio de tiempo real estricto (Hard Real-Time):
/// - 0 asignaciones dinámicas de memoria (cero malloc).
/// - 0 bloqueos de sincronización pesados (cero contención).
/// - Suma vectorial estéreo y limitador armónico suave.
#[inline(always)]
fn process_audio_f32(
    output: &mut [f32],
    tracks: &[Arc<Track>],
    master_vol: &AtomicF32,
    master_paused: &AtomicBool,
    master_peak_l: &AtomicF32,
    master_peak_r: &AtomicF32,
) {
    // 1. Llenamos el búfer de salida con ceros (silencio inicial).
    output.fill(0.0);

    // 2. Si el motor está en pausa general, ponemos los vúmetros a 0 y retornamos de inmediato.
    if master_paused.load(Ordering::Relaxed) {
        master_peak_l.store(0.0, Ordering::Relaxed);
        master_peak_r.store(0.0, Ordering::Relaxed);
        return;
    }

    // 3. Verificamos si alguna pista tiene activado el modo 'Solo'.
    let any_solo = tracks.iter().any(|t| t.solo.load(Ordering::Relaxed));

    // La cantidad de cuadros a procesar es la mitad del tamaño del búfer (cada cuadro = L y R).
    let frames = output.len() / 2;

    // 4. Bucle que itera sobre cada pista activa y suma sus muestras al búfer de salida.
    for track in tracks {
        // Si la pista está en pausa individual o silenciada, saltamos su procesamiento.
        if track.paused.load(Ordering::Relaxed) || track.muted.load(Ordering::Relaxed) {
            track.peak_l.store(0.0, Ordering::Relaxed);
            track.peak_r.store(0.0, Ordering::Relaxed);
            continue;
        }

        // Si hay alguna pista en modo 'Solo' y esta NO es una de ellas, la omitimos.
        if any_solo && !track.solo.load(Ordering::Relaxed) {
            track.peak_l.store(0.0, Ordering::Relaxed);
            track.peak_r.store(0.0, Ordering::Relaxed);
            continue;
        }

        // Leemos el volumen atómico de la pista. Si es prácticamente cero, la omitimos para ahorrar CPU.
        let vol = track.volume.load(Ordering::Relaxed);
        if vol <= 0.0001 {
            track.peak_l.store(0.0, Ordering::Relaxed);
            track.peak_r.store(0.0, Ordering::Relaxed);
            continue;
        }

        // Leemos el balance estéreo atómico (-1.0 a 1.0) y calculamos los multiplicadores de canal.
        let pan: f32 = track.pan.load(Ordering::Relaxed);
        let pan_l = (1.0f32 - pan).clamp(0.0f32, 1.0f32) * vol;
        let pan_r = (1.0f32 + pan).clamp(0.0f32, 1.0f32) * vol;

        // Leemos los cuadros totales disponibles en la pista.
        let total_frames = track.total_frames.load(Ordering::Relaxed);
        if total_frames == 0 {
            continue;
        }

        // Leemos si la pista está configurada para reproducirse en bucle.
        let looping = track.looping.load(Ordering::Relaxed);

        // Obtenemos la posición actual de lectura (playhead).
        let mut cur_frame = track.playhead.load(Ordering::Relaxed);

        // Adquirimos el cerrojo de lectura ultrarrápido para acceder a las muestras de la pista.
        let samples_guard = track.samples.read();
        let src_samples = &*samples_guard;
        if src_samples.len() < total_frames * 2 {
            continue;
        }

        let mut track_peak_l = 0.0f32;
        let mut track_peak_r = 0.0f32;

        // --- BUCLE VECTORIAL SIMD DE MEZCLA ---
        for i in 0..frames {
            // Si el cursor alcanza el final de la pista:
            if cur_frame >= total_frames {
                if looping {
                    cur_frame = 0; // Volver al inicio si es bucle
                } else {
                    break; // Detenerse al finalizar
                }
            }

            // Índice dentro del búfer de la pista de origen (intercalado L/R).
            let idx = cur_frame * 2;
            let sl = src_samples[idx] * pan_l;
            let sr = src_samples[idx + 1] * pan_r;

            // Sumamos algebraicamente la muestra a la salida general.
            output[i * 2] += sl;
            output[i * 2 + 1] += sr;

            // Medimos el pico absoluto para el vúmetro de la pista.
            track_peak_l = track_peak_l.max(sl.abs());
            track_peak_r = track_peak_r.max(sr.abs());

            // Avanzamos el cursor de cuadro.
            cur_frame += 1;
        }

        // Actualizamos de forma atómica la posición y los picos medidos de la pista.
        track.playhead.store(cur_frame, Ordering::Relaxed);
        track.peak_l.store(track_peak_l, Ordering::Relaxed);
        track.peak_r.store(track_peak_r, Ordering::Relaxed);
    }

    // 5. Procesamiento del bus Maestro: Aplicación de ganancia Master y Limitador Suave (Soft-Limiter).
    let m_vol = master_vol.load(Ordering::Relaxed);
    let mut max_l = 0.0f32;
    let mut max_r = 0.0f32;

    for i in 0..frames {
        // Multiplicamos por la ganancia maestra.
        let l = output[i * 2] * m_vol;
        let r = output[i * 2 + 1] * m_vol;

        // Pasamos por la curva de saturación suave para evitar distorsión digital (*clipping* cuadrado).
        let limited_l = soft_limit(l);
        let limited_r = soft_limit(r);

        output[i * 2] = limited_l;
        output[i * 2 + 1] = limited_r;

        max_l = max_l.max(limited_l.abs());
        max_r = max_r.max(limited_r.abs());
    }

    // Almacenamos atómicamente los picos del bus maestro para la interfaz de usuario.
    master_peak_l.store(max_l, Ordering::Relaxed);
    master_peak_r.store(max_r, Ordering::Relaxed);
    /*
    [ROL DE process_audio_f32]:
    Realiza la suma matemática simultánea de todas las pistas activas en el búfer
    que la tarjeta de sonido reproducirá por los altavoces en los próximos milisegundos.
    */
}

/// Función matemática de limitación armónica suave (Soft-Saturation Limiter).
/// 
/// Cuando varias pistas suenan a la vez, la suma de amplitudes puede exceder 1.0.
/// En audio digital tradicional, cualquier valor > 1.0 produce un corte plano cuadrado (distorsión desagradable).
/// Esta función aplica una curva polinómica cúbica que comprime suavemente los picos altos simulando
/// la saturación cálida de una cinta analógica o un amplificador a válvulas.
#[inline(always)]
fn soft_limit(x: f32) -> f32 {
    if x > 1.25 {
        1.0
    } else if x < -1.25 {
        -1.0
    } else {
        // Aproximación racional polinómica suave de tanh(x): x / (1 + 0.35 * x^2)
        x / (1.0 + 0.35 * x * x)
    }
    /*
    [ROL DE soft_limit]:
    Garantiza que la señal nunca sature la tarjeta de sonido, manteniendo máxima fidelidad sonora.
    */
}
