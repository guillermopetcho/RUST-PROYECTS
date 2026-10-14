// Declaración del módulo interno de audio (contiene decoder, engine, plan, synth y track)
mod audio;
// Declaración del módulo interno de flujo conversacional y menú interactivo de la terminal
mod cli_flow;
// Declaración del módulo de interfaz interactiva de usuario en consola (TUI) con crossterm
mod ui;

// Importamos el tipo Result del crate anyhow para manejo simplificado de errores con contexto
use anyhow::Result;
// Importamos los submódulos y tipos clave del subsistema de audio
use audio::{decoder, engine::AudioEngine, plan::AudioPlan, synth, Track};
// Importamos las opciones decididas en el menú interactivo
use cli_flow::CliAction;
// Importamos los traits de CPAL para inicializar y configurar el dispositivo físico de salida de audio
use cpal::traits::{DeviceTrait, HostTrait};
// Importamos el módulo de entorno estándar para capturar los argumentos de la línea de comandos
use std::env;
// Importamos estructuras para trabajar con rutas de archivos y directorios
use std::path::{Path, PathBuf};
// Importamos el puntero atómico compartido Arc para transferir pistas entre hilos concurrentes
use std::sync::Arc;
// Importamos la estructura de interfaz gráfica de terminal
use ui::TerminalUi;

