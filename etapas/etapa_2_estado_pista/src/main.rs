// Declaramos el módulo 'decoder' para tener la función de decodificación disponible.
mod decoder;

// Declaramos el módulo 'track' donde reside la estructura Track y AtomicF32.
mod track;

// Importamos Ordering para especificar el modelo de consistencia de memoria atómica.
use std::sync::atomic::Ordering;

// Importamos Arc para clonar y compartir referencias del Track entre dos hilos independientes.
use std::sync::Arc;

// Importamos thread para crear hilos nativos del sistema operativo.
use std::thread;

// Importamos Duration para pausar hilos en intervalos de tiempo precisos.
use std::time::Duration;

// Importamos el struct Track desde el módulo local 'track'.
use track::Track;

/*
========================================================================================
[ROL DEL BLOQUE DE DECLARACIONES]:
Configura el entorno de ejecución multihilo importando primitivas de sincronización
y los módulos necesarios para demostrar la concurrencia segura sin bloqueos.
========================================================================================
*/

/// Función principal de la Etapa 2.
/// Demuestra cómo dos hilos pueden interactuar con la misma pista concurrentemente sin colisiones:
/// Un hilo lector (simulando la tarjeta de sonido) y un hilo escritor (simulando la UI del usuario).
fn main() {
    // Encabezado de la etapa en consola.
    println!("==================================================================");
    println!("   ETAPA 2: CONCURRENCIA DE TIEMPO REAL Y VARIABLES ATÓMICAS      ");
    println!("==================================================================");

    // Creamos un vector simulado de 88.200 muestras estéreo (1 segundo de audio a 44.1 kHz).
    let samples = vec![0.5f32; 88200];

    // Envolvemos la pista en un Arc (Atomic Reference Counting) para compartirla entre hilos.
    let track = Arc::new(Track::new(0, "Pista A (Inicial)".into(), samples, 44100));

    // Mostramos los datos iniciales de la pista.
    println!("[1] Pista creada: '{}', Duración: {:.1}s", track.get_name(), track.duration_secs());
    println!("    Volumen inicial: {:.0}%, Mute: {}", track.volume.load(Ordering::Relaxed) * 100.0, track.muted.load(Ordering::Relaxed));

    // Clonamos el puntero inteligente Arc. No duplica los datos de audio, solo incrementa el contador de referencias.
    let track_reader = Arc::clone(&track);
    /*
    [ROL DE LA CREACIÓN DEL ARC]:
    `Arc` (Atomic Reference Counted pointer) es el mecanismo de Rust para propiedad compartida
    entre hilos múltiples. Asegura que los datos vivan en el Heap hasta que el último hilo termine.
    */

    // --- HILO SECUNDARIO (SIMULADOR DE MOTOR DE AUDIO) ---
    // `thread::spawn` inicia un hilo nativo. La palabra clave `move` transfiere `track_reader` al nuevo hilo.
    let handle = thread::spawn(move || {
        // Ejecutamos 5 lecturas consecutivas simulando el callback de la tarjeta de sonido.
        for _ in 0..5 {
            // Dormimos 50 milisegundos entre ciclos.
            thread::sleep(Duration::from_millis(50));

            // Lectura atómica de volumen sin bloqueos.
            let vol = track_reader.volume.load(Ordering::Relaxed);

            // Lectura atómica del estado Mute.
            let muted = track_reader.muted.load(Ordering::Relaxed);

            // Lectura del nombre protegido por RwLock.
            let name = track_reader.get_name();

            // Lectura atómica de la cantidad de cuadros disponibles.
            let frames = track_reader.total_frames.load(Ordering::Relaxed);

            // Mostramos los valores leídos por el hilo secundario en tiempo real.
            println!("  [Hilo Audio] Leyendo '{}' -> Vol: {:.0}%, Mute: {}, Frames: {}", name, vol * 100.0, muted, frames);
        }
    });
    /*
    [ROL DEL HILO LECTOR]:
    Representa el consumidor de datos. En un reproductor real, este hilo es invocado
    por ALSA o PulseAudio periódicamente para llenar el búfer de la tarjeta de sonido.
    */

    // --- HILO PRINCIPAL (SIMULADOR DE INTERFAZ DE USUARIO / TECLADO) ---
    // Esperamos 60 ms para permitir que el hilo lector haga su primera pasada.
    thread::sleep(Duration::from_millis(60));

    println!("\n[2] Hilo Principal: Modificando volumen (+20%) y silenciando (Mute)...");
    // El usuario presiona teclas imaginarias para subir volumen y silenciar.
    track.adjust_volume(0.2);
    track.toggle_mute();

    // Esperamos otros 80 ms para observar cómo el hilo lector detecta los cambios atómicamente.
    thread::sleep(Duration::from_millis(80));

    println!("\n[3] Hilo Principal: Realizando Hot-Swap (Reemplazo por 'Pista B' de 2 segundos)...");
    // Generamos un nuevo búfer con el doble de tamaño (176.400 muestras = 2 segundos).
    let new_samples = vec![0.8f32; 176400];

    // Sustituimos el audio en caliente: el búfer anterior se descarta de la memoria RAM automáticamente.
    track.swap_audio(new_samples, 44100, "Pista B (Reemplazada en caliente)".into());

    // Esperamos a que el hilo secundario complete todas sus lecturas de forma segura.
    handle.join().unwrap();
    /*
    [ROL DE LA SINCRONIZACIÓN (join)]:
    `join()` pausa el hilo principal hasta que el hilo secundario termine su ejecución,
    garantizando que no queden hilos huérfanos ni terminaciones abruptas.
    */

    // Conclusión de la prueba de concurrencia.
    println!("\n✅ ¡Etapa 2 completada con éxito!");
    println!("   La estructura Track permite comunicación segura y sin bloqueos entre hilos.");
}
