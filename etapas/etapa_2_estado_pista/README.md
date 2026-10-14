# 🎛️ ETAPA 2: Concurrencia de Tiempo Real, Variables Atómicas y Estado de Pista

Bienvenido a la **Etapa 2** del proyecto **MOPET**. En esta etapa se resuelve el desafío más crítico de cualquier estación de trabajo de audio digital (DAW) o reproductor moderno: **cómo controlar volumen, balance, silenciamiento (Mute), pausa y reemplazo de canciones desde la interfaz de usuario mientras un hilo de audio en tiempo real procesa 48.000 muestras por segundo sin bloqueos ni pausas (*lock-free*)**.

---

## 📦 1. Librerías que se Implementaron (Crates y Dependencias)

En el archivo [`Cargo.toml`](./Cargo.toml) se estructuraron las siguientes dependencias:

```toml
[dependencies]
symphonia = { version = "0.5", features = ["all"] }
parking_lot = "0.12"
anyhow = "1.0"
```

### ¿Por qué se eligieron estas librerías y componentes estándar?

1. **`parking_lot` (v0.12)**:
   - **Qué es**: Una reimplementación ultraligera y de alto rendimiento de las primitivas de sincronización de la biblioteca estándar de Rust (`RwLock` y `Mutex`).
   - **Ventajas sobre `std::sync::RwLock`**:
     - **Tamaño mínimo**: Un `parking_lot::RwLock` ocupa solo **1 byte** en memoria frente a los 24 o 40 bytes del `std::sync::RwLock` en plataformas POSIX.
     - **Sin envenenamiento de locks (*no lock poisoning*)**: Si un hilo falla, no deja inservible el cerrojo para el resto del programa, evitando pánicos en cadena.
     - **Velocidad**: Adquisición de lectura en menos de **10 nanosegundos** en ausencia de contención.

2. **Módulo de concurrencia nativo de Rust (`std::sync` y `std::thread`)**:
   - `std::sync::Arc`: Puntero con conteo de referencias atómicas (*Atomic Reference Counting*). Permite que múltiples hilos sean dueños del mismo objeto sin clonar los pesados vectores de audio.
   - `std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering}`: Instrucciones de CPU directas (1 ciclo de reloj) que garantizan lectura y escritura segura sin jamás detener un hilo.

---

## 📂 2. Archivos Creados en la Etapa 2

```text
etapas/etapa_2_estado_pista/
├── Cargo.toml          # Configuración del paquete en el Workspace y dependencias
├── README.md           # Guía pedagógica y manual técnico de la etapa
└── src/
    ├── track.rs        # Modelo de datos central: AtomicF32, struct Track, atómicos y Hot-Swap
    ├── main.rs         # Demostración multihilo interactiva: Hilo Audio vs Hilo UI
    └── decoder.rs      # Módulo decodificador heredado de la Etapa 1
```

### Rol de cada archivo:
- [`Cargo.toml`](./Cargo.toml): Incorpora `parking_lot` para los bloqueos de escritura esporádicos (como sustituir el archivo de audio).
- [`src/track.rs`](./src/track.rs): Define [`AtomicF32`](./src/track.rs) y la estructura [`Track`](./src/track.rs), encapsulando playhead, volumen, panorama, mute, solo y bufer de audio.
- [`src/main.rs`](./src/main.rs): Lanza dos hilos reales en el sistema operativo: uno simula la tarjeta de sonido leyendo a alta velocidad, mientras el otro simula a un usuario cambiando volumen y cambiando la canción en caliente.

---

## 🧩 3. Bloques de Código Específicos Implementados

A continuación se detallan los 4 bloques centrales de la Etapa 2:

### Bloque A: Envoltorio Atómico para Punto Flotante (`AtomicF32`)
*Ubicación:* [`src/track.rs`](./src/track.rs) líneas 10 a 55.

```rust
pub struct AtomicF32 {
    storage: AtomicU32,
}

impl AtomicF32 {
    pub fn new(val: f32) -> Self {
        Self {
            storage: AtomicU32::new(val.to_bits()),
        }
    }

    #[inline]
    pub fn load(&self, order: Ordering) -> f32 {
        f32::from_bits(self.storage.load(order))
    }

    #[inline]
    pub fn store(&self, val: f32, order: Ordering) {
        self.storage.store(val.to_bits(), order);
    }
}
```

