// Permitimos código no utilizado en esta etapa didáctica si algún método no se llama directamente en el main
#![allow(dead_code)]

// Importamos el trait Context y el tipo Result del crate anyhow para manejo idiomático de errores con contexto
use anyhow::{Context, Result};
// Importamos los traits de derivación Serialize y Deserialize del crate Serde para conversión automática a JSON
use serde::{Deserialize, Serialize};
// Importamos utilidades de manipulación del sistema de archivos e inspectores de archivos del módulo estándar
use std::fs::{self, File};
// Importamos buffers de lectura y escritura (`BufReader`, `BufWriter`) para optimizar operaciones de entrada/salida en disco
use std::io::{BufReader, BufWriter};
// Importamos referencias y rutas dueñas del sistema de archivos (`Path` y `PathBuf`)
use std::path::{Path, PathBuf};

/// Representa un elemento individual o canción dentro del plan de reproducción.
/// Almacena la ruta en el disco y un título legible para mostrar en la interfaz de usuario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    // Ruta absoluta o relativa al archivo en el disco (ej. /musica/rock/tema1.mp3)
    pub path: PathBuf,
    // Nombre o título descriptivo de la pista para la interfaz gráfica o terminal
    pub title: String,
}
/*
 * ROL DE LA ESTRUCTURA `PlanItem`:
 * Modela un nodo de datos mínimo y serializable que representa un archivo de audio en cola.
 * Al almacenar únicamente la ruta y el título (metadatos ligeros), no carga las muestras de audio
 * en memoria RAM hasta que la canción vaya a sonar efectivamente (base del patrón Lazy Loading).
 */

/// Define los modos de repetición admitidos por la cola de reproducción del reproductor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RepeatMode {
    // Modo predeterminado: reproduce una pista tras otra en orden secuencial y se detiene al llegar a la última
    #[default]
    Sequential,
    // Modo bucle de lista: al finalizar la última pista, reinicia la reproducción automáticamente desde la primera
    LoopAll,
    // Modo bucle de canción: repite indefinidamente la pista actual sin avanzar a la siguiente
    LoopSingle,
}
/*
 * ROL DEL ENUM `RepeatMode`:
 * Controla la máquina de estados de transición entre canciones cuando una pista alcanza su final.
 * Al implementar Serialize/Deserialize y Default, se guarda de forma transparente en el archivo `mopet_plan.json`.
 */

impl RepeatMode {
    /// Alterna cíclicamente al siguiente modo de reproducción disponible al presionar la tecla correspondiente (T)
    pub fn next_mode(&self) -> Self {
        // Evaluamos el estado actual del modo de repetición mediante pattern matching exhaustivo
        match self {
            // De Secuencial pasa a Repetir Todo el Plan
            RepeatMode::Sequential => RepeatMode::LoopAll,
            // De Repetir Todo pasa a Repetir la Misma Canción
            RepeatMode::LoopAll => RepeatMode::LoopSingle,
            // De Repetir Canción vuelve al modo Secuencial estándar
            RepeatMode::LoopSingle => RepeatMode::Sequential,
        }
    }
    /*
     * ROL DEL MÉTODO `next_mode`:
     * Permite una alternancia cíclica limpia y sin estado mutable entre los tres modos soportados por la UI.
     */

    /// Retorna una etiqueta textual informativa en español adecuada para mostrar en la barra de estado de la terminal
    pub fn label(&self) -> &'static str {
        // Emparejamos cada variante con su texto descriptivo correspondiente
        match self {
            // Texto para modo secuencial
            RepeatMode::Sequential => "SECUENCIAL (1 a 1)",
            // Texto para bucle de lista
            RepeatMode::LoopAll => "REPETIR PLAN (Loop)",
            // Texto para repetición individual
            RepeatMode::LoopSingle => "REPETIR CANCIÓN (1)",
        }
    }
    /*
     * ROL DEL MÉTODO `label`:
     * Traduce el estado interno del enum a una cadena estática legible para el usuario en la TUI.
     */
}

/// Estructura central que gestiona la lista de canciones en cola, el índice actual y la persistencia en JSON
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioPlan {
    // Nombre identificativo del plan de reproducción (ej. "Álbum Favorito" o "Carpeta: Rock")
    pub name: String,
    // Vector dinámico con la lista ordenada de elementos o canciones que componen el plan
    pub items: Vec<PlanItem>,
    // Índice de la canción que se está reproduciendo actualmente (basado en 0)
    pub current_index: usize,
    // Modo de repetición configurado para este plan
    pub repeat_mode: RepeatMode,
}
/*
 * ROL DE LA ESTRUCTURA `AudioPlan`:
 * Es el contenedor de estado de la lista de reproducción. Contiene los metadatos necesarios
 * para reproducir colecciones enteras de música sin saturar la memoria RAM.
 */

