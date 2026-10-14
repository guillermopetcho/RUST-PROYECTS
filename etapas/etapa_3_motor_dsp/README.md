# 🔊 ETAPA 3: Motor DSP, Salida de Hardware (CPAL) y Síntesis Procedimental

Bienvenido a la **Etapa 3** del proyecto **MOPET**. En esta etapa conectamos el software directamente con los altavoces o auriculares físicos del sistema. Implementamos el **motor de procesamiento de señales digitales (DSP)**, la mezcla de múltiples pistas estéreo en paralelo, un limitador suave analógico para prevenir distorsión digital (*soft-clipping*) y un trío de instrumentos musicales sintetizados por software mediante ecuaciones trigonométricas y envolventes exponenciales.

---

## 📦 1. Librerías que se Implementaron (Crates y Dependencias)

En el archivo [`Cargo.toml`](./Cargo.toml) se incorporó la librería de hardware de sonido:

```toml
[dependencies]
cpal = "0.15"
symphonia = { version = "0.5", features = ["all"] }
parking_lot = "0.12"
anyhow = "1.0"
```

### ¿Por qué se eligieron estas librerías?

1. **`cpal` (v0.15 - *Cross-Platform Audio Library*)**:
   - **Qué es**: La biblioteca estándar de la comunidad de Rust para interactuar directamente con los controladores de audio de bajo nivel del sistema operativo:
     - **Linux**: ALSA nativo y servidores de sonido (PulseAudio / PipeWire).
     - **Windows**: WASAPI (*Windows Audio Session API*).
     - **macOS / iOS**: CoreAudio.
   - **Filosofía**: Crea un hilo del sistema operativo con prioridad de tiempo real que ejecuta una función de callback cada vez que el chip de sonido necesita un nuevo bloque de muestras (ej. cada ~5 a 10 milisegundos).

2. **`parking_lot`, `symphonia` y `anyhow`**:
   - Reutilizados de las etapas 1 y 2 para el control de pistas atómicas, decodificación opcional de archivos locales y manejo idiomático de errores.

---

## 📂 2. Archivos Creados en la Etapa 3

```text
etapas/etapa_3_motor_dsp/
├── Cargo.toml          # Declaración del paquete en el Workspace y dependencias
├── README.md           # Guía pedagógica y manual técnico de la etapa
└── src/
    ├── engine.rs       # Motor DSP: Callback de CPAL, mezcla SIMD, soft-limiter y vúmetros
    ├── synth.rs        # Sintetizadores procedurales: Batería, bajo analógico y arpegio melódico
    ├── main.rs         # Inicialización de hardware, sesión a 120 BPM y vúmetros en vivo
    ├── track.rs        # Modelo de pistas concurrentes atómicas (Etapa 2)
    └── decoder.rs      # Módulo decodificador de formatos (Etapa 1)
```

### Rol de cada archivo:
- [`src/engine.rs`](./src/engine.rs): Administra el ciclo de vida de la tarjeta de sonido física a través de `cpal::Stream`. Mezcla $N$ pistas en un búfer estéreo temporal sin alojar memoria (`zero-allocations`).
- [`src/synth.rs`](./src/synth.rs): Módulo matemático autosuficiente que produce música procedimental a 120 BPM con osciladores senoidales, triangulares, onda sierra y ruido blanco.
- [`src/main.rs`](./src/main.rs): Consulta la frecuencia nativa del hardware (ej. 48.000 Hz), monta las 3 pistas sintetizadas, arranca el audio y despliega barras de nivel estéreo (VU) en tiempo real.

---

## 🧩 3. Bloques de Código Específicos Implementados

### Bloque A: Inicialización del Hardware y Stream de Salida con CPAL
*Ubicación:* [`src/engine.rs`](./src/engine.rs) líneas 50 a 120.

```rust
pub fn new(tracks: Vec<Arc<Track>>) -> Result<Self> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("No se encontró ningún dispositivo de salida de audio")?;

    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            Self::build_stream::<f32>(&device, &config.into(), tracks.clone(), ...)?
        }
        cpal::SampleFormat::I16 => {
            Self::build_stream::<i16>(&device, &config.into(), tracks.clone(), ...)?
        }
        _ => anyhow::bail!("Formato de hardware no soportado"),
    };

    stream.play()?; // Comienza la reproducción física de sonido
    // ...
```

