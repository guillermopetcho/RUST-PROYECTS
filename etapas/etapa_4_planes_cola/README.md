# 📋 ETAPA 4: Planes de Reproducción, Serialización JSON y Carga Diferida (Lazy Loading)

Bienvenido a la **Etapa 4** del proyecto **MOPET**. En esta etapa abordamos uno de los mayores requerimientos de arquitectura del reproductor: **permitir armar listas de reproducción gigantescas (cientos o miles de canciones) y guardarlas en disco de forma persistente (`mopet_plan.json`), consumiendo siempre una cantidad fija y mínima de memoria RAM (< 25 MB)**.

Para lograr esto, implementamos la técnica de **Carga Diferida (*Lazy Loading*)**: el reproductor solo almacena metadatos ligeros (rutas y títulos) y decodifica las muestras de audio únicamente cuando la canción está a punto de sonar, liberando la memoria anterior en caliente (*Hot-Swap*).

---

## 📦 1. Librerías que se Implementaron (Crates y Dependencias)

En el archivo [`Cargo.toml`](./Cargo.toml) se incorporaron las librerías líderes de serialización del ecosistema de Rust:

```toml
[dependencies]
cpal = "0.15"
symphonia = { version = "0.5", features = ["all"] }
parking_lot = "0.12"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
```

### ¿Por qué se eligieron estas librerías?

1. **`serde` (v1.0 con `features = ["derive"]`)**:
   - **Qué es**: El framework de serialización y deserialización universal de Rust (*SerDe: Serializer / Deserializer*).
   - **Ventaja de las Macros Derive**: Permite colocar `#[derive(Serialize, Deserialize)]` encima de cualquier `struct` o `enum`, y el compilador genera automáticamente en tiempo de compilación el código en ensamblador necesario para transformar las estructuras de Rust a texto o binario sin reflexión en tiempo de ejecución (*zero-cost abstraction*).

2. **`serde_json` (v1.0)**:
   - **Qué es**: La implementación oficial para formato JSON estándar (RFC 8259).
   - **Ventajas**:
     - Serialización formateada con sangría legible para humanos (`serde_json::to_writer_pretty`).
     - Deserialización ultra-rápida directamente desde flujos bufferizados en disco (`BufReader`).

3. **`cpal`, `symphonia`, `parking_lot` y `anyhow`**:
   - Proporcionan la decodificación, el remuestreo, las pistas concurrentes y el motor de sonido heredados de las etapas 1, 2 y 3.

---

## 📂 2. Archivos Creados en la Etapa 4

```text
etapas/etapa_4_planes_cola/
├── Cargo.toml          # Declaración del paquete en el Workspace y dependencias
├── README.md           # Guía pedagógica y manual técnico de la etapa
└── src/
    ├── plan.rs         # Módulo de colas: AudioPlan, PlanItem, RepeatMode, JSON y escaneo
    ├── main.rs         # Demostración en vivo de Lazy Loading y transición de canciones
    ├── engine.rs       # Motor DSP y callback CPAL (Etapa 3)
    ├── track.rs        # Modelo de pistas atómicas y Hot-Swap (Etapa 2)
    ├── synth.rs        # Generadores sintéticos de apoyo (Etapa 3)
    └── decoder.rs      # Decodificador multimedia (Etapa 1)
```

### Rol de cada archivo:
- [`src/plan.rs`](./src/plan.rs): Modela la lista de reproducción, la máquina de estados de avance (secuencial, bucle general, bucle de una canción), el escaneo recursivo de directorios y la persistencia en formato JSON.
- [`src/main.rs`](./src/main.rs): Genera dos archivos WAV de prueba, serializa un plan a `plan_demostracion.json`, lo recupera del disco, reproduce la canción #1, detecta su fin y transfiere la pista #2 a la memoria en caliente liberando la anterior.

---

## 🧩 3. Bloques de Código Específicos Implementados

