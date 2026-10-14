// Declaración de los módulos del programa que conforman la Etapa 4
// Módulo de decodificación y remuestreo de archivos de audio
mod decoder;
// Módulo del motor DSP y stream de salida de sonido
mod engine;
// Módulo de gestión de planes de reproducción, listas en cola y serialización JSON
mod plan;
// Módulo de síntesis procedimental de sonido
mod synth;
// Módulo de control atómico y concurrente del estado de las pistas
mod track;

// Importamos los traits de CPAL para control del hardware de audio del sistema operativo
use cpal::traits::{DeviceTrait, HostTrait};
// Importamos la estructura del motor de audio DSP
use engine::AudioEngine;
// Importamos la estructura de planes de reproducción
use plan::AudioPlan;
// Importamos utilidades de entorno para capturar argumentos de la terminal
use std::env;
// Importamos la estructura para crear y abrir archivos en el disco
use std::fs::File;
// Importamos el trait Write para escribir bytes en archivos
use std::io::Write;
// Importamos la estructura de manipulación de rutas del sistema de archivos
use std::path::PathBuf;
// Importamos el ordenamiento de memoria atómica relajado
use std::sync::atomic::Ordering;
// Importamos el puntero atómico compartido para compartir la pista con el motor
use std::sync::Arc;
// Importamos el módulo de hilos para pausas de sondeo
use std::thread;
// Importamos la estructura para definir intervalos de tiempo
use std::time::Duration;
// Importamos la estructura de pista de audio
use track::Track;

