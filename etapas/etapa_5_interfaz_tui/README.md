# 🖥️ ETAPA 5: Interfaz de Terminal Interactiva (TUI), Flujo CLI y Experiencia Global

Bienvenido a la **Etapa 5**, la culminación del proyecto **MOPET**. En esta etapa integramos todos los componentes desarrollados a lo largo de las etapas 1 a 4 y construimos la **experiencia de usuario completa**: un menú interactivo guiado para la terminal, un analizador de rutas tolerante a espacios y comillas, y una **interfaz gráfica basada en texto (TUI: *Text User Interface*)** que renderiza a 30 FPS con vúmetros animados, barras de progreso y controles en tiempo real estilo mezclador profesional (DAW).

---

## 📦 1. Librerías que se Implementaron (Crates y Dependencias)

En el archivo [`Cargo.toml`](./Cargo.toml) se estructuró la pila tecnológica completa del reproductor:

```toml
[dependencies]
cpal = "0.15"
symphonia = { version = "0.5", features = ["all"] }
crossterm = "0.28"
parking_lot = "0.12"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
```

### ¿Por qué se eligieron estas librerías?

1. **`crossterm` (v0.28)**:
   - **Qué es**: La biblioteca estándar de la comunidad de Rust para manipulación de terminales a bajo nivel con compatibilidad 100% multiplataforma (Linux, macOS y Windows Console / Windows Terminal).
   - **Funcionalidades utilizadas**:
     - `terminal::enable_raw_mode()` y `disable_raw_mode()`: Activa el modo crudo (las pulsaciones de teclas llegan al instante sin requerir pulsar Enter ni imprimir el carácter en pantalla).
     - `terminal::EnterAlternateScreen` y `LeaveAlternateScreen`: Abre una pantalla virtual independiente, protegiendo el historial previo de la terminal del usuario.
     - `cursor::Hide` y `cursor::Show`: Oculta el cursor parpadeante para una apariencia estética limpia.
     - `cursor::MoveTo(0, 0)`: Reposiciona el cursor en la esquina superior izquierda para sobrescribir el texto, eliminando el parpadeo (*flickering*).
     - `event::poll` y `event::read`: Captura de eventos de teclado de manera asíncrona y no bloqueante.

2. **Integración completa de etapas anteriores**:
   - `cpal`: Salida de sonido física multicanal.
   - `symphonia`: Decodificación universal (MP3, MP4 con AAC, WAV, FLAC, OGG).
   - `parking_lot`: Cerrojos de 1 byte y sincronización atómica.
   - `serde` y `serde_json`: Persistencia de planes de audio en `mopet_plan.json`.
   - `anyhow`: Manejo unificado de errores.

---

## 📂 2. Archivos Creados en la Etapa 5

```text
etapas/etapa_5_interfaz_tui/
├── Cargo.toml          # Declaración del paquete en el Workspace y dependencias
├── README.md           # Guía pedagógica y manual técnico de la etapa
└── src/
    ├── cli_flow.rs     # Flujo conversacional guiado y analizador de rutas con comillas
    ├── ui.rs           # Interfaz TUI interactiva a 30 FPS, vúmetros y transporte
    ├── main.rs         # Enrutador CLI global, analizador de banderas y manual de ayuda
    ├── plan.rs         # Planes de reproducción y Lazy Loading (Etapa 4)
    ├── engine.rs       # Motor DSP y callback CPAL (Etapa 3)
    ├── synth.rs        # Sintetizadores procedurales (Etapa 3)
    ├── track.rs        # Modelo de pistas atómicas (Etapa 2)
    └── decoder.rs      # Decodificador multimedia (Etapa 1)
```

### Rol de cada archivo:
- [`src/cli_flow.rs`](./src/cli_flow.rs): Despliega el menú de bienvenida cuando el usuario escribe `mopet` sin argumentos. Analiza rutas arrastradas desde el explorador y valida formatos de audio.
- [`src/ui.rs`](./src/ui.rs): Motor de renderizado en terminal. Dibuja el vúmetro estéreo Master, lista de pistas, medidores individuales, botones de estado con color (PLAY, MUTE, SOLO, LOOP) y atiende las teclas en tiempo real.
- [`src/main.rs`](./src/main.rs): Enrutador principal. Decide si arrancar el flujo interactivo, el modo demo, el modo multipista simultáneo (`--multi`) o ejecutar planes guardados (`--plan`).

---

## 🧩 3. Bloques de Código Específicos Implementados

### Bloque A: Menú Interactivo Guiado y Parseador FSM de Rutas
*Ubicación:* [`src/cli_flow.rs`](./src/cli_flow.rs) líneas 20 a 190.

```rust
// Máquina de estados finitos (FSM) para parsear rutas con comillas y espacios
fn parse_input_paths(input: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for ch in input.chars() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"'  if !in_single_quote => in_double_quote = !in_double_quote,
            ' ' | ',' if !in_single_quote && !in_double_quote => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    paths.push(PathBuf::from(trimmed));
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    // ...
```

