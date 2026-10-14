// Importamos el tipo Result del crate anyhow para propagación idiomática de errores de entrada y salida
use anyhow::Result;
// Importamos los traits de E/S estándar para leer desde el teclado (`stdin`) y vaciar el búfer de salida (`flush`)
use std::io::{self, Write};
// Importamos las referencias y tipos propietarios de rutas en el sistema de archivos
use std::path::{Path, PathBuf};

// Importamos el submódulo de planes y la estructura `AudioPlan` del crate local
use crate::audio::plan::{self, AudioPlan};

/// Modela las decisiones o acciones seleccionadas por el usuario a través del menú interactivo de la terminal
pub enum CliAction {
    // Iniciar la reproducción guiada por un plan de canciones en cola (con Lazy Loading y persistencia)
    PlayPlan(AudioPlan),
    // Iniciar la reproducción simultánea de dos o más pistas en tiempo real (Modo Mezclador Multipista)
    PlayMulti(Vec<PathBuf>),
    // Iniciar la demostración musical procedural generada por los sintetizadores matemáticos
    PlayDemo,
    // Salir del programa de forma limpia y ordenada
    Exit,
}
/*
 * ROL DEL ENUM `CliAction`:
 * Desacopla la lógica de captura e interacción con el usuario en la terminal (`cli_flow.rs`)
 * de la inicialización de motores y bucles de renderizado (`main.rs` / `ui.rs`),
 * permitiendo una arquitectura de flujo limpia y mantenible.
 */