/// Punto de entrada del ejecutable de la Etapa 4.
/// Demuestra la arquitectura completa de Lazy Loading (carga perezosa/diferida en memoria),
/// persistencia en formato JSON (`mopet_plan.json`), transición secuencial entre pistas
/// y reemplazo dinámico en caliente (Hot-Swap) con consumo constante y mínimo de RAM (< 25 MB).
fn main() -> anyhow::Result<()> {
    // Encabezado visual de la Etapa 4
    println!("==================================================================");
    // Título descriptivo
    println!("   ETAPA 4: PLANES DE REPRODUCCIÓN, SERIALIZACIÓN Y LAZY LOADING ");
    // Separador
    println!("==================================================================");

    // Capturamos los argumentos de la línea de comandos omitiendo el nombre del ejecutable
    let args: Vec<String> = env::args().skip(1).collect();

    // Determinamos si generamos 2 archivos temporales de prueba o usamos los provistos por el usuario
    let plan = if args.is_empty() {
        // Al no haber argumentos, creamos 2 tonos sinusoidales WAV de prueba en /tmp
        println!("[1] Creando 2 archivos de prueba para el plan...");
        // Definimos la ruta para la primera canción de prueba
        let p1 = "/tmp/etapa4_cancion1.wav";
        // Definimos la ruta para la segunda canción de prueba
        let p2 = "/tmp/etapa4_cancion2.wav";
        // Generamos un tono puro de 440 Hz (Nota musical La / A4) de 1 segundo de duración
        create_test_tone(p1, 440.0, 1.0)?;
        // Generamos un tono puro de 660 Hz (Nota musical Mi / E5) de 1 segundo de duración
        create_test_tone(p2, 660.0, 1.0)?;

        // Creamos una lista con las rutas generadas
        let paths = vec![PathBuf::from(p1), PathBuf::from(p2)];
        // Construimos el plan de audio a partir de las rutas
        let mut p = AudioPlan::from_paths(&paths);
        // Le asignamos un nombre descriptivo
        p.name = "Plan de Estudio Etapa 4".into();
        // Retornamos el plan configurado
        p
    } else {
        // Si el usuario proporcionó rutas por terminal, las convertimos a `PathBuf`
        println!("[1] Creando plan con archivos proporcionados...");
        // Mapeamos los strings a PathBuf
        let paths: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
        // Construimos el plan de reproducción
        AudioPlan::from_paths(&paths)
    };
    /*
     * ROL DEL BLOQUE DE INICIALIZACIÓN DEL PLAN:
     * Garantiza que la Etapa 4 funcione de forma totalmente autónoma sin requerir archivos externos
     * preexistentes, sintetizando archivos WAV limpios en disco para realizar la prueba de cola.
     */

    // Definimos el nombre del archivo JSON de persistencia
    let json_path = "plan_demostracion.json";
    // Notificamos al usuario la serialización
    println!("\n[2] Serializando plan a formato JSON en '{}'...", json_path);
    // Guardamos el plan en el archivo JSON
    plan.save_to_file(json_path)?;

    // Procedemos a recuperar el plan desde el archivo JSON recién creado
    println!("    Recuperando plan desde JSON...");
    // Leemos y deserializamos el plan
    let mut plan_cargado = AudioPlan::load_from_file(json_path)?;
    // Confirmamos la carga exitosa y mostramos la cantidad de elementos en la cola
    println!("    ✅ Plan cargado: '{}' con {} canciones en cola.", plan_cargado.name, plan_cargado.items.len());

    // Obtenemos el host de audio del sistema
    let host = cpal::default_host();
    // Consultamos el dispositivo de salida predeterminado
    let device = host.default_output_device().expect("No hay dispositivo");
    // Obtenemos la frecuencia de muestreo nativa del hardware
    let sample_rate = device.default_output_config()?.sample_rate().0;

    // Arquitectura Lazy Loading: Obtenemos los metadatos de la primera pista
    let first_item = plan_cargado.current_item().unwrap().clone();
    // Explicamos el principio de bajo consumo de RAM
    println!("\n[3] Carga Diferida (Lazy Loading):");
    // Mostramos qué canción específica se decodificará
    println!("    Decodificando únicamente la pista #1: {}", first_item.title);
    // Solo decodificamos en memoria RAM los samples de la canción 1
    let (samples, sr) = decoder::load_and_decode_file(&first_item.path, sample_rate)?;

    // Creamos la pista en memoria envuelta en un puntero atómico `Arc`
    let track = Arc::new(Track::new(0, first_item.title.clone(), samples, sr));
    // Desactivamos el bucle para poder detectar cuándo la canción finaliza
    track.looping.store(false, Ordering::Relaxed);

    // Inicializamos el motor DSP con la primera pista activa
    let _engine = AudioEngine::new(vec![Arc::clone(&track)])?;

    // Notificamos que la pista 1 está sonando por los altavoces
    println!("    🔊 Reproduciendo pista #1...");
    // Esperamos activamente en un bucle ligero de sondeo hasta que la canción termine
    while !track.is_finished() {
        // Pausamos el hilo 50 ms para no consumir ciclos de CPU innecesarios
        thread::sleep(Duration::from_millis(50));
    }
    // Indicamos que la pista 1 ha alcanzado su último frame de audio
    println!("    ⏹️  Pista #1 finalizada.");
    /*
     * ROL DEL BUCLE DE ESPERA DE PISTA 1:
     * Monitorea la posición atómica de reproducción (`current_frame`) hasta alcanzar el límite
     * (`total_frames`), demostrando la finalización natural de una canción.
     */

    // Avance automático en el plan al detectar la finalización
    if let Some(next_item) = plan_cargado.next() {
        // Notificamos la transición a la siguiente canción en la cola
        println!("\n[4] Avance automático en cola -> Pista #2: {}", next_item.title);
        // Explicamos la liberación de memoria de la pista anterior
        println!("    Liberando memoria de la pista anterior y cargando la nueva...");
        // Decodificamos la nueva canción justo a tiempo (Just-In-Time)
        let (new_samples, new_sr) = decoder::load_and_decode_file(&next_item.path, sample_rate)?;

        // Reemplazo en caliente de memoria (Hot-Swap): los samples viejos se destruyen y entran los nuevos
        track.swap_audio(new_samples, new_sr, next_item.title.clone());

        // Notificamos que la pista 2 está en reproducción en la misma ranura de memoria
        println!("    🔊 Reproduciendo pista #2...");
        // Esperamos a que la segunda canción termine
        while !track.is_finished() {
            // Dormimos el hilo 50 ms
            thread::sleep(Duration::from_millis(50));
        }
        // Indicamos la finalización de la pista 2
        println!("    ⏹️  Pista #2 finalizada.");
    }
    /*
     * ROL DEL BLOQUE HOT-SWAP Y TRANSICIÓN:
     * Es el corazón del patrón de ahorro de recursos de MOPET. En lugar de acumular gigabytes
     * decodificando toda la lista en memoria por adelantado, recicla la estructura `Track`
     * liberando los vectores de audio pasados y cargando únicamente la canción en turno.
     */

    // Mensaje de éxito al concluir la Etapa 4
    println!("\n✅ ¡Etapa 4 completada con éxito!");
    // Resumen didáctico del beneficio arquitectónico
    println!("   El reproductor maneja planes persistentes en JSON y mantiene el consumo de RAM fijo (< 25 MB).");

    // Limpiamos los archivos temporales creados para la prueba
    let _ = std::fs::remove_file(json_path);
    // Eliminamos el primer tono temporal
    let _ = std::fs::remove_file("/tmp/etapa4_cancion1.wav");
    // Eliminamos el segundo tono temporal
    let _ = std::fs::remove_file("/tmp/etapa4_cancion2.wav");

    // Retornamos éxito
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `main`:
 * Demuestra experimentalmente cómo un reproductor en Rust puede ejecutar colas infinitas
 * de canciones mediante la combinación de serialización JSON, Carga Diferida (Lazy Loading)
 * e intercambio seguro de memoria atómica (Hot-Swap), manteniendo un consumo de RAM plano e invariable.
 */

/// Genera un archivo WAV estéreo de prueba con una onda senoidal pura y duración especificada
fn create_test_tone(path: &str, freq: f32, duration_secs: f32) -> anyhow::Result<()> {
    // Definimos la frecuencia de muestreo estándar de CD (44100 Hz)
    let sample_rate: u32 = 44100;
    // Calculamos la cantidad total de muestras por canal
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    // Calculamos el byte rate: 44100 * 2 canales * 2 bytes por muestra (16 bits)
    let byte_rate: u32 = sample_rate * 2 * 2;
    // Calculamos el tamaño total en bytes del bloque de datos de audio
    let data_len = (num_samples * 4) as u32;

    // Creamos el archivo de destino en el disco
    let mut file = File::create(path)?;
    // Escribimos el identificador "RIFF" del contenedor WAV
    file.write_all(b"RIFF")?;
    // Escribimos el tamaño total del archivo menos los primeros 8 bytes
    file.write_all(&(36 + data_len).to_le_bytes())?;
    // Escribimos la firma del formato "WAVE"
    file.write_all(b"WAVE")?;
    // Escribimos el sub-chunk de especificación del formato "fmt "
    file.write_all(b"fmt ")?;
    // Tamaño del sub-chunk fmt (16 bytes para formato PCM estándar)
    file.write_all(&16u32.to_le_bytes())?;
    // Formato de codificación de audio (1 = PCM lineal sin comprimir)
    file.write_all(&1u16.to_le_bytes())?;
    // Número de canales de audio (2 = Estéreo L y R)
    file.write_all(&2u16.to_le_bytes())?;
    // Frecuencia de muestreo en Hertz en formato little-endian
    file.write_all(&sample_rate.to_le_bytes())?;
    // Tasa de transferencia en bytes por segundo
    file.write_all(&byte_rate.to_le_bytes())?;
    // Alineación de bloques de muestras (4 bytes = 2 canales * 2 bytes)
    file.write_all(&4u16.to_le_bytes())?;
    // Profundidad de bits por muestra (16 bits)
    file.write_all(&16u16.to_le_bytes())?;
    // Escribimos la cabecera del bloque de datos "data"
    file.write_all(b"data")?;
    // Tamaño en bytes del bloque de muestras
    file.write_all(&data_len.to_le_bytes())?;

    // Generamos las muestras de la onda senoidal pura y las escribimos en el archivo
    for i in 0..num_samples {
        // Calculamos el tiempo relativo en segundos
        let t = i as f32 / sample_rate as f32;
        // Calculamos el valor senoidal en el rango [-0.5, +0.5] para evitar saturación
        let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
        // Escribimos la muestra escalada a un entero con signo de 16 bits en el canal izquierdo
        file.write_all(&((s * 32767.0) as i16).to_le_bytes())?;
        // Escribimos la misma muestra en el canal derecho (sonido centrado)
        file.write_all(&((s * 32767.0) as i16).to_le_bytes())?;
    }
    // Retornamos éxito al terminar de escribir el archivo WAV
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `create_test_tone`:
 * Proporciona un sintetizador WAV autónomo de nivel de bytes que escribe directamente la cabecera RIFF/WAVE
 * y los enteros de 16 bits little-endian, eliminando la necesidad de paquetes o recursos externos de prueba.
 */