- **¿Qué hace?**:
  1. Detecta la tarjeta de sonido predeterminada del sistema.
  2. Determina si el convertidor digital a analógico (DAC) espera muestras en formato decimal `f32` o entero de 16 bits `i16`, instanciando el stream adecuado mediante genéricos.
  3. Lanza el hilo de audio de alta prioridad del kernel con `stream.play()`.

---

### Bloque B: Bucle de Mezcla SIMD en Tiempo Real (*Zero-Allocation Callback*)
*Ubicación:* [`src/engine.rs`](./src/engine.rs) líneas 250 a 350.

```rust
// Búfer preasignado reutilizable (cero allocations en el hilo de audio)
let mut mix_buf = vec![0.0f32; out_frames * 2];

for track in tracks.iter() {
    let vol = track.volume.load(Ordering::Relaxed);
    if vol <= 0.0001 || track.paused.load(Ordering::Relaxed) {
        continue; // Ahorro de CPU: Salta pistas silenciadas o pausadas
    }

    let pan = track.pan.load(Ordering::Relaxed);
    let pan_l = (1.0f32 - pan).clamp(0.0f32, 1.0f32) * vol;
    let pan_r = (1.0f32 + pan).clamp(0.0f32, 1.0f32) * vol;

    let samples = track.samples.read();
    let mut cur = track.playhead.load(Ordering::Relaxed);

    for f in 0..out_frames {
        if cur >= total_frames {
            if looping { cur = 0; } else { break; }
        }
        // Mezcla acumulativa en el bus estéreo
        mix_buf[f * 2]     += samples[cur * 2]     * pan_l;
        mix_buf[f * 2 + 1] += samples[cur * 2 + 1] * pan_r;
        cur += 1;
    }
    track.playhead.store(cur, Ordering::Release);
}
```

- **¿Qué hace?**:
  - Lee los buffers inmutables de cada pista simultáneamente aplicando el volumen y el balance estéreo independiente de cada una.
  - El compilador LLVM de Rust vectoriza automáticamente este bucle mediante instrucciones **SIMD** (AVX2/NEON), procesando múltiples muestras en un solo ciclo de reloj.
  - El playhead se actualiza de forma atómica (`Ordering::Release`) para que la interfaz de usuario pueda graficar el progreso sin colisiones.

---

### Bloque C: Limitador Suave Analógico (*Soft Clipper* $\tanh(x)$)
*Ubicación:* [`src/engine.rs`](./src/engine.rs) líneas 351 a 390.

```rust
// Aplicación de volumen Master, vúmetros y Soft-Limiter no lineal
for f in 0..out_frames {
    let raw_l = mix_buf[f * 2]     * master_vol;
    let raw_r = mix_buf[f * 2 + 1] * master_vol;

    // Saturación suave analógica basada en tangente hiperbólica
    let limited_l = raw_l.tanh();
    let limited_r = raw_r.tanh();

    // Escritura en el búfer final del hardware (rango estrictamente [-1.0, 1.0])
    output_samples[f * 2]     = limited_l;
    output_samples[f * 2 + 1] = limited_r;
}
```

- **¿Por qué es fundamental?**:
  - Al mezclar varias pistas sonando a la vez (ej. Bombo + Bajo + Sintetizador), la suma de amplitudes puede exceder $1.0$. En audio digital tradicional, esto causa un recorte duro (*hard digital clipping*), produciendo una distorsión áspera y desagradable.
  - La función matemática $\tanh(x)$ (tangente hiperbólica) comprime suavemente los picos excesivos hacia las asíntotas $[-1.0, +1.0]$, emulando la calidez y saturación de las cintas magnéticas de audio profesional de estudio.

---

### Bloque D: Síntesis Musical Procedimental (Batería, Bajo y Arpegio)
*Ubicación:* [`src/synth.rs`](./src/synth.rs) líneas 20 a 160.

```rust
// Síntesis de Bombo (Kick Drum) mediante modulación de frecuencia exponencial
if step % 4 == 0 && t < 0.25 {
    let freq = 120.0 * (-18.0 * t).exp() + 45.0; // Pitch-drop de 165 Hz cayendo a 45 Hz
    let phase = freq * t * 2.0 * PI;
    let env = (-10.0 * t).exp();                  // Envolvente de volumen percusiva
    sample += phase.sin() * env * 0.8;
}

// Síntesis de Bajo con acumulador de fase continuo
phase += freq / sr * 2.0 * PI;
if phase > 2.0 * PI { phase -= 2.0 * PI; }
let saw = (phase / PI) - 1.0;                     // Onda diente de sierra analítica
let sine = phase.sin();                            // Subgrave senoidal puro
let val = (sine * 0.6 + saw * 0.4) * env * 0.6;   // Mezcla analógica
```

