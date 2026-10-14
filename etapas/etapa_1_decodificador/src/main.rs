// Declaramos el submódulo 'decoder' definido en decoder.rs para tener acceso a sus funciones públicas.
mod decoder;

// Importamos 'env' para acceder a los argumentos de línea de comandos pasados por el usuario.
use std::env;

// Importamos 'File' para poder crear archivos en disco.
use std::fs::File;

// Importamos el trait 'Write' para habilitar el método .write_all() sobre archivos y flujos de bytes.
use std::io::Write;

/*
========================================================================================
[ROL DEL BLOQUE DE DECLARACIONES]:
Vincula el módulo de decodificación con este ejecutable principal y trae las herramientas
básicas de entrada/salida necesarias para generar archivos de prueba e interactuar con la CLI.
========================================================================================
*/

/// Función principal de la Etapa 1.
/// Demuestra la decodificación de audio, el cálculo de metadatos y el remuestreo de frecuencias.
fn main() -> anyhow::Result<()> {
    // Imprimimos el encabezado explicativo de la etapa en la consola.
    println!("==================================================================");
    println!("   ETAPA 1: FUNDAMENTOS DE RUST Y DECODIFICACIÓN DE AUDIO         ");
    println!("==================================================================");

    // Obtenemos los argumentos de la línea de comandos omitiendo el primer elemento (que es el nombre del binario).
    let args: Vec<String> = env::args().skip(1).collect();

    // Definimos una frecuencia de destino estándar de hardware de estudio: 48.000 Hz.
    let target_sample_rate = 48000;

    // Si el usuario pasó una ruta en la terminal, la usamos; de lo contrario, creamos un archivo WAV sintético.
    let file_path = if let Some(path) = args.first() {
        // Clonamos la ruta proporcionada por el usuario.
        path.clone()
    } else {
        // Ruta temporal para generar un archivo WAV de demostración.
        let temp_wav = "/tmp/etapa1_demo.wav";
        println!("No se especificó un archivo. Creando audio de prueba: {}", temp_wav);

        // Llamamos a nuestra función generadora creando un tono estéreo de 44.1 kHz durante 1.5 segundos.
        create_test_wav(temp_wav, 44100, 1.5)?;

        // Retornamos la ruta como String.
        temp_wav.to_string()
    };
    /*
    [ROL DEL BLOQUE DE ENTRADA]:
    Gestiona la entrada del usuario con fallback automático a un archivo sintético.
    Garantiza que la etapa siempre sea ejecutable y didáctica, incluso sin archivos externos.
    */

    // Notificamos el inicio del proceso de decodificación.
    println!("\n[1] Decodificando archivo: {}", file_path);
    println!("    Frecuencia objetivo para el hardware: {} Hz", target_sample_rate);

    // Llamamos a la función decodificadora central que implementamos en decoder.rs.
    let (samples, sr) = decoder::load_and_decode_file(&file_path, target_sample_rate)?;

    // Calculamos el número de cuadros (frames). Al ser estéreo intercalado [L, R], cada cuadro son 2 floats.
    let frames = samples.len() / 2;

    // Calculamos la duración exacta en segundos dividiendo la cantidad de cuadros entre la frecuencia de muestreo.
    let duration = frames as f32 / sr as f32;

    // Calculamos el pico máximo de amplitud absoluta recorriendo el vector con un iterador funcional (fold).
    let peak = samples.iter().copied().fold(0.0f32, |max, s| max.max(s.abs()));
    /*
    [ROL DEL BLOQUE DE ANÁLISIS]:
    Aplica conceptos funcionales de Rust (iteradores, fold, closures) para inspeccionar
    las características acústicas de las muestras en memoria sin bucles mutables tradicionales.
    */

    // Imprimimos el reporte final con los datos calculados.
    println!("\n[2] Resultados del análisis de audio:");
    println!("    * Muestras estéreo decodificadas: {}", samples.len());
    println!("    * Cuadros (frames): {}", frames);
    println!("    * Frecuencia final: {} Hz", sr);
    println!("    * Duración calculada: {:.2} segundos", duration);
    println!("    * Amplitud pico: {:.4} (normalizado en [-1.0, 1.0])", peak);

    println!("\n✅ ¡Etapa 1 completada con éxito!");
    println!("   El decodificador lee binarios en MP3/MP4/WAV y los remuestrea de forma segura.");

    // Limpiamos el archivo temporal generado para mantener el sistema limpio.
    if file_path == "/tmp/etapa1_demo.wav" {
        let _ = std::fs::remove_file(&file_path);
    }

    // Retornamos Ok(()) indicando salida exitosa con código de salida 0.
    Ok(())
}