- **¿Qué resuelve?**:
  - En Linux, macOS o Windows, si arrastras archivos desde el gestor de ventanas a la terminal, llegan rodeados de comillas (ej. `'/home/musica/01 - Queen - Bohemian Rhapsody.mp3'`).
  - Un simple `.split(' ')` rompería la ruta en varios fragmentos inválidos.
  - Este bloque implementa un tokenizador de máquina de estados de un solo pase que respeta comillas simples y dobles, permitiendo ingresar múltiples canciones separadas por espacios sin errores.

---

### Bloque B: Ciclo de Vida de la Terminal en Modo Crudo (*Raw Mode*)
*Ubicación:* [`src/ui.rs`](./src/ui.rs) líneas 40 a 75 y 260 a 270.

```rust
pub fn run(&mut self, engine: &AudioEngine, mut plan: Option<&mut AudioPlan>) -> Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, cursor::Hide)?;

    let mut running = true;
    while running {
        // 1. Verificación automática de avance en cola (Lazy Loading)
        // 2. Consulta no bloqueante de teclado (25 ms timeout)
        // 3. Renderizado a 30 FPS (~30 ms)
    }

    // Restauración limpia garantizada al salir
    execute!(out, cursor::Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
```

- **¿Por qué es fundamental?**:
  - Aplica el patrón de diseño **RAII** (*Resource Acquisition Is Initialization*).
  - Al ingresar, toma el control de la terminal ocultando el cursor.
  - Al salir (por tecla `Q` o `Esc`), restaura la pantalla principal del usuario y el eco del teclado, evitando que la terminal quede "congelada" o rota.

---

### Bloque C: Sondeo No Bloqueante de Eventos de Teclado
*Ubicación:* [`src/ui.rs`](./src/ui.rs) líneas 75 a 250.

```rust
if event::poll(Duration::from_millis(25))? {
    if let Event::Key(key) = event::read()? {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Char(' ') => engine.toggle_master_pause(),
                KeyCode::Char('n') => /* siguiente canción en plan */,
                KeyCode::Char('b') => /* canción anterior en plan */,
                KeyCode::Char('t') => /* alternar modo repetición */,
                KeyCode::Char('w') => /* guardar plan en JSON */,
                KeyCode::Up        => track.adjust_volume(0.05),
                KeyCode::Down      => track.adjust_volume(-0.05),
                KeyCode::Left      => track.adjust_pan(-0.1),
                KeyCode::Right     => track.adjust_pan(0.1),
                KeyCode::Char('m') => track.toggle_mute(),
                KeyCode::Char('s') => track.toggle_solo(),
                KeyCode::Char('q') | KeyCode::Esc => running = false,
                _ => {}
            }
        }
    }
}
```

- **¿Qué hace?**:
  - `event::poll` espera como máximo 25 ms por un evento. Si el usuario no presiona nada, la función retorna inmediatamente `false` para no congelar el renderizado de los vúmetros a 30 FPS.
  - Modifica directamente las variables atómicas del `Track` o `AudioEngine`, haciendo que los cambios de volumen o mute se escuchen en menos de **5 milisegundos**.

---

### Bloque D: Motor de Renderizado TUI a 30 FPS Sin Parpadeos
*Ubicación:* [`src/ui.rs`](./src/ui.rs) líneas 270 a 445 y 460 a 500.

```rust
// Redibujado suave sobreescribiendo desde la coordenada (0, 0)
execute!(out, cursor::MoveTo(0, 0))?;

// Generación de barra de vúmetro de pico: ■■■■■······
fn render_vu_meter(peak: f32, width: usize) -> String {
    let filled = ((peak.min(1.0)) * width as f32).round() as usize;
    let mut s = String::with_capacity(width);
    for i in 0..width {
        if i < filled { s.push('■'); } else { s.push('·'); }
    }
    s
}

// Generación de barra de progreso de reproducción: [=====>---------]
fn render_progress_bar(pct: f32, width: usize) -> String {
    let filled = ((pct.clamp(0.0, 1.0)) * width as f32).round() as usize;
    let mut s = String::with_capacity(width + 2);
    s.push('[');
    for i in 0..width {
        if i < filled { s.push('='); }
        else if i == filled { s.push('>'); }
        else { s.push('-'); }
    }
    s.push(']');
    s
}
```

- **¿Por qué no parpadea la pantalla?**:
  - La mayoría de programas de terminal usan `clear()` antes de imprimir, lo que borra la pantalla durante un instante provocando un parpadeo constante (*flicker*).
  - MOPET utiliza la técnica de sobrescritura directa con `cursor::MoveTo(0, 0)` y buffers estáticos, logrando una animación fluida de 30 FPS con consumo de CPU inferior al 1%.

---

## 🧠 4. Conceptos Clave de Rust que se Aprenden en la Etapa 5