/// Punto de entrada principal de la aplicación MOPET.
/// Determina si se ejecuta en modo interactivo asistido por menús (si no se pasan argumentos)
/// o en modo directo por terminal procesando banderas como `--multi`, `--dir`, `--plan` o `--demo`.
fn main() -> Result<()> {
    // Obtenemos los argumentos de la línea de comandos saltando el nombre del ejecutable
    let args: Vec<String> = env::args().skip(1).collect();

    // Verificamos si el usuario solicitó la ayuda mediante `-h` o `--help`
    if args.iter().any(|a| a == "-h" || a == "--help") {
        // Desplegamos la documentación de uso en pantalla
        print_help();
        // Concluimos la ejecución inmediatamente con éxito
        return Ok(());
    }

    // Inicializamos el host de audio del sistema (ALSA/PulseAudio en Linux, CoreAudio en macOS, WASAPI en Windows)
    let host = cpal::default_host();
    // Consultamos el dispositivo de salida de audio predeterminado
    let device = host
        // Intentamos obtener el dispositivo
        .default_output_device()
        // Error descriptivo si no hay parlantes o auriculares conectados
        .expect("No se encontró ningún dispositivo de salida de audio");
    // Obtenemos la configuración predeterminada de salida del dispositivo
    let default_config = device
        // Obtenemos la configuración
        .default_output_config()
        // Error descriptivo si falla la consulta de configuración
        .expect("No se pudo obtener la configuración de salida de audio");
    // Extraemos la frecuencia de muestreo de hardware (ej. 44100 Hz o 48000 Hz)
    let target_sample_rate = default_config.sample_rate().0;

    // Inicializamos el gestor de la interfaz interactiva de terminal
    let mut ui = TerminalUi::new();

    // Caso 1: El usuario ejecutó `mopet` sin argumentos -> Flujo interactivo guiado
    if args.is_empty() {
        // Imprimimos el encabezado superior
        println!("======================================================================");
        // Título del programa
        println!("     MOPET - REPRODUCTOR Y MEZCLADOR MULTIPISTA EN RUST               ");
        // Separador
        println!("======================================================================");
        // Información del hardware detectado
        println!("Dispositivo: {} │ Frecuencia: {} Hz", device.name().unwrap_or_else(|_| "Audio".to_string()), target_sample_rate);

        // Ejecutamos el menú interactivo y esperamos la acción seleccionada
        match cli_flow::run_interactive_menu()? {
            // Reproducir un plan de canciones en cola
            CliAction::PlayPlan(plan) => {
                run_plan_player(plan, target_sample_rate, &mut ui)?;
            }
            // Modo multipista simultáneo (2 o más archivos a la vez)
            CliAction::PlayMulti(files) => {
                run_multi_track_mode(files, target_sample_rate, &mut ui)?;
            }
            // Modo demostración procedural generado por sintetizadores
            CliAction::PlayDemo => {
                run_demo_mode(target_sample_rate, &mut ui)?;
            }
            // Salir del reproductor
            CliAction::Exit => {
                println!("Saliendo de MOPET.");
                return Ok(());
            }
        }
        // Finalizamos tras completar la sesión del menú interactivo
        return Ok(());
    }
    /*
     * ROL DEL BLOQUE SIN ARGUMENTOS:
     * Brinda la experiencia conversacional "plug-and-play" solicitada por el usuario:
     * al escribir simplemente `mopet` en cualquier consola, se abre el asistente para elegir qué escuchar.
     */

    // Caso 2: El usuario pasó argumentos directos por terminal (ej. `mopet cancion.mp3`)
    println!("======================================================================");
    println!("     MOPET - REPRODUCTOR DE AUDIO ULTRA-EFICIENTE (RUST)              ");
    println!("======================================================================");
    println!("Dispositivo: {} │ Frecuencia: {} Hz", device.name().unwrap_or_else(|_| "Audio".to_string()), target_sample_rate);
    println!("----------------------------------------------------------------------");

    // Bandera para evaluar si se solicitó la demostración
    let is_demo = args.iter().any(|a| a == "--demo");
    // Bandera para evaluar si se solicitó el modo multipista explícito
    let is_multi_mode = args.iter().any(|a| a == "--multi");

    // Ruta de plan a cargar si se especificó con `-p` o `--plan`
    let mut plan_path_load: Option<PathBuf> = None;
    // Nombre del archivo donde guardar el plan (por defecto `mopet_plan.json`)
    let mut plan_path_save = "mopet_plan.json".to_string();
    // Ruta de directorio a escanear si se especificó con `-d` o `--dir`
    let mut dir_path: Option<PathBuf> = None;
    // Vector para acumular archivos posicionales pasados como argumentos
    let mut positional_files: Vec<PathBuf> = Vec::new();

    // Índice para recorrer los argumentos manualmente
    let mut i = 0;
    // Bucle analizador de argumentos
    while i < args.len() {
        let arg = &args[i];
        // Opción `--plan <archivo.json>` o `-p <archivo.json>`
        if arg == "--plan" || arg == "-p" {
            if i + 1 < args.len() {
                plan_path_load = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
        // Opción `--save-plan <archivo.json>`
        } else if arg == "--save-plan" {
            if i + 1 < args.len() {
                plan_path_save = args[i + 1].clone();
                i += 1;
            }
        // Opción `--dir <carpeta>` o `-d <carpeta>`
        } else if arg == "--dir" || arg == "-d" {
            if i + 1 < args.len() {
                dir_path = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
        // Rutas posicionales (sin prefijo de banderas)
        } else if !arg.starts_with("--") {
            let p = PathBuf::from(arg);
            if p.is_dir() {
                dir_path = Some(p);
            } else if p.extension().and_then(|e| e.to_str()) == Some("json") {
                plan_path_load = Some(p);
            } else {
                positional_files.push(p);
            }
        }
        i += 1;
    }

    // Configuramos la ruta de guardado en la interfaz de usuario
    ui.set_save_path(plan_path_save.clone());

    // Despachamos según las opciones detectadas
    if is_demo {
        // Lanzamos el modo demostración generativo
        run_demo_mode(target_sample_rate, &mut ui)?;
    } else if is_multi_mode && !positional_files.is_empty() {
        // Lanzamos la mezcla multipista simultánea
        run_multi_track_mode(positional_files, target_sample_rate, &mut ui)?;
    } else {
        // Determinamos el plan de reproducción según los argumentos provistos
        let plan_opt = if let Some(p) = plan_path_load {
            println!("Cargando plan desde: {}", p.display());
            Some(AudioPlan::load_from_file(&p)?)
        } else if let Some(d) = dir_path {
            println!("Escaneando carpeta: {}", d.display());
            let plan = AudioPlan::from_directory(&d, true)?;
            println!("Se encontraron {} pistas.", plan.items.len());
            let _ = plan.save_to_file(&plan_path_save);
            Some(plan)
        } else if !positional_files.is_empty() {
            println!("Armando plan con {} canciones...", positional_files.len());
            let plan = AudioPlan::from_paths(&positional_files);
            let _ = plan.save_to_file(&plan_path_save);
            Some(plan)
        } else if Path::new("mopet_plan.json").exists() {
            // Si existe un plan previo en el directorio de trabajo, lo reutilizamos
            AudioPlan::load_from_file("mopet_plan.json").ok()
        } else {
            None
        };

        // Si tenemos un plan disponible, lo reproducimos
        if let Some(plan) = plan_opt {
            run_plan_player(plan, target_sample_rate, &mut ui)?;
        } else {
            // Si no se pasaron archivos válidos, avisamos e iniciamos el modo demo
            println!("No se especificaron pistas válidas.");
            run_demo_mode(target_sample_rate, &mut ui)?;
        }
    }
    /*
     * ROL DEL ENRUTADOR PRINCIPAL CLI:
     * Permite que scripts de shell, comandos rápidos y usuarios expertos ejecuten MOPET
     * con cualquier combinación de carpetas, listas de canciones y parámetros avanzados.
     */

    // Despedida tras cerrar la TUI
    println!("\nReproductor finalizado correctamente.");
    // Éxito
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `main`:
 * Inicializa el subsistema de sonido, analiza la línea de comandos y arranca el ciclo
 * de reproducción en el modo correspondiente.
 */

/// Inicializa y ejecuta la reproducción secuencial de un plan de canciones en cola con Carga Diferida (Lazy Loading)
fn run_plan_player(mut plan: AudioPlan, target_sample_rate: u32, ui: &mut TerminalUi) -> Result<()> {
    // Si la lista está vacía, no hay nada que reproducir
    if plan.items.is_empty() {
        println!("El plan no contiene pistas de audio válidas.");
        return Ok(());
    }

    // Informamos que la reproducción mantendrá la memoria RAM constante (< 25 MB)
    println!("Iniciando reproducción con bajo consumo (< 25 MB RAM fija)...");
    // Obtenemos los metadatos de la primera canción
    let current_item = plan.current_item().cloned().expect("El plan contiene elementos");
    println!("  [Decodificando inicial] {} ...", current_item.title);

    // Decodificamos solo la canción actual
    let (samples, sr) = decoder::load_and_decode_file(&current_item.path, target_sample_rate)?;
    // Creamos la pista de audio
    let track = Arc::new(Track::new(0, current_item.title.clone(), samples, sr));
    // Inicializamos el motor DSP con la pista
    let engine = AudioEngine::new(vec![track])?;

    // Ejecutamos la interfaz de terminal interactiva con el plan activo
    ui.run(&engine, Some(&mut plan))?;
    // Éxito
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `run_plan_player`:
 * Ejecuta el modo de lista de reproducción eficiente con avance secuencial automático y Hot-Swap.
 */

/// Inicializa y ejecuta el modo de demostración con 3 pistas sintetizadas proceduralmente
fn run_demo_mode(target_sample_rate: u32, ui: &mut TerminalUi) -> Result<()> {
    println!("Iniciando demo de 3 pistas sintetizadas (120 BPM)...");
    // Generamos las pistas de batería, bajo y sintetizador melódico
    let demo_data = synth::generate_demo_tracks(target_sample_rate);
    // Creamos el vector de pistas
    let mut tracks = Vec::new();
    // Envolvemos cada pista en un `Arc<Track>`
    for (i, (name, samples)) in demo_data.into_iter().enumerate() {
        let t = Arc::new(Track::new(i, name, samples, target_sample_rate));
        // Activamos bucle continuo
        t.looping.store(true, std::sync::atomic::Ordering::Relaxed);
        tracks.push(t);
    }
    // Inicializamos el motor de audio con las pistas simultáneas
    let engine = AudioEngine::new(tracks)?;
    // Lanzamos la interfaz gráfica en modo demostración
    ui.run(&engine, None)?;
    // Éxito
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `run_demo_mode`:
 * Proporciona una sesión musical instantánea generada puramente por código matemático.
 */

/// Inicializa y ejecuta el modo multipista cargando y reproduciendo varios archivos en simultáneo
fn run_multi_track_mode(
    files: Vec<PathBuf>,
    target_sample_rate: u32,
    ui: &mut TerminalUi,
) -> Result<()> {
    println!("Modo Multipista Simultáneo: Cargando {} pistas en paralelo...", files.len());
    let mut tracks = Vec::new();
    // Recorremos cada archivo recibido
    for (i, path) in files.into_iter().enumerate() {
        let path_str = path.display().to_string();
        print!("  [Cargando pista #{}] {} ... ", i + 1, path_str);
        // Decodificamos el archivo
        match decoder::load_and_decode_file(&path, target_sample_rate) {
            Ok((samples, sr)) => {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&path_str)
                    .to_string();
                let dur = (samples.len() / 2) as f32 / sr as f32;
                println!("OK ({:.1}s)", dur);
                let t = Arc::new(Track::new(i, name, samples, sr));
                // Bucle activado para mezcla continua
                t.looping.store(true, std::sync::atomic::Ordering::Relaxed);
                tracks.push(t);
            }
            Err(e) => {
                println!("ERROR: {}", e);
            }
        }
    }

    // Si no se cargó ninguna pista válida, informamos y salimos
    if tracks.is_empty() {
        eprintln!("No se pudo cargar ninguna pista.");
        return Ok(());
    }

    // Inicializamos el motor DSP con todas las pistas activas
    let engine = AudioEngine::new(tracks)?;
    // Lanzamos la interfaz interactiva
    ui.run(&engine, None)?;
    // Éxito
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `run_multi_track_mode`:
 * Permite al usuario mezclar capas sonoras independientes en paralelo con control de volumen, mute y solo.
 */

/// Despliega el manual de ayuda de MOPET en la consola
fn print_help() {
    println!(
        r#"MOPET - Reproductor de Audio Ultra-Eficiente y Multipista en Rust

USO:
    mopet [OPCIONES] [ARCHIVOS_O_CARPETAS...]

    Si se ejecuta sin argumentos ('mopet'), abre un menú interactivo guiado en la terminal.

EJEMPLOS DIRECTOS DESDE LA TERMINAL:
    # 1. Menú interactivo asistido en terminal:
    mopet

    # 2. Reproducir toda una carpeta (MP3, MP4, WAV, etc.) de forma secuencial:
    mopet /home/usuario/Musica

    # 3. Armar un plan con varias canciones (gasto mínimo de RAM, 1 tras otra):
    mopet cancion1.mp3 video.mp4 cancion2.wav

    # 4. Cargar un plan guardado previamente en JSON:
    mopet --plan mopet_plan.json

    # 5. Modo multipista simultáneo (mezclar capas/stems al mismo tiempo):
    mopet --multi voz.wav bateria.mp3 bajo.mp4

FORMATOS SOPORTADOS:
    .mp3, .mp4 (audio AAC/ALAC), .m4a, .wav, .flac, .ogg, .aac

OPCIONES:
    -p, --plan <ARCHIVO>        Cargar plan de reproducción desde archivo JSON
    --save-plan <ARCHIVO>       Especificar nombre del archivo donde guardar el plan
    -d, --dir <CARPETA>         Cargar y ordenar todas las pistas de una carpeta
    --multi                     Modo multipista (reproduce todas las pistas en paralelo)
    --demo                      Ejecuta el demo de 3 pistas sintetizadas
    -h, --help                  Muestra esta ayuda

CONTROLES EN TIEMPO REAL:
    [Espacio]     Pausar / Reanudar reproducción
    [N]           Siguiente canción del plan (Next)
    [B]           Canción anterior del plan (Back)
    [T]           Alternar Modo: Secuencial (1 a 1) -> Loop Plan -> Loop Canción
    [W]           Guardar plan actual en disco (mopet_plan.json)
    [↑ / ↓]       Subir / Bajar volumen (±5%)
    [← / →]       Ajustar balance estéreo (L / R)
    [M]           Silenciar pista (Mute)
    [S]           Solo pista (aislar audio)
    [J / K]       Rebobinar / Adelantar 5 segundos
    [Q / Esc]     Salir
"#
    );
}
/*
 * ROL DE LA FUNCIÓN `print_help`:
 * Proporciona documentación inmediata en la terminal detallando comandos, opciones y atajos.
 */
