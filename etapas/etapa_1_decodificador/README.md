# 🎵 ETAPA 1: Fundamentos de Rust y Decodificación Multimedia de Audio

Bienvenido a la **Etapa 1** del proyecto **MOPET**. En esta etapa se construyen los cimientos del reproductor: la capacidad de abrir flujos binarios desde el disco, identificar formatos multimedia (MP3, MP4 con AAC, WAV, FLAC, OGG), decodificar los paquetes comprimidos a muestras puras de punto flotante (`f32`) y remuestrear la señal a la frecuencia del hardware.

---

## 📦 1. Librerías que se Implementaron (Crates y Dependencias)

En el archivo [`Cargo.toml`](./Cargo.toml) se integraron dos dependencias fundamentales del ecosistema moderno de Rust:

```toml
[dependencies]
symphonia = { version = "0.5", features = ["all"] }
anyhow = "1.0"
```

### ¿Por qué se eligieron estas librerías?

1. **`symphonia` (v0.5 con `features = ["all"]`)**:
   - **Qué es**: La biblioteca líder en Rust para lectura, demultiplexación (demux) y decodificación de audio, escrita en código 100% Rust puro (sin bindings inseguros de C/C++ ni dependencias externas de sistema como FFmpeg o libasound).
   - **Formatos habilitados**:
     - *Contenedores*: MP4/M4A (`symphonia-format-isomp4`), RIFF/WAV, Ogg, FLAC, MP3 (`symphonia-bundle-mp3`).
     - *Códecs*: AAC (`symphonia-codec-aac`), PCM lineal, FLAC, Vorbis, MP3 (Layer I/II/III).
   - **Filosofía de memoria**: Diseñada con principios de cero copias (*zero-copy*), asignando buffers contiguos de alto rendimiento.

2. **`anyhow` (v1.0)**:
   - **Qué es**: Librería estándar de la industria para manejo idiomático de errores en aplicaciones binarias.
   - **Ventaja**: Proporciona el tipo `anyhow::Result<T>` y el trait `Context`, permitiendo adjuntar mensajes explicativos (ej. `.with_context(|| "No se pudo abrir el archivo")`) sin necesidad de crear decenas de estructuras de error personalizadas con `enum`.

---

## 📂 2. Archivos Creados en la Etapa 1

```text
etapas/etapa_1_decodificador/
├── Cargo.toml          # Declaración del paquete en el Workspace y sus dependencias
├── README.md           # Guía pedagógica y manual técnico de la etapa
└── src/
    ├── decoder.rs      # Módulo núcleo: Probing, decodificación por paquetes y remuestreo lineal
    └── main.rs         # Binario de prueba: Síntesis de WAV en bytes, decodificación y métricas
```

### Rol de cada archivo:
- [`Cargo.toml`](./Cargo.toml): Declara el crate `etapa_1_decodificador` como miembro del workspace, garantizando compilación aislada pero reutilizando la caché global.
- [`src/decoder.rs`](./src/decoder.rs): Módulo reutilizable que no depende de interfaces gráficas. Expone `load_and_decode_file` y `resample_stereo_linear`.
- [`src/main.rs`](./src/main.rs): Punto de entrada interactivo. Si no se le pasan argumentos, sintetiza un archivo WAV de 440 Hz en `/tmp`, lo decodifica, analiza su nivel de pico y mide su duración en segundos.

---

## 🧩 3. Bloques de Código Específicos Implementados

A continuación se detallan los 4 bloques clave de la Etapa 1 con su explicación técnica:

### Bloque A: Inspección y Apertura Multimedia (*Probing & Codec Setup*)
*Ubicación:* [`src/decoder.rs`](./src/decoder.rs) líneas 35 a 105.

```rust
pub fn load_and_decode_file<P: AsRef<Path>>(
    path: P,
    target_sample_rate: u32,
) -> Result<(Vec<f32>, u32)> {
    let path = path.as_ref();
    let file = File::open(path)
        .with_context(|| format!("No se pudo abrir el archivo {}", path.display()))?;
    
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let meta_opts = MetadataOptions::default();
    let fmt_opts = FormatOptions::default();
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)?;
    let mut format = probed.format;
    // ...
```