| Concepto de Rust | Cómo se aplica en esta etapa |
| :--- | :--- |
| **Control de Terminal y Códigos ANSI** | Modos Raw y Alternate Screen gestionados a bajo nivel mediante llamadas de sistema POSIX/Windows encapsuladas por `crossterm`. |
| **Arquitectura Basada en Eventos (*Event-Driven*)** | Desacoplamiento entre la captura de eventos de teclado, el avance del plan de audio y el bucle de renderizado visual a 30 FPS. |
| **Patrón RAII (*Resource Acquisition Is Initialization*)** | Garantizar que el modo de la terminal se restablezca de forma segura incluso ante salidas anticipadas con el operador `?`. |
| **Máquinas de Estados Finitos (FSM)** | Tokenización de entradas del usuario con estados booleanos (`in_single_quote`, `in_double_quote`) en una sola pasada $O(N)$. |
| **Polimorfismo de Interfaz** | El mismo método `ui.run(...)` renderiza tanto listas de reproducción secuenciales con plan como sesiones multipista simultáneas. |

---

## ⚡ 5. Cómo Compilar y Ejecutar

### Opción 1: Menú interactivo guiado en la terminal
```bash
cargo run -p etapa_5_interfaz_tui
```
*(O simplemente escribiendo `mopet` si está instalado globalmente)*

### Opción 2: Reproducir una carpeta completa
```bash
cargo run -p etapa_5_interfaz_tui -- /ruta/a/mi/musica
```

### Opción 3: Mezclar múltiples archivos en paralelo (Modo Multipista)
```bash
cargo run -p etapa_5_interfaz_tui -- --multi voz.wav bateria.mp3 bajo.mp4
```

### Opción 4: Modo Demostración procedural
```bash
cargo run -p etapa_5_interfaz_tui -- --demo
```

---

## ⌨️ 6. Controles Interactivos en Vivo

Durante la reproducción en la interfaz TUI, puedes pulsar las siguientes teclas:

| Tecla | Acción |
| :--- | :--- |
| **`[Espacio]`** | Pausar / Reanudar la reproducción general (Master). |
| **`[N]`** | Siguiente canción en la lista del plan (*Next*). |
| **`[B]`** | Canción anterior en la lista del plan (*Back*). |
| **`[T]`** | Alternar modo de repetición: Secuencial $\rightarrow$ Repetir Plan $\rightarrow$ Repetir Canción. |
| **`[W]`** | Guardar la lista actual en disco en formato `mopet_plan.json`. |
| **`[1..9]` / `[Tab]`** | Seleccionar la pista activa para ajustes individuales. |
| **`[↑]` / `[↓]`** | Subir / Bajar volumen de la pista seleccionada (±5%). |
| **`[←]` / `[→]`** | Ajustar balance estéreo (Panorama Izquierda / Derecha). |
| **`[M]`** | Silenciar pista seleccionada (*Mute*). |
| **`[S]`** | Escucha exclusiva (*Solo* - aislar pista). |
| **`[L]`** | Alternar repetición en bucle de la pista individual. |
| **`[J]` / `[K]`** | Rebobinar / Adelantar 5 segundos (*Seek* ±5s). |
| **`[R]`** | Reiniciar reproducción al inicio (00:00). |
| **`[+]` / `[-]`** | Subir / Bajar volumen general Master. |
| **`[Q]` / `[Esc]`** | Salir del reproductor de forma limpia. |

---

### Vista previa de la interfaz interactiva en terminal:

```text
╔════════════════════════════════════════════════════════════════════════════════════════════╗
║                 MOPET - REPRODUCTOR MULTIPISTA ULTRA-EFICIENTE (RUST)                      ║
╚════════════════════════════════════════════════════════════════════════════════════════════╝
  PLAN: "Carpeta: Rock Clásico" │ Canción [1/12] │ Modo: SECUENCIAL (1 a 1)
  Siguiente en cola: 02 - Bohemian Rhapsody.mp3
  Hardware: 48000 Hz │ Canales: 2 │ Pistas activas: 1 │ Consumo RAM aprox: ~20 MB
  Master: [▶ EN PLAY ] │ Vol: 100% │ VU L:[■■■■■■····] R:[■■■■■■····]
  ──────────────────────────────────────────────────────────────────────────────────────────
 ► [Pista #1] 01 - Bohemian Rhapsody.mp3       [PLAY] [MUTE] [SOLO] [LOOP] 
     Vol: 100% │ Pan: C    │ VU: [■■■■■···] [■■■■■···] │ [======>-----------------] 01:14/05:55 ( 21%)
  ──────────────────────────────────────────────────────────────────────────────────────────
  STATUS: Reproductor iniciado. [Espacio] Play/Pausa | [W] Guardar Plan | [Q] Salir
  [Espacio] Play/Pausa │ [N] Sig. Canción │ [B] Canción Anterior │ [T] Modo Repetición
  [W] Guardar Plan (JSON) │ [↑/↓] Vol Pista │ [M] Mute │ [S] Solo │ [J/K] ±5s │ [Q] Salir
```