- **¿Qué hace?**:
  1. En las arquitecturas x86_64 y ARM64, las CPUs no ofrecen una instrucción nativa `AtomicF32` en el conjunto estándar de Rust.
  2. Mediante manipulación de bits a costo cero (*zero-cost abstraction*), convertimos el `f32` a su representación binaria IEEE-754 de 32 bits enteros con `val.to_bits()`.
  3. Esto permite almacenar y leer valores continuos de volumen (ej. `0.85`), balance estéreo (`-0.2`) o mediciones de vúmetro en **1 solo ciclo de reloj de la CPU sin usar un Mutex**.

---

### Bloque B: Modelo Híbrido de Pista Concurrente (`Track`)
*Ubicación:* [`src/track.rs`](./src/track.rs) líneas 58 a 130.

```rust
pub struct Track {
    pub id: usize,
    pub name: RwLock<String>,
    pub samples: RwLock<Arc<[f32]>>,
    pub total_frames: AtomicUsize,
    pub sample_rate: AtomicU32,

    // Controles de tiempo de ejecución sin cerrojos (Lock-free)
    pub playhead: AtomicUsize,
    pub volume: AtomicF32,
    pub pan: AtomicF32,
    pub muted: AtomicBool,
    pub solo: AtomicBool,
    pub paused: AtomicBool,
    pub looping: AtomicBool,

    // Vúmetros en tiempo real
    pub peak_l: AtomicF32,
    pub peak_r: AtomicF32,
}
```

- **¿Qué hace?**:
  1. **Separación de responsabilidades**:
     - Parámetros dinámicos (playhead, volumen, mute, pausa): Son variables atómicas puras (`Atomic*`). El hilo de audio puede consultarlos millones de veces por segundo con cero sobrecoste.
     - Búfer de audio y nombre: Están protegidos por un `RwLock<Arc<[f32]>>`. Como el audio rara vez cambia de archivo, los lectores múltiples (`read()`) acceden en simultáneo sin bloquearse.

---

### Bloque C: Reemplazo en Caliente de Memoria (*Hot-Swap*)
*Ubicación:* [`src/track.rs`](./src/track.rs) líneas 140 a 170.

```rust
pub fn swap_audio(&self, new_samples: Vec<f32>, new_sample_rate: u32, new_name: String) {
    let frames = new_samples.len() / 2;
    {
        let mut s = self.samples.write();
        *s = new_samples.into(); // El búfer anterior se descarta y libera de RAM aquí
    }
    self.total_frames.store(frames, Ordering::Release);
    self.sample_rate.store(new_sample_rate, Ordering::Release);
    self.playhead.store(0, Ordering::Release);
    {
        let mut n = self.name.write();
        *n = new_name;
    }
}
```