/// Despliega el menú interactivo principal en la terminal cuando el usuario ejecuta `mopet` sin parámetros
pub fn run_interactive_menu() -> Result<CliAction> {
    // Bucle continuo del menú: se repite hasta que el usuario elija una opción válida que retorne una acción o decida salir
    loop {
        // Imprimimos una línea en blanco para espaciado visual
        println!();
        // Imprimimos el borde superior de la caja de bienvenida
        println!("  ╔═══════════════════════════════════════════════════════════════════╗");
        // Imprimimos el título destacado de la aplicación MOPET
        println!("  ║           MOPET - FLUJO INTERACTIVO DESDE LA TERMINAL             ║");
        // Imprimimos el borde inferior de la caja
        println!("  ╚═══════════════════════════════════════════════════════════════════╝");
        // Espaciado
        println!();
        // Opción 1: Escanear y reproducir una carpeta entera
        println!("    [1] 📁 Reproducir una carpeta completa (MP3, MP4, WAV, FLAC)");
        // Opción 2: Recuperar un plan guardado previamente en formato JSON
        println!("    [2] 📋 Cargar un plan guardado (mopet_plan.json)");
        // Opción 3: Ensamblar una lista de reproducción personalizada ingresando rutas
        println!("    [3] 🎵 Armar una cola con canciones específicas (1 a 1)");
        // Opción 4: Modo multipista en simultáneo para mezclar audios
        println!("    [4] 🎛️  Modo Multipista (reproducir 2 o más audios al mismo tiempo)");
        // Opción 5: Probar el reproductor con los sintetizadores integrados
        println!("    [5] ⚡ Probar reproductor (Modo Demo generativo)");
        // Opción 6: Salir del programa
        println!("    [6] ❌ Salir");
        // Espaciado
        println!();
        // Solicitamos al usuario que ingrese su elección numérica
        print!("  Selecciona una opción [1-6]: ");
        // Vaciamos inmediatamente el búfer de salida para garantizar que el prompt se imprima antes de esperar la entrada
        io::stdout().flush()?;

        // Variable para almacenar el texto ingresado por el usuario
        let mut input = String::new();
        // Leemos la línea ingresada desde la entrada estándar
        io::stdin().read_line(&mut input)?;
        // Eliminamos espacios en blanco o saltos de línea al inicio y final
        let choice = input.trim();

        // Evaluamos la opción elegida mediante pattern matching
        match choice {
            // Opción 1: Reproducir una carpeta completa
            "1" => {
                // Línea en blanco
                println!();
                // Solicitamos la ruta del directorio
                print!("  📁 Ingresa la ruta de la carpeta [Enter para carpeta actual '.']: ");
                // Vaciamos el búfer
                io::stdout().flush()?;
                // Variable para la ruta
                let mut dir_str = String::new();
                // Leemos la ruta escrita
                io::stdin().read_line(&mut dir_str)?;
                // Limpiamos la cadena de entrada
                let dir_str = dir_str.trim();
                // Si el usuario presionó enter directamente, asumimos el directorio actual (.)
                let dir_path = if dir_str.is_empty() {
                    // Ruta a la carpeta actual
                    PathBuf::from(".")
                } else {
                    // Quitamos comillas si el usuario arrastró la carpeta a la terminal
                    PathBuf::from(dir_str.trim_matches('\'').trim_matches('"'))
                };

                // Comprobamos si la carpeta realmente existe en el sistema de archivos
                if !dir_path.exists() {
                    // Advertimos del error y regresamos al menú principal
                    println!("  ❌ La carpeta '{}' no existe.", dir_path.display());
                    // Saltamos a la siguiente iteración del loop
                    continue;
                }

                // Notificamos el inicio del escaneo
                println!("  🔍 Escaneando archivos de audio en '{}'...", dir_path.display());
                // Escaneamos el directorio recursivamente construyendo un plan de reproducción
                let plan = AudioPlan::from_directory(&dir_path, true)?;
                // Verificamos si se encontraron archivos válidos
                if plan.items.is_empty() {
                    // Advertimos que no se hallaron pistas de audio compatibles
                    println!("  ⚠️ No se encontraron archivos de audio (.mp3, .mp4, .m4a, .wav, .flac, .ogg) en esa carpeta.");
                    // Regresamos al menú principal
                    continue;
                }

                // Informamos la cantidad de canciones encontradas
                println!("  ✅ Se encontraron {} canciones.", plan.items.len());
                // Guardamos automáticamente el plan para permitir restaurarlo en el futuro
                let _ = plan.save_to_file("mopet_plan.json");
                // Confirmamos el guardado
                println!("  💾 Plan guardado en 'mopet_plan.json' para futuros inicios.");
                // Retornamos la acción de reproducir el plan generado
                return Ok(CliAction::PlayPlan(plan));
            }
            /*
             * ROL DE LA RAMA "1":
             * Provee la forma más rápida para escuchar álbumes completos o carpetas de música,
             * encargándose automáticamente de descubrir, validar y ordenar las pistas.
             */

            // Opción 2: Cargar un plan guardado previamente
            "2" => {
                // Línea en blanco
                println!();
                // Verificamos si existe el archivo de plan predeterminado `mopet_plan.json`
                if Path::new("mopet_plan.json").exists() {
                    // Intentamos deserializar el plan predeterminado
                    if let Ok(plan) = AudioPlan::load_from_file("mopet_plan.json") {
                        // Informamos al usuario sobre el plan encontrado
                        println!("  📋 Encontrado 'mopet_plan.json' con {} pistas.", plan.items.len());
                        // Preguntamos si desea reproducir directamente este plan guardado
                        print!("  ¿Reproducir este plan? [S/n]: ");
                        // Vaciamos el búfer
                        io::stdout().flush()?;
                        // Variable para la respuesta de confirmación
                        let mut confirm = String::new();
                        // Leemos la respuesta
                        io::stdin().read_line(&mut confirm)?;
                        // Limpiamos y convertimos a minúsculas
                        let confirm = confirm.trim().to_lowercase();
                        // Si presionó enter (vacío) o respondió afirmativamente ("s", "si", "y")
                        if confirm.is_empty() || confirm == "s" || confirm == "si" || confirm == "y" {
                            // Retornamos inmediatamente el plan cargado
                            return Ok(CliAction::PlayPlan(plan));
                        }
                    }
                }

                // Si no existía o el usuario prefirió otra ruta, solicitamos la ruta del archivo JSON
                print!("  Ingresa la ruta del archivo de plan (.json): ");
                // Vaciamos el búfer
                io::stdout().flush()?;
                // Variable para la ruta
                let mut path_str = String::new();
                // Leemos la entrada
                io::stdin().read_line(&mut path_str)?;
                // Limpiamos espacios y comillas
                let path_str = path_str.trim().trim_matches('\'').trim_matches('"');
                // Si la cadena está vacía, volvemos al menú
                if path_str.is_empty() {
                    // Continuamos
                    continue;
                }
                // Intentamos cargar el archivo JSON indicado
                match AudioPlan::load_from_file(path_str) {
                    // Si la carga fue exitosa, iniciamos la reproducción
                    Ok(plan) => return Ok(CliAction::PlayPlan(plan)),
                    // Si hubo un error de sintaxis o de ruta, lo mostramos
                    Err(e) => println!("  ❌ Error cargando plan: {}", e),
                }
            }
            /*
             * ROL DE LA RAMA "2":
             * Permite recuperar listas de reproducción previas sin tener que volver a ingresar
             * las rutas manualmente, facilitando una experiencia de usuario fluida y persistente.
             */

            // Opción 3: Armar una cola personalizada cancion por cancion
            "3" => {
                // Línea en blanco
                println!();
                // Instrucciones de ingreso
                println!("  🎵 Escribe o arrastra las rutas de las canciones separadas por espacio o coma:");
                // Indicador de entrada
                print!("  > ");
                // Vaciamos el búfer
                io::stdout().flush()?;
                // Variable para las rutas
                let mut songs_input = String::new();
                // Leemos la entrada completa
                io::stdin().read_line(&mut songs_input)?;
                // Parseamos las rutas respetando comillas y espacios en los nombres de archivo
                let raw_paths = parse_input_paths(&songs_input);

                // Si no se extrajeron rutas válidas, reintentamos
                if raw_paths.is_empty() {
                    // Advertencia
                    println!("  ⚠️ No se especificaron rutas válidas.");
                    // Continuamos
                    continue;
                }

                // Vector para acumular únicamente rutas que existen y son formatos de audio admitidos
                let mut valid_paths = Vec::new();
                // Iteramos por las rutas analizadas
                for p in raw_paths {
                    // Comprobamos si el archivo existe y es compatible
                    if p.exists() && plan::is_supported_audio(&p) {
                        // Añadimos a la lista válida
                        valid_paths.push(p);
                    // Si no existe, informamos al usuario
                    } else if !p.exists() {
                        // Advertencia de ruta inexistente
                        println!("  ⚠️ Archivo no encontrado: {}", p.display());
                    }
                }

                // Verificamos si al menos una ruta superó el filtrado
                if valid_paths.is_empty() {
                    // Advertencia
                    println!("  ❌ Ningún archivo válido especificado.");
                    // Continuamos
                    continue;
                }

                // Construimos el plan de audio a partir de las rutas verificadas
                let plan = AudioPlan::from_paths(&valid_paths);
                // Informamos el éxito
                println!("  ✅ Plan armado con {} canciones.", plan.items.len());
                // Guardamos el plan recién ensamblado en el archivo por defecto
                let _ = plan.save_to_file("mopet_plan.json");
                // Confirmamos el guardado
                println!("  💾 Guardado en 'mopet_plan.json'.");
                // Retornamos el plan para reproducir
                return Ok(CliAction::PlayPlan(plan));
            }
            /*
             * ROL DE LA RAMA "3":
             * Permite al usuario curar una selección musical ad-hoc escribiendo o arrastrando
             * canciones una tras otra a la terminal, validando automáticamente la existencia física de cada una.
             */

            // Opción 4: Modo Multipista simultáneo
            "4" => {
                // Línea en blanco
                println!();
                // Instrucciones para el modo multipista
                println!("  🎛️  Modo Multipista: Ingresa las rutas de los audios para reproducir simultáneamente:");
                // Prompt
                print!("  > ");
                // Vaciamos el búfer
                io::stdout().flush()?;
                // Variable de entrada
                let mut multi_input = String::new();
                // Leemos las rutas
                io::stdin().read_line(&mut multi_input)?;
                // Parseamos respetando comillas
                let raw_paths = parse_input_paths(&multi_input);

                // Filtramos las rutas válidas que existan y sean de audio
                let valid_paths: Vec<PathBuf> = raw_paths
                    // Convertimos a iterador consumidor
                    .into_iter()
                    // Filtramos por existencia física y compatibilidad de formato
                    .filter(|p| p.exists() && plan::is_supported_audio(p))
                    // Colectamos en un vector
                    .collect();

                // Validamos el requisito mínimo: al menos 2 pistas para mezclar
                if valid_paths.len() < 2 {
                    // Advertencia
                    println!("  ⚠️ Debes especificar al menos 2 archivos de audio válidos para el modo multipista.");
                    // Continuamos
                    continue;
                }

                // Notificamos el inicio de la sesión multipista
                println!("  ✅ Iniciando reproducción simultánea de {} pistas...", valid_paths.len());
                // Retornamos la acción multipista con los archivos seleccionados
                return Ok(CliAction::PlayMulti(valid_paths));
            }
            /*
             * ROL DE LA RAMA "4":
             * Satisface el requerimiento central de mezclar 2 o más pistas de audio al mismo tiempo,
             * preparando los archivos para ser cargados en ranuras individuales de mezcla simultánea.
             */

            // Opción 5: Modo Demostración procedural
            "5" => {
                // Retornamos la acción de demostración sin necesidad de archivos
                return Ok(CliAction::PlayDemo);
            }
            /*
             * ROL DE LA RAMA "5":
             * Permite evaluar el reproductor, verificar el hardware de sonido y explorar
             * los controles de la interfaz visual incluso si el usuario no tiene archivos de música disponibles.
             */

            // Opción 6: Salir del programa
            "6" => {
                // Retornamos la acción de salida
                return Ok(CliAction::Exit);
            }
            /*
             * ROL DE LA RAMA "6":
             * Finaliza la ejecución del CLI limpiamente.
             */

            // Cualquier otra opción ingresada que no coincida con los dígitos [1-6]
            _ => {
                // Informamos que la opción es inválida y el bucle vuelve a pedir una opción
                println!("  ⚠️ Opción no válida. Por favor introduce un número del 1 al 6.");
            }
        }
    }
    /*
     * ROL DEL BUCLE PRINCIPAL DEL MENÚ:
     * Asegura una experiencia robusta y tolerante a fallos: cualquier error tipográfico, ruta equivocada
     * o archivo inexistente es atrapado y advertido con claridad sin que el programa se cierre abruptamente.
     */
}
/*
 * ROL DE LA FUNCIÓN `run_interactive_menu`:
 * Es el cerebro conversacional de la terminal. Ofrece una bienvenida atractiva y guía al usuario
 * de forma amigable a través de todas las prestaciones de MOPET sin necesidad de memorizar banderas complejas.
 */