#[allow(dead_code)]
impl AudioPlan {
    /// Crea un nuevo plan de reproducción vacío con un nombre asignado
    pub fn new(name: String) -> Self {
        // Instanciamos el plan con valores iniciales limpios
        Self {
            // Asignamos el nombre recibido
            name,
            // Inicializamos la lista de canciones como un vector vacío
            items: Vec::new(),
            // El índice apunta al inicio (pista 0)
            current_index: 0,
            // Establecemos el modo secuencial por defecto
            repeat_mode: RepeatMode::Sequential,
        }
    }
    /*
     * ROL DEL CONSTRUCTOR `new`:
     * Proporciona un inicializador simple para construir planes manualmente en memoria.
     */

    /// Construye un plan de reproducción a partir de una lista de rutas de archivos pasadas como parámetro
    pub fn from_paths<P: AsRef<Path>>(paths: &[P]) -> Self {
        // Creamos un vector temporal donde acumularemos los elementos válidos
        let mut items = Vec::new();
        // Iteramos por cada una de las referencias de ruta recibidas
        for p in paths {
            // Obtenemos una referencia a la estructura `Path` subyacente
            let path = p.as_ref();
            // Verificamos si la extensión del archivo corresponde a un formato soportado (MP3, MP4, WAV, FLAC, etc.)
            if is_supported_audio(path) {
                // Extraemos el nombre del archivo sin directorios padre para usarlo como título legible
                let title = path
                    // Obtenemos el nombre base del archivo
                    .file_name()
                    // Lo convertimos a una cadena UTF-8 si es válido
                    .and_then(|n| n.to_str())
                    // Si no pudiera leerse, usamos el texto genérico "Pista"
                    .unwrap_or("Pista")
                    // Convertimos la referencia a un String dueño
                    .to_string();
                // Añadimos el nuevo elemento a la lista del plan
                items.push(PlanItem {
                    // Clonamos la ruta hacia un `PathBuf` propio
                    path: path.to_path_buf(),
                    // Asignamos el título extraído
                    title,
                });
            }
        }
        // Retornamos la estructura `AudioPlan` inicializada con las pistas detectadas
        Self {
            // Asignamos un nombre por defecto para este plan
            name: "Plan de Reproducción".to_string(),
            // Asignamos la lista de canciones filtradas
            items,
            // Comenzamos en la primera pista
            current_index: 0,
            // Modo secuencial por defecto
            repeat_mode: RepeatMode::Sequential,
        }
    }
    /*
     * ROL DEL MÉTODO `from_paths`:
     * Permite instanciar un plan directamente a partir de archivos seleccionados por el usuario
     * en el menú o pasados como argumentos posicionales desde la línea de comandos.
     */

    /// Escanea un directorio en busca de todos los archivos de audio soportados (.mp3, .mp4, .m4a, .wav, .flac, .ogg),
    /// con opción de búsqueda recursiva en subcarpetas.
    pub fn from_directory<P: AsRef<Path>>(dir: P, recursive: bool) -> Result<Self> {
        // Convertimos la entrada a una referencia genérica de `Path`
        let dir = dir.as_ref();
        // Inicializamos el vector colector de elementos del plan
        let mut items = Vec::new();
        // Ejecutamos la recolección recursiva o superficial en el sistema de archivos
        collect_audio_files(dir, recursive, &mut items)?;

        // Ordenamos alfabéticamente las pistas por su título (ignorando mayúsculas/minúsculas)
        items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

        // Obtenemos el nombre de la carpeta para bautizar al plan de manera amigable
        let folder_name = dir
            // Obtenemos el nombre del directorio
            .file_name()
            // Convertimos el OsStr a &str
            .and_then(|n| n.to_str())
            // Valor por defecto en caso de no poder leer el nombre
            .unwrap_or("Carpeta")
            // Convertimos a String
            .to_string();

        // Construimos y retornamos el plan de la carpeta
        Ok(Self {
            // Nombre descriptivo con el nombre de la carpeta
            name: format!("Carpeta: {}", folder_name),
            // Canciones encontradas y ordenadas
            items,
            // Puntero en la primera canción
            current_index: 0,
            // Modo secuencial
            repeat_mode: RepeatMode::Sequential,
        })
    }
    /*
     * ROL DEL MÉTODO `from_directory`:
     * Automatiza la creación masiva de planes de reproducción al apuntar a cualquier carpeta de música.
     * Soporta escaneo profundo en jerarquías de directorios (árboles de carpetas) y organiza las pistas en orden alfabético.
     */

