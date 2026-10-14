// Importamos tipos para el manejo de errores desde la biblioteca 'anyhow'.
// 'anyhow' simplifica el manejo de errores permitiendo usar Result con cualquier tipo de error.
use anyhow::{anyhow, Context, Result};

// Importamos 'File' de la biblioteca estándar para interactuar con archivos en el disco.
use std::fs::File;

// Importamos 'Path' para representar rutas en el sistema de archivos de manera independiente del S.O.
use std::path::Path;

// Importamos estructuras de Symphonia para gestionar búferes de audio y especificaciones de señal.
use symphonia::core::audio::{SampleBuffer, SignalSpec};

// Importamos opciones del decodificador y la constante CODEC_TYPE_NULL para identificar códecs nulos o no soportados.
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};

// Importamos tipos de error propios del motor Symphonia.
use symphonia::core::errors::Error;

// Importamos las opciones de configuración del formato (demuxer).
use symphonia::core::formats::FormatOptions;

// Importamos MediaSourceStream, que adapta cualquier fuente que implemente Read (como File) para Symphonia.
use symphonia::core::io::MediaSourceStream;

// Importamos opciones de metadatos para controlar la lectura de etiquetas ID3, metadatos Vorbis, etc.
use symphonia::core::meta::MetadataOptions;

// Importamos Hint, que permite darle pistas al demuxer (por ejemplo, la extensión del archivo).
use symphonia::core::probe::Hint;

/*
========================================================================================
[ROL DEL BLOQUE DE IMPORTACIONES]:
Declara todas las dependencias necesarias de la biblioteca estándar y de Symphonia.
Su función es traer al ámbito (scope) las estructuras que permiten abrir archivos,
identificar su contenedor multimedia, extraer los flujos de audio y convertir muestras a f32.
========================================================================================
*/