- **¿Qué hace?**:
  1. **Batería electrónica**: Genera un bombo (*4-on-the-floor*), caja con ruido blanco pseudoaleatorio y platillos hi-hat a 120 BPM.
  2. **Bajo analógico**: Utiliza un acumulador de fase continuo para evitar chasquidos (*clicks*) al cambiar entre notas musicales (progresión La menor, Fa, Do, Sol).
  3. **Arpegio melódico**: Genera ondas triangulares y aplica un paneo alternante tipo ping-pong (izquierda/derecha) para crear amplitud espacial en estéreo.

---

## 🧠 4. Conceptos Clave de Rust que se Aprenden en la Etapa 3

| Concepto de Rust | Cómo se aplica en esta etapa |
| :--- | :--- |
| **Seguridad de Tiempo Real (*Real-Time Safety*)** | Dentro del callback de audio de CPAL está terminantemente prohibido usar `println!`, abrir archivos o reservar memoria (`Vec::new()`), ya que causarían microcortes (*audio underruns*). |
| **Clausuras con Captura `move`** | `build_output_stream(..., move |data, _| { ... })` transfiere la propiedad del vector de pistas al hilo de hardware con ciclo de vida `'static`. |
| **Vectorización Automática (SIMD)** | Estructurar los bucles de mezcla con accesos continuos `mix_buf[f * 2]` permite a LLVM emitir instrucciones AVX/NEON de procesamiento vectorial paralelo. |
| **Acumuladores de Fase (*Phase Accumulation*)** | Técnica matemática en DSP para sintetizadores que preserva la continuidad de la forma de onda modulando el ángulo entre $[0, 2\pi)$. |
| **Telemetría Libre de Esperas (*Wait-Free Telemetry*)** | Actualizar los vúmetros `peak_l` y `peak_r` mediante `AtomicF32` con `Ordering::Relaxed`, permitiendo que la terminal lea los niveles sin interferir con la reproducción acústica. |

---

## ⚡ 5. Cómo Compilar y Ejecutar

### Ejecución de la demostración:
```bash
cargo run -p etapa_3_motor_dsp
```

### O cargar tus propias pistas para reproducirlas simultáneamente:
```bash
cargo run -p etapa_3_motor_dsp -- bateria.wav bajo.mp3 voz.mp4
```

### Salida esperada en terminal (con vúmetros Master activos):
```text
==================================================================
   ETAPA 3: MOTOR DSP Y HARDWARE DE SONIDO EN TIEMPO REAL         
==================================================================
[1] Dispositivo de salida: HDA Intel PCH (ALSA)
    Frecuencia de muestreo nativa: 48000 Hz

[2] Generando 3 pistas sincronizadas (Batería, Bajo y Sintetizador)...

[3] Iniciando hilo de procesamiento de audio de baja latencia...
🔊 ¡Sonando ahora mismo a través de tus altavoces!
   Mostrando medición de nivel estéreo (VU) durante 6 segundos:

   VU Master L:[■■■■■■■········] R:[■■■■■■■········]  (Picos: L=0.48, R=0.47)
   VU Master L:[■■■■■■■■■······] R:[■■■■■■■■·······]  (Picos: L=0.62, R=0.55)
   VU Master L:[■■■■■■■■■■·····] R:[■■■■■■■■■■·····]  (Picos: L=0.71, R=0.70)
   VU Master L:[■■■■■■■········] R:[■■■■■■■■·······]  (Picos: L=0.45, R=0.52)
   VU Master L:[■■■■■■■■■······] R:[■■■■■■■■·······]  (Picos: L=0.59, R=0.54)
   VU Master L:[■■■■■■■■■■·····] R:[■■■■■■■■■■·····]  (Picos: L=0.68, R=0.69)

✅ ¡Etapa 3 completada con éxito!
   El motor mezcla en tiempo real sin pausas y previene distorsión digital con el soft-limiter.
```