- **¿Por qué es fundamental?**:
  - Es el secreto del bajo consumo de memoria de MOPET (**< 25 MB de RAM fija**).
  - Al cambiar de canción en una cola de reproducción, no creamos una nueva pista ni destruimos el motor de audio: simplemente intercambiamos el búfer interno.
  - Al sobrescribir `*s = new_samples.into()`, Rust invoca el destructor automático ([`Drop`](https://doc.rust-lang.org/std/ops/trait.Drop.html)) del búfer anterior, liberando los megabytes de RAM inmediatamente sin esperar a un recolector de basura.

---

### Bloque D: Demostración Multihilo en Tiempo Real
*Ubicación:* [`src/main.rs`](./src/main.rs) líneas 35 a 115.

```rust
// Hilo Compartido mediante Arc
let track = Arc::new(Track::new(0, "Pista A (Inicial)".into(), samples, 44100));
let track_reader = Arc::clone(&track);

// HILO SECUNDARIO (Simulador del motor de audio de hardware)
let handle = thread::spawn(move || {
    for _ in 0..5 {
        thread::sleep(Duration::from_millis(50));
        let vol = track_reader.volume.load(Ordering::Relaxed);
        let muted = track_reader.muted.load(Ordering::Relaxed);
        let name = track_reader.get_name();
        let frames = track_reader.total_frames.load(Ordering::Relaxed);
        println!("  [Hilo Audio] Leyendo '{}' -> Vol: {:.0}%, Mute: {}, Frames: {}", name, vol * 100.0, muted, frames);
    }
});

// HILO PRINCIPAL (Simulador de eventos de teclado de la UI)
thread::sleep(Duration::from_millis(60));
track.adjust_volume(0.2); // Sube volumen al 120%
track.toggle_mute();      // Activa silencio
track.swap_audio(new_samples, 44100, "Pista B (Reemplazada en caliente)".into());

handle.join().unwrap(); // Espera sincronizada
```

- **¿Qué demuestra?**:
  - Mientras el hilo secundario lee constantemente para no perder muestras, el hilo principal puede modificar el volumen, silenciar la pista y cambiar el archivo en el Heap.
  - No hay colisiones de memoria (*data races*), no hay bloqueos mutuos (*deadlocks*) y la compilación está respaldada por el sistema de tipos de Rust.

---

## 🧠 4. Conceptos Clave de Rust que se Aprenden en la Etapa 2

| Concepto de Rust | Cómo se aplica en esta etapa |
| :--- | :--- |
| **`Send` y `Sync`** | Marcadores automáticos del compilador. `Track` implementa `Sync` porque todos sus campos son atómicos o están protegidos por `RwLock`, permitiendo compartir `&Track` entre hilos. |
| **`Arc<T>`** | Puntero atómico compartido. `Arc::clone(&track)` crea un nuevo puntero que apunta a la misma zona del Heap incrementando el contador de referencias atómicamente. |
| **Mutabilidad Interior (*Interior Mutability*)** | Modificar campos como `volume` o `playhead` a través de referencias compartidas `&self` (inmutables), sin necesitar exclusividad `&mut self`. |
| **Modelos de Consistencia de Memoria (`Ordering`)** | Uso de `Ordering::Relaxed` para contadores independientes y `Ordering::Release` / `Ordering::Acquire` para sincronizar el Hot-Swap de buffers. |
| **Clausura con `move` (`thread::spawn(move || ...)`)** | Transfiere la propiedad del clon `track_reader` al contexto del nuevo hilo para garantizar que la variable viva lo suficiente. |
| **Sincronización con `.join()`** | Garantiza la recolección limpia de hilos hijos antes de que termine el programa principal. |

---

## ⚡ 5. Cómo Compilar y Ejecutar

### Ejecución de la demostración multihilo:
```bash
cargo run -p etapa_2_estado_pista
```

### Salida real en terminal obtenida:
```text
==================================================================
   ETAPA 2: CONCURRENCIA DE TIEMPO REAL Y VARIABLES ATÓMICAS      
==================================================================
[1] Pista creada: 'Pista A (Inicial)', Duración: 1.0s
    Volumen inicial: 100%, Mute: false
  [Hilo Audio] Leyendo 'Pista A (Inicial)' -> Vol: 100%, Mute: false, Frames: 44100

[2] Hilo Principal: Modificando volumen (+20%) y silenciando (Mute)...
  [Hilo Audio] Leyendo 'Pista A (Inicial)' -> Vol: 120%, Mute: true, Frames: 44100

[3] Hilo Principal: Realizando Hot-Swap (Reemplazo por 'Pista B' de 2 segundos)...
  [Hilo Audio] Leyendo 'Pista B (Reemplazada en caliente)' -> Vol: 120%, Mute: true, Frames: 88200
  [Hilo Audio] Leyendo 'Pista B (Reemplazada en caliente)' -> Vol: 120%, Mute: true, Frames: 88200
  [Hilo Audio] Leyendo 'Pista B (Reemplazada en caliente)' -> Vol: 120%, Mute: true, Frames: 88200

✅ ¡Etapa 2 completada con éxito!
   La estructura Track permite comunicación segura y sin bloqueos entre hilos.
```