    /// Guarda el estado actual del plan en un archivo en disco utilizando formato JSON con sangría legible
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        // Obtenemos la referencia de ruta del archivo de destino
        let path = path.as_ref();
        // Creamos o sobreescribimos el archivo en el sistema de archivos
        let file = File::create(path)
            // Añadimos contexto al error en caso de fallo de permisos o ruta inválida
            .with_context(|| format!("No se pudo guardar el plan en {}", path.display()))?;
        // Envolvemos el archivo en un `BufWriter` para minimizar llamadas al sistema operativo al escribir datos
        let writer = BufWriter::new(file);
        // Serializamos la estructura completa `AudioPlan` al archivo JSON en formato formateado ("pretty")
        serde_json::to_writer_pretty(writer, self)
            // Añadimos contexto al posible error de serialización
            .with_context(|| "Error serializando plan a formato JSON")?;
        // Notificamos que la operación culminó con éxito
        Ok(())
    }
    /*
     * ROL DEL MÉTODO `save_to_file`:
     * Implementa la persistencia de planes de reproducción solicitada por el usuario (`mopet_plan.json`),
     * permitiendo cerrar el reproductor y recuperar la lista completa en futuras sesiones de escucha.
     */

    /// Carga y deserializa un plan de reproducción previamente guardado en formato JSON desde el disco
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        // Obtenemos la referencia al path
        let path = path.as_ref();
        // Abrimos el archivo en modo de solo lectura
        let file = File::open(path)
            // Contexto de error si el archivo no existe o no tiene permisos de lectura
            .with_context(|| format!("No se pudo abrir el plan en {}", path.display()))?;
        // Envolvemos el descriptor de archivo en un `BufReader` para lecturas bufferizadas de alta velocidad
        let reader = BufReader::new(file);
        // Deserializamos el contenido JSON directamente a la estructura `AudioPlan`
        let plan: Self = serde_json::from_reader(reader)
            // Contexto de error si la sintaxis JSON está corrupta o no coincide con los campos esperados
            .with_context(|| format!("Error leyendo formato JSON del plan {}", path.display()))?;
        // Retornamos el plan recuperado
        Ok(plan)
    }
    /*
     * ROL DEL MÉTODO `load_from_file`:
     * Lee y reconstruye instantáneamente toda la cola de reproducción guardada previamente en disco,
     * verificando la integridad del formato mediante el motor de deserialización de Serde.
     */

    /// Retorna una referencia al elemento que se encuentra activo según el índice actual
    pub fn current_item(&self) -> Option<&PlanItem> {
        // Accedemos de manera segura al vector de elementos mediante `.get()` para prevenir pánicos fuera de rango
        self.items.get(self.current_index)
    }
    /*
     * ROL DEL MÉTODO `current_item`:
     * Permite consultar la pista que debería estar sonando en el momento presente sin modificar el estado del plan.
     */

    /// Avanza a la siguiente pista de acuerdo a la política establecida en `repeat_mode`.
    /// Retorna `None` si se alcanzó el final de la lista en modo secuencial.
    pub fn next(&mut self) -> Option<&PlanItem> {
        // Si el plan no tiene pistas, no hay siguiente elemento disponible
        if self.items.is_empty() {
            // Devolvemos None
            return None;
        }

        // Aplicamos la lógica de navegación según el modo de repetición configurado
        match self.repeat_mode {
            // En modo repetición individual, nos mantenemos en la misma pista
            RepeatMode::LoopSingle => self.items.get(self.current_index),
            // En modo secuencial, avanzamos solo si no estamos en la última canción
            RepeatMode::Sequential => {
                // Verificamos si aún quedan canciones por delante
                if self.current_index + 1 < self.items.len() {
                    // Incrementamos el índice a la siguiente pista
                    self.current_index += 1;
                    // Retornamos la nueva pista activa
                    self.items.get(self.current_index)
                } else {
                    // Llegamos al final de la cola: retornamos None para detener la reproducción
                    None
                }
            }
            // En modo bucle de lista completa, usamos aritmética modular (%) para volver a la pista 0 al terminar
            RepeatMode::LoopAll => {
                // Calculamos el siguiente índice circular
                self.current_index = (self.current_index + 1) % self.items.len();
                // Retornamos el elemento correspondiente
                self.items.get(self.current_index)
            }
        }
    }
    /*
     * ROL DEL MÉTODO `next`:
     * Es el motor de avance de la cola. Determina matemáticamente cuál debe ser la siguiente canción
     * cuando una pista finaliza de forma natural o cuando el usuario presiona la tecla de siguiente (`N`).
     */

    /// Retrocede a la pista anterior en la cola respetando los límites de la lista
    pub fn prev(&mut self) -> Option<&PlanItem> {
        // Verificamos si la lista está vacía
        if self.items.is_empty() {
            // Si no hay pistas, no hay retroceso posible
            return None;
        }

        // Si no estamos en la primera pista, simplemente decrementamos el índice
        if self.current_index > 0 {
            // Retrocedemos una posición
            self.current_index -= 1;
        // Si estamos en la primera pista y el modo es LoopAll, saltamos a la última canción
        } else if self.repeat_mode == RepeatMode::LoopAll {
            // Saltamos a la última posición del vector
            self.current_index = self.items.len() - 1;
        }

        // Devolvemos una referencia a la pista seleccionada tras el retroceso
        self.items.get(self.current_index)
    }
    /*
     * ROL DEL MÉTODO `prev`:
     * Permite al usuario retroceder a la pista musical anterior (tecla `B`), implementando retorno circular
     * si el modo de repetición `LoopAll` está activo.
     */

    /// Permite saltar directamente a una canción específica indicando su índice numérico
    pub fn set_index(&mut self, idx: usize) -> Option<&PlanItem> {
        // Validamos que el índice solicitado esté dentro de los límites del vector
        if idx < self.items.len() {
            // Actualizamos el índice actual
            self.current_index = idx;
            // Devolvemos el elemento en esa posición
            self.items.get(self.current_index)
        } else {
            // Si el índice está fuera de rango, retornamos None
            None
        }
    }
    /*
     * ROL DEL MÉTODO `set_index`:
     * Facilita el salto directo a cualquier pista de la lista sin tener que avanzar de una en una.
     */
}