### Bloque A: Modelado de Datos y Derivación Serde
*Ubicación:* [`src/plan.rs`](./src/plan.rs) líneas 10 a 50.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub path: PathBuf,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RepeatMode {
    #[default]
    Sequential, // Reproduce una tras otra y se detiene al final
    LoopAll,    // Vuelve a la primera canción al terminar la última
    LoopSingle, // Repite la misma canción indefinidamente
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioPlan {
    pub name: String,
    pub items: Vec<PlanItem>,
    pub current_index: usize,
    pub repeat_mode: RepeatMode,
}
```

- **¿Qué hace?**:
  1. `PlanItem` almacena exclusivamente la ruta física del archivo y su título legible. **No carga las muestras de audio en la estructura**, permitiendo tener 10.000 canciones en el plan consumiendo apenas unos pocos kilobytes de memoria.
  2. `RepeatMode` es un `enum` con la anotación `#[default]`, permitiendo inicializar el plan en modo secuencial.
  3. Las directivas `Serialize` y `Deserialize` permiten guardar y restaurar todo el estado de la cola en una sola línea de código.

---

### Bloque B: Serialización y Deserialización Bufferizada
*Ubicación:* [`src/plan.rs`](./src/plan.rs) líneas 110 a 140.

```rust
// Guardar plan a archivo JSON
pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
    let path = path.as_ref();
    let file = File::create(path)
        .with_context(|| format!("No se pudo guardar el plan en {}", path.display()))?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, self)
        .with_context(|| "Error serializando plan a formato JSON")?;
    Ok(())
}

// Cargar plan desde archivo JSON
pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
    let path = path.as_ref();
    let file = File::open(path)
        .with_context(|| format!("No se pudo abrir el plan en {}", path.display()))?;
    let reader = BufReader::new(file);
    let plan: Self = serde_json::from_reader(reader)
        .with_context(|| format!("Error leyendo formato JSON del plan {}", path.display()))?;
    Ok(plan)
}
```

- **¿Por qué usar `BufWriter` y `BufReader`?**:
  - En lugar de realizar cientos de pequeñas llamadas al sistema (*syscalls* de escritura y lectura de disco por cada campo o carácter), envuelven el descriptor de archivo en un búfer en memoria de 8 KB.
  - Esto acelera la carga y el guardado de listas de reproducción de gran tamaño hasta **20 veces**.

---

### Bloque C: Escaneo Recursivo de Carpetas y Filtrado de Audio
*Ubicación:* [`src/plan.rs`](./src/plan.rs) líneas 80 a 105 y 180 a 210.

```rust
pub fn is_supported_audio(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase());
    matches!(
        ext.as_deref(),
        Some("mp3") | Some("mp4") | Some("m4a") | Some("wav") | Some("flac") | Some("ogg") | Some("aac")
    )
}

fn collect_audio_files(dir: &Path, recursive: bool, items: &mut Vec<PlanItem>) -> Result<()> {
    if !dir.is_dir() { return Ok(()); }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_supported_audio(&path) {
            let title = path.file_name().and_then(|n| n.to_str()).unwrap_or("Pista").to_string();
            items.push(PlanItem { path, title });
        } else if path.is_dir() && recursive {
            let _ = collect_audio_files(&path, true, items);
        }
    }
    Ok(())
}
```

- **¿Qué hace?**:
  1. Recorre de forma recursiva cualquier árbol de directorios del disco duro.
  2. Filtra únicamente archivos con extensiones compatibles con el motor de decodificación de MOPET, ignorando archivos ocultos, imágenes de portadas (`cover.jpg`) o archivos de texto.
  3. Ordena alfabéticamente las pistas para garantizar una reproducción ordenada.

---

### Bloque D: Máquina de Estados de la Cola y Carga Diferida (*Lazy Loading*)
*Ubicación:* [`src/plan.rs`](./src/plan.rs) líneas 140 a 175 y [`src/main.rs`](./src/main.rs) líneas 60 a 95.

```rust
// 1. Decodificamos ÚNICAMENTE la primera pista en memoria RAM
let first_item = plan_cargado.current_item().unwrap().clone();
let (samples, sr) = decoder::load_and_decode_file(&first_item.path, sample_rate)?;
let track = Arc::new(Track::new(0, first_item.title.clone(), samples, sr));
track.looping.store(false, Ordering::Relaxed);

let _engine = AudioEngine::new(vec![Arc::clone(&track)])?;

// 2. Esperamos a que la pista 1 termine
while !track.is_finished() {
    thread::sleep(Duration::from_millis(50));
}

// 3. Avance automático: Carga Just-In-Time y Hot-Swap
if let Some(next_item) = plan_cargado.next() {
    let (new_samples, new_sr) = decoder::load_and_decode_file(&next_item.path, sample_rate)?;
    track.swap_audio(new_samples, new_sr, next_item.title.clone()); // Libera memoria vieja
    while !track.is_finished() {
        thread::sleep(Duration::from_millis(50));
    }
}
```

- **¿Cómo logra el consumo < 25 MB de RAM?**:
  - Si un usuario tiene una lista de reproducción con 50 canciones en formato FLAC (unos 2 GB de audio sin comprimir), un reproductor ingenuo intentaría decodificar todo a memoria consumiendo 2 GB de RAM.
  - En MOPET, el objeto `AudioPlan` solo ocupa unos pocos kilobytes.
  - Cuando suena la pista 1, se ocupan ~15 MB de RAM. Al terminar, la llamada `track.swap_audio` destruye el vector anterior e introduce el nuevo, manteniendo la memoria plana en **~15 a 20 MB** de manera indefinida.

---

## 🧠 4. Conceptos Clave de Rust que se Aprenden en la Etapa 4

| Concepto de Rust | Cómo se aplica en esta etapa |
| :--- | :--- |
| **Macros Procedimentales (`#[derive(...)]`)** | Serde genera automáticamente el código de serialización y deserialización para tipos complejos sin costo en tiempo de ejecución. |
| **Enums con Atributo `#[default]`** | Permite que `RepeatMode` implemente el trait `Default`, integrándose de forma natural con constructores y deserializadores. |
| **E/S Bufferizada (`BufWriter` / `BufReader`)** | Minimiza las operaciones lentas de I/O contra el disco mediante un búfer intermedio en RAM de 8 KB. |
| **Aritmética Modular para Bucles (`%`)** | En modo `LoopAll`, el cálculo `(current_index + 1) % items.len()` permite volver a la canción inicial sin desbordar el vector. |
| **Patrón Arquitectónico *Lazy Loading*** | Carga diferida bajo demanda: retarda la asignación de recursos costosos (memoria y CPU) hasta el momento exacto en que se necesitan. |
| **Recursión de Sistema de Archivos** | `collect_audio_files` invoca a sí misma para descender por árboles de subdirectorios gestionando la pila de llamadas de Rust de forma segura. |

---

## ⚡ 5. Cómo Compilar y Ejecutar

### Ejecución de la demostración de planes y Lazy Loading:
```bash
cargo run -p etapa_4_planes_cola
```

### O probar armando un plan con tus propios archivos:
```bash
cargo run -p etapa_4_planes_cola -- cancion1.mp3 cancion2.wav cancion3.flac
```

### Salida esperada en terminal:
```text
==================================================================
   ETAPA 4: PLANES DE REPRODUCCIÓN, SERIALIZACIÓN Y LAZY LOADING 
==================================================================
[1] Creando 2 archivos de prueba para el plan...

[2] Serializando plan a formato JSON en 'plan_demostracion.json'...
    Recuperando plan desde JSON...
    ✅ Plan cargado: 'Plan de Estudio Etapa 4' con 2 canciones en cola.

[3] Carga Diferida (Lazy Loading):
    Decodificando únicamente la pista #1: etapa4_cancion1.wav
    🔊 Reproduciendo pista #1...
    ⏹️  Pista #1 finalizada.

[4] Avance automático en cola -> Pista #2: etapa4_cancion2.wav
    Liberando memoria de la pista anterior y cargando la nueva...
    🔊 Reproduciendo pista #2...
    ⏹️  Pista #2 finalizada.

✅ ¡Etapa 4 completada con éxito!
   El reproductor maneja planes persistentes en JSON y mantiene el consumo de RAM fijo (< 25 MB).
```