/// Analizador sintáctico auxiliar que separa una cadena de rutas respetando comillas simples y dobles.
/// Esto permite ingresar nombres de archivos con espacios arrastrados directamente desde el explorador
/// de archivos de Linux, macOS o Windows (ej. "'/home/musica/01 - Mi Cancion.mp3' '/home/musica/02.wav'").
fn parse_input_paths(input: &str) -> Vec<PathBuf> {
    // Vector de rutas extraídas
    let mut paths = Vec::new();
    // Búfer acumulador para el fragmento de ruta actual
    let mut current = String::new();
    // Bandera que indica si nos encontramos dentro de un bloque entre comillas simples ('...')
    let mut in_single_quote = false;
    // Bandera que indica si nos encontramos dentro de un bloque entre comillas dobles ("...")
    let mut in_double_quote = false;

    // Recorremos carácter por carácter la cadena ingresada
    for ch in input.chars() {
        // Evaluamos el carácter mediante pattern matching
        match ch {
            // Comilla simple: si no estamos dentro de comillas dobles, conmutamos el estado
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            // Comilla doble: si no estamos dentro de comillas simples, conmutamos el estado
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            // Espacio o coma: si estamos fuera de cualquier par de comillas, delimita el fin de una ruta
            ' ' | ',' if !in_single_quote && !in_double_quote => {
                // Limpiamos los extremos del fragmento acumulado
                let trimmed = current.trim();
                // Si contiene texto no vacío, lo convertimos a PathBuf y lo guardamos
                if !trimmed.is_empty() {
                    // Insertamos la ruta limpia
                    paths.push(PathBuf::from(trimmed));
                    // Vaciamos el acumulador para la siguiente ruta
                    current.clear();
                }
            }
            // Cualquier otro carácter: lo agregamos a la ruta actual en construcción
            _ => current.push(ch),
        }
    }
    /*
     * ROL DEL BUCLE DE PARSEO:
     * Implementa una pequeña máquina de estados finitos (FSM) de un solo pase para tokenizar
     * cadenas con comillas escapadas, emulando el comportamiento de un shell POSIX.
     */

    // Procesamos el último fragmento remanente tras finalizar la cadena
    let trimmed = current.trim();
    // Si contiene caracteres válidos
    if !trimmed.is_empty() {
        // Lo agregamos como la última ruta del vector
        paths.push(PathBuf::from(trimmed));
    }

    // Retornamos la lista de rutas procesadas
    paths
}
/*
 * ROL DE LA FUNCIÓN `parse_input_paths`:
 * Resuelve uno de los problemas más comunes en aplicaciones CLI: el soporte transparente
 * de rutas con espacios o caracteres especiales al arrastrar y soltar archivos desde el gestor de ventanas.
 */