- **¿Qué hace?**:
  1. Acepta cualquier tipo de ruta (`&str`, `String`, `PathBuf`) mediante el trait genérico `P: AsRef<Path>`.
  2. Abre el descriptor del archivo y lo envuelve en un `MediaSourceStream`, que abstrae la lectura en disco en bloques optimizados.
  3. Crea un `Hint` con la extensión del archivo para acelerar la detección del contenedor.
  4. Invoca al `get_probe()` de Symphonia para inspeccionar los "números mágicos" de la cabecera del archivo.

---

### Bloque B: Filtrado de Pistas y Decodificación por Paquetes (*Demuxing Loop*)
*Ubicación:* [`src/decoder.rs`](./src/decoder.rs) líneas 106 a 180.

```rust
// Localizamos la primera pista válida de audio (ignorando video en archivos MP4)
let track = format
    .tracks()
    .iter()
    .find(|t| t.codec_params.codec != CODEC_TYPE_NULL && t.codec_params.channels.map_or(0, |c| c.count()) > 0)
    .context("No se encontró ninguna pista de audio válida en el contenedor")?;

let track_id = track.id;
let original_sr = track.codec_params.sample_rate.unwrap_or(target_sample_rate);
let mut decoder = symphonia::default::get_codecs()
    .make(&track.codec_params, &DecoderOptions::default())?;

let mut raw_samples = Vec::new();

while let Ok(packet) = format.next_packet() {
    if packet.track_id() != track_id {
        continue; // Ignora paquetes de video o subtítulos en archivos .mp4
    }
    match decoder.decode(&packet) {
        Ok(decoded) => copy_to_interleaved_stereo(&decoded, &mut raw_samples),
        Err(symphonia::core::errors::Error::DecodeError(_)) => continue, // Salta frames corruptos
        Err(_) => break,
    }
}
```

- **¿Qué hace?**:
  1. **Soporte MP4**: En un video MP4 hay paquetes de video H.264/H.265 y paquetes de audio AAC. La condición `packet.track_id() != track_id` descarta los fotogramas de video instantáneamente sin gastar CPU.
  2. **Tolerancia a fallos**: Si un paquete está dañado, no crashea; ignora el error de decodificación (`DecodeError`) y continúa con el siguiente.
  3. Convierte las muestras internas de cualquier formato nativo (U8, S16, S24, S32) a punto flotante de 32 bits (`f32`) en formato estéreo intercalado `[L, R, L, R, ...]`.

---

### Bloque C: Algoritmo de Remuestreo Lineal Estéreo (*DSP Resampling*)
*Ubicación:* [`src/decoder.rs`](./src/decoder.rs) líneas 200 a 250.

```rust
pub fn resample_stereo_linear(input: &[f32], in_sr: u32, out_sr: u32) -> Vec<f32> {
    if in_sr == out_sr || input.is_empty() {
        return input.to_vec();
    }

    let in_frames = input.len() / 2;
    let ratio = in_sr as f64 / out_sr as f64;
    let out_frames = ((in_frames as f64) / ratio).round() as usize;
    let mut output = Vec::with_capacity(out_frames * 2);

    for out_idx in 0..out_frames {
        let in_pos = out_idx as f64 * ratio;
        let in_frame = in_pos.floor() as usize;
        let frac = (in_pos - in_frame as f64) as f32;

        let (l0, r0) = (input[in_frame * 2], input[in_frame * 2 + 1]);
        let next_frame = (in_frame + 1).min(in_frames - 1);
        let (l1, r1) = (input[next_frame * 2], input[next_frame * 2 + 1]);

        output.push(l0 + (l1 - l0) * frac); // Canal Izquierdo (L)
        output.push(r0 + (r1 - r0) * frac); // Canal Derecho (R)
    }

    output
}
```

- **¿Qué hace?**:
  1. Si un archivo está grabado a 44.100 Hz (CD de audio) y la tarjeta de sonido opera a 48.000 Hz, remuestrea la señal matemáticamente sin alterar el tono (*pitch*) ni la velocidad de reproducción.
  2. Implementa **interpolación lineal continua**: calcula la muestra intermedia como $S_{out} = S_0 + (S_1 - S_0) \cdot \text{frac}$.
  3. Aplica `Vec::with_capacity(out_frames * 2)` para reservar en una única operación toda la memoria requerida en el Heap, evitando realocaciones dinámicas lentas.

---

### Bloque D: Sintetizador de Cabeceras RIFF/WAVE a Nivel de Bytes
*Ubicación:* [`src/main.rs`](./src/main.rs) líneas 60 a 115.

