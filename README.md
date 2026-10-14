# MOPET - Reproductor de Audio Ultra-Eficiente y Multipista en Rust

**MOPET** está diseñado para ejecutarse y controlarse **100% desde la terminal**, con dos modos de uso:
1. **Modo Asistido Interactivo**: Al ejecutar `mopet` (o `./mopet.sh`) sin argumentos, se despliega un menú interactivo en la terminal para elegir carpetas, cargar listas o armar planes con unos pocos toques.
2. **Modo Directo por Comandos**: Puedes pasarle directamente archivos, carpetas o listas para que empiece a sonar al instante.

---

## ⚡ Instalación del comando `mopet` en la Terminal

Para que **cualquier usuario** pueda abrir el programa simplemente escribiendo `mopet` en su terminal:

### Opción A: Para TODOS los usuarios del sistema (Recomendada en Linux)
Instala el comando en `/usr/local/bin/mopet` (disponible globalmente para cualquier usuario de la máquina):
```bash
sudo ./install.sh
# o también: sudo make install
```

### Opción B: Para el usuario actual (Sin permisos de administrador)
Instala en `$HOME/.cargo/bin` y `$HOME/.local/bin`:
```bash
./install.sh
# o también: cargo install --path .
```
*(El script `./install.sh` configura automáticamente tus variables de entorno y alias en `~/.bashrc` y `~/.zshrc`).*

---

## 🖥️ Flujos de Uso desde la Terminal

### Flujo 1: Menú Interactivo Guiado (Sin memorizar comandos)
Simplemente escribe:

```bash
mopet
# o si estás en la carpeta: ./mopet.sh
```

Se desplegará el menú interactivo:
```text
  [1] 📁 Reproducir una carpeta completa (MP3, MP4, WAV, FLAC)
  [2] 📋 Cargar un plan guardado (mopet_plan.json)
  [3] 🎵 Armar una cola con canciones específicas (1 a 1)
  [4] 🎛️  Modo Multipista (reproducir 2 o más audios al mismo tiempo)
  [5] ⚡ Probar reproductor (Modo Demo generativo)
  [6] ❌ Salir
```
Solo seleccionas el número y MOPET se encarga del resto.

---

### Flujo 2: Reproducir una Carpeta Completa de Música
Escanea recursivamente todos los audios y videos MP4, arma el plan y lo guarda automáticamente:

```bash
mopet /home/tu_usuario/Musica
```

---

### Flujo 3: Armar un Plan de Canciones (Cola 1 a 1 con mínimo consumo de RAM)
Pasa las canciones que deseas escuchar en orden:

```bash
mopet cancion1.mp3 video.mp4 cancion2.wav
```
* **Consumo de RAM constante (< 25 MB)**: Solo se decodifica en memoria la canción que suena. Al terminar o avanzar, el buffer anterior se destruye de inmediato.
* **Avance automático**: Cuando termina una pista, pasa automáticamente a la siguiente.

---

### Flujo 4: Cargar un Plan Guardado Previamente
```bash
mopet --plan mopet_plan.json
```

---

### Flujo 5: Modo Multipista Simultáneo (Mezclar pistas en paralelo)
Para hacer sonar 2 o más pistas al mismo tiempo (ej. stems/capas de audio):

```bash
mopet --multi voz.mp3 guitarra.wav bateria.mp4
```

---

### Flujo 6: Probar Sonido (Modo Demo)
```bash
mopet --demo
```

---

## ⌨️ Controles en Vivo dentro de la Terminal

| Tecla | Acción |
|---|---|
| **`[Espacio]`** | Pausar / Reanudar reproducción |
| **`[N]`** | **Siguiente canción** del plan (*Next*) |
| **`[B]`** | **Canción anterior** del plan (*Back / Previous*) |
| **`[T]`** | **Modo de repetición**: Secuencial (1 a 1) ➔ Repetir Todo ➔ Repetir Canción |
| **`[W]`** | **Guardar plan actual en disco** (`mopet_plan.json`) |
| **`[↑] / [↓]`** | Subir / Bajar volumen (±5%) |
| **`[←] / [→]`** | Ajustar balance estéreo / panorama (L / R) |
| **`[M]`** | Silenciar pista (**Mute**) |
| **`[S]`** | Reproducir en **Solo** (aislar audio) |
| **`[J] / [K]`** | Rebobinar / Adelantar 5 segundos |
| **`[R]`** | Reiniciar canción actual al segundo 00:00 |
| **`[+] / [-]`** | Volumen general (**Master**) |
| **`[Q] / [Esc]`** | Salir limpiamente |

---

## 🧪 Pruebas Automatizadas

```bash
cargo test --workspace
```

---

## 📚 Aprende a Programar en Rust con MOPET (5 Etapas Didácticas)

El proyecto está desglosado en **5 etapas progresivas** e incrementales en la carpeta `etapas/`. Cada etapa cuenta con su propio manual detallado y código comentado línea por línea:

1. **[Etapa 1: Decodificación Multimedia y Remuestreo](./etapas/etapa_1_decodificador/README.md)**  
   *Conceptos*: Ownership, Borrowing, Slices, Zero-Copy, Result, Symphonia y síntesis WAV en bytes.
2. **[Etapa 2: Concurrencia de Tiempo Real, Atómicos y Estado de Pista](./etapas/etapa_2_estado_pista/README.md)**  
   *Conceptos*: `Arc<T>`, `Atomic*`, `parking_lot::RwLock`, Mutabilidad Interior, Hot-Swap y exclusión de Data Races.
3. **[Etapa 3: Motor DSP, Salida de Hardware (CPAL) y Síntesis](./etapas/etapa_3_motor_dsp/README.md)**  
   *Conceptos*: Callbacks de baja latencia con CPAL, mezcla SIMD, soft-limiter $\tanh(x)$ y sintetizadores procedurales a 120 BPM.
4. **[Etapa 4: Planes de Reproducción, Serialización JSON y Lazy Loading](./etapas/etapa_4_planes_cola/README.md)**  
   *Conceptos*: Serde `derive`, `BufReader`/`BufWriter`, escaneo recursivo y memoria constante (< 25 MB RAM).
5. **[Etapa 5: Interfaz de Terminal Interactiva (TUI) y Flujo CLI](./etapas/etapa_5_interfaz_tui/README.md)**  
   *Conceptos*: `crossterm`, modo crudo (*Raw Mode*), animación a 30 FPS sin parpadeos y parser FSM con comillas.

# RUST-PROYECTS