/// Carga y decodifica un archivo de audio (.mp3, .mp4, .wav, .flac, .ogg, etc.) a memoria.
/// 
/// Parámetros:
/// - `path`: Cualquier tipo que implemente `AsRef<Path>` (como &str, String o PathBuf).
/// - `target_sample_rate`: Frecuencia de muestreo destino del hardware (ej. 44100 o 48000 Hz).
///
/// Retorna:
/// - `Result<(Vec<f32>, u32)>`: Tupla con el vector de muestras intercaladas estéreo y la frecuencia final.
pub fn load_and_decode_file<P: AsRef<Path>>(
    path: P,
    target_sample_rate: u32,
) -> Result<(Vec<f32>, u32)> {
    // Convertimos el parámetro genérico 'path' a una referencia a 'Path' prestada.
    let path = path.as_ref();

    // Abrimos el archivo en modo solo lectura desde el sistema de archivos.
    // Usamos .with_context para adjuntar un mensaje de error legible si el archivo no existe.
    let file = File::open(path)
        .with_context(|| format!("No se pudo abrir el archivo de audio: {}", path.display()))?;

    // Envolvemos el archivo en un Box en el Heap y creamos un MediaSourceStream compatible con Symphonia.
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // Creamos un objeto 'Hint' para proporcionar pistas al motor de detección de formato.
    let mut hint = Hint::new();

    // Si el archivo tiene extensión (.mp3, .mp4, etc.), la registramos en la pista para acelerar el probing.
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    // Configuramos las opciones de lectura de formato habilitando reproducción sin pausas (gapless).
    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };

    // Configuramos las opciones de metadatos con sus valores por defecto.
    let metadata_opts = MetadataOptions::default();

    // Usamos el probe predeterminado de Symphonia para inspeccionar los bytes del archivo y determinar el formato.
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .with_context(|| format!("Formato no reconocido o archivo dañado: {}", path.display()))?;

    // Obtenemos el lector de formato (demuxer) del archivo ya identificado.
    let mut format = probed.format;

    // Buscamos la pista de audio principal dentro del contenedor multimedia.
    // Esto es crucial para archivos MP4 o MKV, ya que pueden contener pistas de video que debemos ignorar.
    let (track_id, src_sample_rate, mut decoder) = {
        // Variable opcional que almacenará la tupla (ID de pista, frecuencia de muestreo, decodificador instanciado).
        let mut found = None;

        // Iteramos sobre todas las pistas que contiene el archivo (video, audio, subtítulos, etc.).
        for t in format.tracks() {
            // Si el códec es nulo o desconocido, lo ignoramos y pasamos a la siguiente pista.
            if t.codec_params.codec == CODEC_TYPE_NULL {
                continue;
            }

            // Verificamos si la pista tiene una frecuencia de muestreo (lo cual confirma que es audio).
            if let Some(sr) = t.codec_params.sample_rate {
                // Intentamos crear un decodificador para los parámetros de este códec (ej. AAC, MP3, FLAC).
                if let Ok(dec) = symphonia::default::get_codecs()
                    .make(&t.codec_params, &DecoderOptions::default())
                {
                    // Si se creó con éxito, guardamos la pista encontrada y salimos del bucle.
                    found = Some((t.id, sr, dec));
                    break;
                }
            }
        }

        // Si ninguna pista era de audio soportado, retornamos un error descriptivo con anyhow.
        found.ok_or_else(|| {
            anyhow!(
                "No se encontró ninguna pista de audio reproducible en {} (se inspeccionaron {} flujos)",
                path.display(),
                format.tracks().len()
            )
        })?
    };
    /*
    [ROL DEL BLOQUE DE INICIALIZACIÓN]:
    Abre el archivo físico, identifica el códec apropiado (saltándose flujos de video en MP4)
    y crea la instancia del decodificador que convertirá los paquetes binarios en ondas PCM.
    */

    // Vector dinámico donde acumularemos todas las muestras de audio en punto flotante estéreo [L, R, L, R...].
    let mut raw_stereo_samples: Vec<f32> = Vec::new();

    // Búfer reutilizable de Symphonia para evitar realocar memoria en cada paquete decodificado.
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    // Bucle principal de decodificación: se ejecuta hasta agotar los paquetes del archivo.
    loop {
        // Leemos el siguiente paquete de datos comprimidos del archivo.
        let packet = match format.next_packet() {
            // Paquete obtenido correctamente.
            Ok(packet) => packet,
            // Si alcanzamos el fin de archivo esperado (EOF), salimos limpiamente del bucle.
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            // Si el demuxer requiere reiniciar el estado del decodificador, lo reiniciamos y continuamos.
            Err(Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            // Cualquier otro fin de flujo o error menor termina la lectura.
            Err(_) => {
                break;
            }
        };

        // Si el paquete pertenece a otra pista (por ejemplo, video en un MP4), lo ignoramos.
        if packet.track_id() != track_id {
            continue;
        }

        // Decodificamos el paquete de audio comprimido a muestras en crudo (PCM).
        match decoder.decode(&packet) {
            Ok(decoded) => {
                // Obtenemos el número de canales del audio decodificado (1 para mono, 2 para estéreo, etc.).
                let channels = decoded.spec().channels.count();

                // Si el búfer de muestras aún no ha sido creado, lo instanciamos con la capacidad del paquete.
                if sample_buf.is_none() {
                    let spec = SignalSpec::new(
                        decoded.capacity() as u32,
                        decoded.spec().channels,
                    );
                    sample_buf = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
                }

                // Usamos el búfer para convertir cualquier formato (i16, i24, i32, u8) a punto flotante f32.
                if let Some(buf) = sample_buf.as_mut() {
                    // Copiamos y convertimos las muestras al formato intercalado en el búfer.
                    buf.copy_interleaved_ref(decoded);
                    // Obtenemos un slice prestado (&[f32]) de las muestras convertidas.
                    let samples = buf.samples();

                    if channels == 1 {
                        // Si el audio original es mono, duplicamos cada muestra para crear un canal estéreo [L, R].
                        for &s in samples {
                            raw_stereo_samples.push(s);
                            raw_stereo_samples.push(s);
                        }
                    } else if channels == 2 {
                        // Si ya es estéreo nativo, añadimos las muestras directamente al vector de salida.
                        raw_stereo_samples.extend_from_slice(samples);
                    } else {
                        // Si tiene más de 2 canales (ej. 5.1 envolvente), tomamos los dos primeros canales frontales.
                        for chunk in samples.chunks_exact(channels) {
                            raw_stereo_samples.push(chunk[0]);
                            raw_stereo_samples.push(chunk[1]);
                        }
                    }
                }
            }
            // Si ocurre un error de corrupción en un cuadro individual, lo saltamos para no interrumpir la reproducción.
            Err(Error::DecodeError(_)) => {
                continue;
            }
            // Error de E/S: terminamos la decodificación.
            Err(Error::IoError(_)) => break,
            // Cualquier otro error crítico se propaga al llamador.
            Err(e) => return Err(e.into()),
        }
    }
    /*
    [ROL DEL BUCLE DE DECODIFICACIÓN]:
    Consume los paquetes binarios del contenedor demuxer, los pasa por el descompresor
    correspondiente (AAC/MP3/PCM) y acumula las muestras normalizadas en memoria como estéreo f32.
    */

    // Verificamos que el archivo haya generado al menos una muestra de audio válida.
    if raw_stereo_samples.is_empty() {
        return Err(anyhow!("El archivo decodificó 0 muestras: {}", path.display()));
    }

    // Si la frecuencia del archivo (ej. 44100 Hz) no coincide con la del dispositivo de sonido (ej. 48000 Hz),
    // aplicamos el remuestreo lineal matemático en este momento de carga.
    if src_sample_rate != target_sample_rate && target_sample_rate > 0 {
        // Remuestreamos todo el vector a la frecuencia requerida por el hardware.
        let resampled = resample_stereo_linear(&raw_stereo_samples, src_sample_rate, target_sample_rate);
        // Retornamos el vector remuestreado junto con la frecuencia destino.
        Ok((resampled, target_sample_rate))
    } else {
        // Si las frecuencias ya coinciden, retornamos el vector directamente sin procesado extra.
        Ok((raw_stereo_samples, src_sample_rate))
    }
    /*
    [ROL FINAL DE load_and_decode_file]:
    Garantiza que el resto del motor de audio reciba datos consistentes, 100% compatibles
    con el hardware de salida, eliminando la necesidad de remuestrear en el hilo de reproducción.
    */
}