```rust
fn create_test_wav_file(path: &str, freq: f32, duration_secs: f32) -> anyhow::Result<()> {
    let sample_rate: u32 = 44100;
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let data_len = (num_samples * 4) as u32; // 2 canales * 2 bytes (16 bits)

    let mut file = File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?; // Little-endian 32-bit
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;           // Subchunk size (16 bytes)
    file.write_all(&1u16.to_le_bytes())?;            // PCM lineal = 1
    file.write_all(&2u16.to_le_bytes())?;            // 2 canales (Estéreo)
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * 4).to_le_bytes())?; // Byte rate
    file.write_all(&4u16.to_le_bytes())?;            // Block align
    file.write_all(&16u16.to_le_bytes())?;           // Bits por muestra
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;
    
    // Escritura de muestras sinusoidales cuantizadas a i16
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
        let sample_i16 = (s * 32767.0) as i16;
        file.write_all(&sample_i16.to_le_bytes())?;
        file.write_all(&sample_i16.to_le_bytes())?;
    }
    Ok(())
}
```

- **¿Qué hace?**:
  - Escribe directamente los 44 bytes de la cabecera estándar RIFF/WAVE utilizando el orden de bytes del procesador (`to_le_bytes`: Little-Endian).
  - Permite que esta etapa sea **100% autosuficiente**: no necesitas tener archivos de audio previos en tu máquina para probar el código.

---

## 🧠 4. Conceptos Clave de Rust que se Aprenden en la Etapa 1

| Concepto de Rust | Cómo se aplica en esta etapa |
| :--- | :--- |
| **Ownership (Propiedad)** | `load_and_decode_file` devuelve `Vec<f32>` transfiriendo la propiedad exclusiva del vector al llamador para que no se libere al salir de la función. |
| **Borrowing (Préstamo)** | `resample_stereo_linear` recibe `&[f32]`, prestando el búfer sin clonar datos en memoria. |
| **Slices (`&[T]`)** | Permiten inspeccionar ventanas de audio contiguas en memoria con costo $O(1)$ y sin allocations. |
| **Traits Genéricos (`AsRef<Path>`)** | Evita forzar al usuario a hacer `PathBuf::from(...)`, aceptando `&str` o `String` de forma polimórfica. |
| **Manejo de Errores con `?`** | El operador `?` desempaqueta `Ok(val)` o retorna temprano `Err(e)` sin provocar pánicos (`panic!`). |
| **Pattern Matching** | Permite bifurcar la lógica según el formato de muestra del códec (`SampleFormat::F32`, `SampleFormat::S16`, etc.). |
| **Little-Endianness (`to_le_bytes`)** | Manipulación directa del orden de bytes de enteros de 16 y 32 bits requeridos por la especificación WAV. |

---

## ⚡ 5. Cómo Compilar y Ejecutar

### Opción 1: Ejecución Autónoma (Genera un tono WAV de 440 Hz en `/tmp`)
```bash
cargo run -p etapa_1_decodificador
```

### Opción 2: Probar con tus propios archivos de audio o video
```bash
# Decodificar un archivo MP3
cargo run -p etapa_1_decodificador -- /ruta/a/tu/cancion.mp3

# Decodificar la pista de audio de un video MP4
cargo run -p etapa_1_decodificador -- /ruta/a/tu/video.mp4

# Decodificar un archivo FLAC sin pérdidas
cargo run -p etapa_1_decodificador -- /ruta/a/tu/archivo.flac
```

### Salida esperada en terminal:
```text
==================================================================
   ETAPA 1: FUNDAMENTOS DE RUST Y DECODIFICACIÓN DE AUDIO         
==================================================================
No se especificó un archivo. Creando audio de prueba: /tmp/etapa1_demo.wav

[1] Decodificando archivo: /tmp/etapa1_demo.wav
    Frecuencia objetivo para el hardware: 48000 Hz

[2] Resultados del análisis de audio:
    * Muestras estéreo decodificadas: 144000
    * Cuadros (frames): 72000
    * Frecuencia final: 48000 Hz
    * Duración calculada: 1.50 segundos
    * Amplitud pico: 0.5000 (normalizado en [-1.0, 1.0])

✅ ¡Etapa 1 completada con éxito!
   El decodificador lee binarios en MP3/MP4/WAV y los remuestrea de forma segura.
```