/// Genera un archivo WAV válido en formato PCM de 16 bits estéreo con cabecera estándar RIFF.
///
/// Parámetros:
/// - `path`: Ruta del archivo de destino.
/// - `sample_rate`: Frecuencia de muestreo (ej. 44100 Hz).
/// - `duration_secs`: Duración en segundos.
fn create_test_wav(path: &str, sample_rate: u32, duration_secs: f32) -> anyhow::Result<()> {
    // Calculamos el total de muestras a generar en función de la frecuencia y duración.
    let num_samples = (sample_rate as f32 * duration_secs) as usize;

    // Tasa de bytes por segundo: frecuencia * canales (2) * bytes por muestra (2 bytes para 16 bits).
    let byte_rate = sample_rate * 2 * 2;

    // Longitud total en bytes de los datos de audio en el subchunk 'data'.
    let data_len = (num_samples * 4) as u32;

    // Creamos el archivo físico en disco.
    let mut file = File::create(path)?;

    // --- CABECERA RIFF ---
    // Identificador de formato 'RIFF'.
    file.write_all(b"RIFF")?;
    // Tamaño total del archivo menos los primeros 8 bytes de cabecera.
    file.write_all(&(36 + data_len).to_le_bytes())?;
    // Tipo de formato 'WAVE'.
    file.write_all(b"WAVE")?;

    // --- SUBCHUNK 'fmt ' (Especificaciones de formato) ---
    // Identificador de subchunk de formato 'fmt '.
    file.write_all(b"fmt ")?;
    // Tamaño del subchunk fmt (16 bytes para formato PCM estándar).
    file.write_all(&16u32.to_le_bytes())?;
    // Formato de audio (1 indica PCM sin compresión).
    file.write_all(&1u16.to_le_bytes())?;
    // Número de canales (2 indica Estéreo).
    file.write_all(&2u16.to_le_bytes())?;
    // Frecuencia de muestreo en Hz (ej. 44100).
    file.write_all(&sample_rate.to_le_bytes())?;
    // Byte rate (tasa de transferencia requerida).
    file.write_all(&byte_rate.to_le_bytes())?;
    // Block align (canales * bytes por muestra = 4 bytes por frame).
    file.write_all(&4u16.to_le_bytes())?;
    // Bits por muestra (16 bits).
    file.write_all(&16u16.to_le_bytes())?;

    // --- SUBCHUNK 'data' (Muestras de audio) ---
    // Identificador de datos 'data'.
    file.write_all(b"data")?;
    // Tamaño total en bytes de los datos de audio.
    file.write_all(&data_len.to_le_bytes())?;

    // Generamos matemáticamente una onda senoidal estéreo:
    // Canal Izquierdo: 440 Hz (Nota Musical La)
    // Canal Derecho: 880 Hz (Una octava más arriba)
    for i in 0..num_samples {
        // Tiempo en segundos transcurrido para la muestra actual.
        let t = i as f32 / sample_rate as f32;

        // Senoide izquierda a 440 Hz multiplicada por 0.5 de amplitud para no saturar.
        let s_l = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;

        // Senoide derecha a 880 Hz multiplicada por 0.5 de amplitud.
        let s_r = (2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.5;

        // Convertimos el flotante [-1.0, 1.0] a un entero con signo de 16 bits [-32768, 32767].
        file.write_all(&((s_l * 32767.0) as i16).to_le_bytes())?;
        file.write_all(&((s_r * 32767.0) as i16).to_le_bytes())?;
    }

    // Confirmamos éxito retornando Ok(()).
    Ok(())
    /*
    [ROL DE create_test_wav]:
    Construye desde cero un archivo WAV válido byte por byte cumpliendo el estándar RIFF/WAVE.
    Demuestra la manipulación de bytes y endianness (to_le_bytes) en Rust.
    */
}
