// Declaración de los módulos que componen la arquitectura completa de MOPET en la Etapa 5
// Módulo de menú interactivo guiado por texto desde la terminal
mod cli_flow;
// Módulo de decodificación y remuestreo de archivos de audio multimedia (MP3, MP4/AAC, WAV, FLAC, OGG)
mod decoder;
// Módulo del motor DSP de audio de baja latencia con CPAL, mezcla SIMD y limitador suave
mod engine;
// Módulo de planes de reproducción, listas de canciones en cola y serialización JSON
mod plan;
// Módulo de sintetizadores procedurales para el modo demostración
mod synth;
// Módulo de pistas de audio atómicas con controles en tiempo real y soporte Hot-Swap
mod track;
// Módulo de interfaz gráfica de terminal (TUI) con crossterm a 30 FPS y vúmetros
mod ui;

// Importamos el tipo Result del crate anyhow para manejo idiomático y robusto de errores
use anyhow::Result;
// Importamos las acciones decididas en el menú interactivo
use cli_flow::CliAction;
// Importamos los traits de CPAL para inicializar y configurar el dispositivo físico de salida de sonido
use cpal::traits::{DeviceTrait, HostTrait};
// Importamos el motor de audio central
use engine::AudioEngine;
// Importamos la estructura de planes de reproducción
use plan::AudioPlan;
// Importamos el módulo de entorno para inspeccionar argumentos de la terminal
use std::env;
// Importamos referencias y estructuras de rutas en el sistema de archivos
use std::path::{Path, PathBuf};
// Importamos el puntero atómico compartido para compartir las pistas de audio entre hilos
use std::sync::Arc;
// Importamos la estructura de pista de audio
use track::Track;
// Importamos el controlador de la interfaz de terminal interactiva
use ui::TerminalUi;