/// Remuestreador por interpolación lineal de alta eficiencia para PCM estéreo intercalado [L, R, L, R...].
/// 
/// Parámetros:
/// - `input`: Slice prestado con las muestras originales en estéreo.
/// - `src_rate`: Frecuencia de muestreo original (ej. 44100).
/// - `dst_rate`: Frecuencia de muestreo destino (ej. 48000).
///
/// Retorna:
/// - `Vec<f32>`: Nuevo vector con las muestras interpoladas exactamente a la frecuencia destino.
pub fn resample_stereo_linear(
    input: &[f32],
    src_rate: u32,
    dst_rate: u32,
) -> Vec<f32> {
    // Si las frecuencias son iguales o el vector de entrada está vacío, clonamos y salimos inmediatamente.
    if src_rate == dst_rate || input.is_empty() {
        return input.to_vec();
    }

    // Cada cuadro (frame) estéreo consta de 2 muestras: Canal Izquierdo y Canal Derecho.
    let src_frames = input.len() / 2;

    // Calculamos el factor de conversión (ratio) entre ambas frecuencias.
    let ratio = src_rate as f64 / dst_rate as f64;

    // Calculamos la cantidad total de cuadros destino que debemos generar.
    let dst_frames = ((src_frames as f64) / ratio).round() as usize;

    // Pre-alocamos la memoria exacta necesaria en el vector de salida para evitar realocaciones dinámicas.
    let mut output = Vec::with_capacity(dst_frames * 2);

    // Iteramos cuadro por cuadro para generar la señal en la nueva frecuencia.
    for out_frame in 0..dst_frames {
        // Calculamos la posición flotante equivalente en el arreglo de origen.
        let src_pos = out_frame as f64 * ratio;

        // Obtenemos el índice entero anterior en el arreglo de origen.
        let idx0 = src_pos.floor() as usize;

        // Obtenemos la parte fraccionaria (frac) entre 0.0 y 1.0 para ponderar la interpolación.
        let frac = (src_pos - idx0 as f64) as f32;

        // Si tenemos un cuadro siguiente disponible para interpolar entre dos puntos conocidos:
        if idx0 + 1 < src_frames {
            // Muestra izquierda en el punto 0.
            let l0 = input[idx0 * 2];
            // Muestra derecha en el punto 0.
            let r0 = input[idx0 * 2 + 1];

            // Muestra izquierda en el punto 1 siguiente.
            let l1 = input[(idx0 + 1) * 2];
            // Muestra derecha en el punto 1 siguiente.
            let r1 = input[(idx0 + 1) * 2 + 1];

            // Fórmula de interpolación lineal: punto0 + fracción * (punto1 - punto0).
            output.push(l0 + frac * (l1 - l0));
            output.push(r0 + frac * (r1 - r0));
        } else if idx0 < src_frames {
            // Si estamos exactamente en el último cuadro disponible, tomamos ese valor directamente.
            output.push(input[idx0 * 2]);
            output.push(input[idx0 * 2 + 1]);
        } else {
            // Relleno de seguridad con silencio en caso de desbordamiento de índice.
            output.push(0.0);
            output.push(0.0);
        }
    }

    // Retornamos el vector con el audio remuestreado a la frecuencia exacta.
    output
    /*
    [ROL DEL REMUESTREADOR LINEAL]:
    Ajusta la velocidad y el tono de la pista a la frecuencia de reloj del hardware.
    Sin esta función, una canción de 44.1 kHz reproducida en una tarjeta de 48 kHz
    sonaría acelerada y con tono más agudo (efecto ardilla).
    */
}