/// Comprueba si la extensión del archivo corresponde a un formato de audio soportado por Symphonia / MOPET
pub fn is_supported_audio(path: &Path) -> bool {
    // Extraemos la extensión del archivo en minúsculas para comparación uniforme
    let ext = path
        // Obtenemos la extensión del sistema de archivos
        .extension()
        // Convertimos a &str
        .and_then(|e| e.to_str())
        // Pasamos toda la cadena a minúsculas para ignorar diferencias como .MP3 vs .mp3
        .map(|s| s.to_lowercase());

    // Evaluamos contra los formatos decodificables por la arquitectura de MOPET
    matches!(
        // Desreferenciamos el Option<String> a Option<&str>
        ext.as_deref(),
        // Aceptamos MP3, MP4/M4A (AAC/ALAC), WAV (PCM sin compresión), FLAC (Lossless), OGG (Vorbis), AAC
        Some("mp3") | Some("mp4") | Some("m4a") | Some("wav") | Some("flac") | Some("ogg") | Some("aac")
    )
}
/*
 * ROL DE LA FUNCIÓN `is_supported_audio`:
 * Actúa como filtro validador para admitir exclusivamente extensiones decodificables por Symphonia.
 * Garantiza que archivos de texto, imágenes o videos no compatibles sean ignorados al escanear carpetas.
 */

/// Función recursiva auxiliar que recorre el árbol de directorios recolectando todos los archivos de audio válidos
fn collect_audio_files(dir: &Path, recursive: bool, items: &mut Vec<PlanItem>) -> Result<()> {
    // Si la ruta recibida no es un directorio válido, salimos inmediatamente sin error
    if !dir.is_dir() {
        // Retornamos Ok(())
        return Ok(());
    }

    // Leemos las entradas presentes en el directorio
    for entry in fs::read_dir(dir)? {
        // Desenvolvemos la entrada de directorio individual
        let entry = entry?;
        // Obtenemos la ruta absoluta o relativa de la entrada
        let path = entry.path();

        // Si es un archivo regular y tiene extensión de audio soportada, lo incorporamos al plan
        if path.is_file() && is_supported_audio(&path) {
            // Extraemos el nombre del archivo para asignarlo como título
            let title = path
                // Obtenemos el nombre base
                .file_name()
                // Convertimos a &str
                .and_then(|n| n.to_str())
                // Título por defecto si fallara la conversión
                .unwrap_or("Pista")
                // Convertimos a String
                .to_string();
            // Insertamos la nueva canción encontrada en el vector de resultados
            items.push(PlanItem { path, title });
        // Si es un subdirectorio y se activó la opción recursiva, exploramos dentro de él
        } else if path.is_dir() && recursive {
            // Llamada recursiva a la misma función para explorar el subdirectorio hijo
            let _ = collect_audio_files(&path, true, items);
        }
    }
    // Finalización exitosa del escaneo
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `collect_audio_files`:
 * Explora exhaustivamente el árbol de archivos en el disco duro de manera recursiva,
 * permitiendo importar discografías o librerías completas de música con un solo comando.
 */