/// Punto de entrada del ejecutable de la Etapa 5.
/// Integra armónicamente todos los componentes del proyecto: menú guiado para la terminal,
/// decodificación multimedia universal, reproducción multipista simultánea, listas en cola persistentes
/// y consola interactiva estilo mezclador profesional (TUI).
fn main() -> Result<()> {
    // Capturamos los argumentos pasados en la línea de comandos omitiendo el nombre del ejecutable
    let args: Vec<String> = env::args().skip(1).collect();

    // Verificamos si el usuario solicitó la ayuda con -h o --help
    if args.iter().any(|a| a == "-h" || a == "--help") {
        // Imprimimos la ayuda completa en la terminal
        print_help();
        // Finalizamos la ejecución de inmediato con éxito
        return Ok(());
    }

    // Inicializamos el host de audio del sistema operativo (ALSA/PulseAudio en Linux, CoreAudio en macOS, WASAPI en Windows)
    let host = cpal::default_host();
    // Consultamos el dispositivo de salida físico predeterminado
    let device = host
        // Intentamos obtener el dispositivo
        .default_output_device()
        // Si no hay hardware de audio, detenemos la ejecución con un mensaje claro
        .expect("No se encontró dispositivo de salida de audio");
    // Obtenemos la configuración nativa de salida
    let default_config = device.default_output_config()?;
    // Extraemos la frecuencia de muestreo del hardware (ej. 44100 Hz o 48000 Hz)
    let target_sample_rate = default_config.sample_rate().0;

    // Instanciamos el controlador de la interfaz interactiva de terminal
    let mut ui = TerminalUi::new();

    // Caso 1: El usuario ejecutó `mopet` sin argumentos -> Lanzamos el flujo interactivo guiado
    if args.is_empty() {
        // Encabezado visual de bienvenida
        println!("==================================================================");
        // Título de la etapa
        println!("   ETAPA 5: INTERFAZ DE TERMINAL INTERACTIVA (TUI) Y CLI GLOBAL   ");
        // Separador
        println!("==================================================================");
        // Mostramos el dispositivo de salida activo y su frecuencia de muestreo
        println!("Dispositivo: {} │ Frecuencia: {} Hz", device.name().unwrap_or_else(|_| "Audio".to_string()), target_sample_rate);

        // Desplegamos el menú interactivo y esperamos la decisión del usuario
        match cli_flow::run_interactive_menu()? {
            // Opción de reproducir un plan de cola
            CliAction::PlayPlan(plan) => {
                // Arrancamos el reproductor de planes con Lazy Loading
                run_plan_player(plan, target_sample_rate, &mut ui)?;
            }
            // Opción de mezclar dos o más canciones al mismo tiempo
            CliAction::PlayMulti(files) => {
                // Arrancamos el modo multipista simultáneo
                run_multi_track_mode(files, target_sample_rate, &mut ui)?;
            }
            // Opción de demostración procedural sintetizada
            CliAction::PlayDemo => {
                // Arrancamos el modo demo musical
                run_demo_mode(target_sample_rate, &mut ui)?;
            }
            // Opción de salida
            CliAction::Exit => {
                // Despedida
                println!("Saliendo.");
                // Retornamos éxito
                return Ok(());
            }
        }
        // Retornamos tras finalizar la sesión interactiva
        return Ok(());
    }
    /*
     * ROL DEL BLOQUE DE ENTRADA SIN ARGUMENTOS:
     * Brinda la experiencia conversacional "plug-and-play" solicitada por el usuario:
     * al escribir simplemente `mopet` en cualquier consola, se abre el asistente para elegir qué escuchar.
     */

    // Caso 2: El usuario pasó argumentos directos por línea de comandos (ej. `mopet cancion.mp3` o `mopet --multi a.wav b.wav`)
    // Verificamos si se especificó la bandera de demostración
    let is_demo = args.iter().any(|a| a == "--demo");
    // Verificamos si se activó el modo multipista explícito
    let is_multi_mode = args.iter().any(|a| a == "--multi");

    // Variable para almacenar la ruta de un archivo de plan a cargar si se indicó
    let mut plan_path_load: Option<PathBuf> = None;
    // Variable para almacenar la ruta de un directorio si se indicó
    let mut dir_path: Option<PathBuf> = None;
    // Vector para acumular archivos posicionales
    let mut positional_files: Vec<PathBuf> = Vec::new();

    // Índice para recorrer los argumentos
    let mut i = 0;
    // Bucle de análisis de argumentos CLI
    while i < args.len() {
        // Tomamos una referencia al argumento actual
        let arg = &args[i];
        // Bandera de plan específico: `--plan ruta.json` o `-p ruta.json`
        if arg == "--plan" || arg == "-p" {
            if i + 1 < args.len() {
                plan_path_load = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
        // Bandera de carpeta: `--dir /carpeta/` o `-d /carpeta/`
        } else if arg == "--dir" || arg == "-d" {
            if i + 1 < args.len() {
                dir_path = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
        // Argumentos posicionales (no comienzan con '-')
        } else if !arg.starts_with("--") {
            let p = PathBuf::from(arg);
            // Si la ruta es un directorio
            if p.is_dir() {
                dir_path = Some(p);
            // Si es un archivo JSON de plan
            } else if p.extension().and_then(|e| e.to_str()) == Some("json") {
                plan_path_load = Some(p);
            // Si es un archivo individual de audio
            } else {
                positional_files.push(p);
            }
        }
        // Avanzamos al siguiente argumento
        i += 1;
    }

    // Despachamos según las opciones detectadas
    if is_demo {
        // Si se pidió `--demo`, ejecutamos el modo de demostración generativo
        run_demo_mode(target_sample_rate, &mut ui)?;
    } else if is_multi_mode && !positional_files.is_empty() {
        // Si se especificó `--multi` con archivos, arrancamos la mezcla multipista
        run_multi_track_mode(positional_files, target_sample_rate, &mut ui)?;
    } else {
        // Determinamos el plan de reproducción a utilizar
        let plan_opt = if let Some(p) = plan_path_load {
            // Plan cargado desde archivo JSON específico
            Some(AudioPlan::load_from_file(&p)?)
        } else if let Some(d) = dir_path {
            // Plan generado escaneando la carpeta indicada
            Some(AudioPlan::from_directory(&d, true)?)
        } else if !positional_files.is_empty() {
            // Plan armado con los archivos de audio posicionales
            Some(AudioPlan::from_paths(&positional_files))
        } else if Path::new("mopet_plan.json").exists() {
            // Si no se pasó nada pero existe `mopet_plan.json`, lo recuperamos automáticamente
            AudioPlan::load_from_file("mopet_plan.json").ok()
        } else {
            // Sin plan disponible
            None
        };

        // Si se pudo armar o encontrar un plan, lo ejecutamos
        if let Some(plan) = plan_opt {
            run_plan_player(plan, target_sample_rate, &mut ui)?;
        } else {
            // Si no se especificó nada válido, caemos al modo demo
            run_demo_mode(target_sample_rate, &mut ui)?;
        }
    }
    /*
     * ROL DEL BLOQUE DE ANÁLISIS CLI:
     * Permite que usuarios avanzados o scripts de automatización utilicen MOPET directamente
     * desde la terminal sin pasar por menús interactivos (ej. `mopet -d ~/Musica` o `mopet *.flac`).
     */

    // Finalización exitosa
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `main`:
 * Es el director de orquesta global de MOPET. Resuelve la intención del usuario (sea interactiva
 * o mediante argumentos de terminal), inicializa el hardware y delega la ejecución al modo correspondiente.
 */

/// Inicializa y ejecuta la reproducción de un plan de canciones en cola con Carga Diferida (Lazy Loading)
fn run_plan_player(mut plan: AudioPlan, target_sample_rate: u32, ui: &mut TerminalUi) -> Result<()> {
    // Si el plan no tiene pistas, avisamos y salimos
    if plan.items.is_empty() {
        println!("El plan no contiene pistas de audio.");
        return Ok(());
    }

    // Obtenemos la primera pista activa del plan
    let current_item = plan.current_item().cloned().expect("El plan tiene elementos");
    // Decodificamos únicamente esta canción en memoria RAM
    let (samples, sr) = decoder::load_and_decode_file(&current_item.path, target_sample_rate)?;
    // Construimos la pista en memoria
    let track = Arc::new(Track::new(0, current_item.title.clone(), samples, sr));
    // Inicializamos el motor DSP con la pista
    let engine = AudioEngine::new(vec![track])?;

    // Cedemos el control a la interfaz interactiva pasando el plan como contexto de cola
    ui.run(&engine, Some(&mut plan))?;
    // Éxito al concluir la sesión
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `run_plan_player`:
 * Ejecuta el modo estándar de reproducción de colas de canciones con consumo de memoria mínimo (< 25 MB).
 */

/// Inicializa y ejecuta el modo de demostración con 3 pistas musicales generadas por software
fn run_demo_mode(target_sample_rate: u32, ui: &mut TerminalUi) -> Result<()> {
    // Generamos las 3 pistas sintetizadas (Batería, Bajo y Arpegio)
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
    // Inicializamos el motor DSP con las 3 pistas
    let engine = AudioEngine::new(tracks)?;
    // Lanzamos la interfaz gráfica en modo multipista sin plan
    ui.run(&engine, None)?;
    // Éxito
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `run_demo_mode`:
 * Provee un banco de pruebas de sonido inmediato y libre de archivos externos para demostrar
 * las capacidades de mezcla multicanal, vúmetros L/R y limitador suave.
 */

/// Inicializa y ejecuta el modo multipista cargando 2 o más archivos de audio simultáneos
fn run_multi_track_mode(files: Vec<PathBuf>, target_sample_rate: u32, ui: &mut TerminalUi) -> Result<()> {
    // Vector para acumular las pistas decodificadas
    let mut tracks = Vec::new();
    // Recorremos cada archivo de audio
    for (i, path) in files.into_iter().enumerate() {
        let path_str = path.display().to_string();
        // Intentamos decodificar el archivo al formato del hardware
        if let Ok((samples, sr)) = decoder::load_and_decode_file(&path, target_sample_rate) {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(&path_str).to_string();
            let t = Arc::new(Track::new(i, name, samples, sr));
            // Activamos bucle continuo para la mezcla simultánea
            t.looping.store(true, std::sync::atomic::Ordering::Relaxed);
            tracks.push(t);
        }
    }
    // Si ninguna pista pudo ser decodificada, informamos
    if tracks.is_empty() {
        println!("No se pudieron cargar las pistas.");
        return Ok(());
    }
    // Inicializamos el motor DSP con todas las pistas en paralelo
    let engine = AudioEngine::new(tracks)?;
    // Ejecutamos la TUI en modo mezclador multipista
    ui.run(&engine, None)?;
    // Éxito
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `run_multi_track_mode`:
 * Resuelve el requerimiento específico de cargar 2 o más archivos de audio a la vez,
 * permitiendo mezclarlos, controlar sus volúmenes y balances de forma independiente en vivo.
 */

/// Imprime en la consola el texto de ayuda con las opciones y modos de ejecución de MOPET
fn print_help() {
    println!(
        r#"MOPET - Reproductor de Audio Multipista Ultra-Eficiente en Rust

USO:
    mopet [OPCIONES] [ARCHIVOS_O_CARPETAS...]

OPCIONES:
    -h, --help           Muestra esta ayuda detallada.
    --demo               Inicia el modo demostración generativo (Batería, Bajo y Sintetizador).
    --multi              Reproduce múltiples archivos de forma simultánea (Modo Mezclador).
    -d, --dir <RUTA>     Escanea y reproduce todos los audios de la carpeta indicada.
    -p, --plan <JSON>    Carga y ejecuta un plan de reproducción guardado previamente.

CONTROLES DURANTE LA REPRODUCCIÓN:
    [Espacio]            Play / Pausa general (Master).
    [N] / [B]            Canción Siguiente / Anterior en el plan.
    [T]                  Alternar Modo de Repetición (Secuencial, Loop Plan, Loop Canción).
    [W]                  Guardar el plan actual en 'mopet_plan.json'.
    [1..9] / [Tab]       Seleccionar pista activa.
    [↑] / [↓]            Ajustar volumen de la pista seleccionada (±5%).
    [←] / [→]            Ajustar balance estéreo (Panorama L / R).
    [M]                  Silenciar pista seleccionada (Mute).
    [S]                  Modo solo (aislar pista seleccionada).
    [L]                  Alternar repetición en bucle de la pista.
    [J] / [K]            Rebobinar / Adelantar 5 segundos (Seek ±5s).
    [R]                  Reiniciar reproducción al inicio (00:00).
    [+] / [-]            Subir / Bajar volumen general Master.
    [Q] / [Esc]          Salir del reproductor de forma limpia.
"#
    );
}
/*
 * ROL DE LA FUNCIÓN `print_help`:
 * Proporciona documentación rápida y accesible directamente desde la línea de comandos
 * explicando cada bandera y cada tecla de control del reproductor.
 */
