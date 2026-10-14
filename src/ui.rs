// Importamos los submódulos de crossterm para manipulación avanzada de la terminal en modo crudo (Raw Mode)
use crossterm::{
    // Módulo para reposicionamiento del cursor en coordenadas (x, y) y control de visibilidad
    cursor,
    // Módulo para captura de eventos del teclado, ratón y redimensionamiento de ventana
    event::{self, Event, KeyCode, KeyEventKind},
    // Macro execute! para enviar comandos ANSI de terminal en lote
    execute,
    // Módulo de estilos ANSI: colores de primer plano, fondo y reinicio de formato
    style::{Color, Print, ResetColor, SetForegroundColor},
    // Módulo para alternar entre la pantalla principal y la pantalla alterna del emulador de terminal
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
// Importamos el acceso al búfer estándar de salida `stdout` y el trait `Write`
use std::io::{stdout, Write};
// Importamos el orden atómico relajado para lecturas instantáneas sin bloqueos
use std::sync::atomic::Ordering;
// Importamos utilidades de medición de tiempo de alta precisión (`Duration` e `Instant`)
use std::time::{Duration, Instant};

// Importamos el módulo de decodificación multimedia de Symphonia
use crate::audio::decoder;
// Importamos la estructura del motor de mezcla de audio en tiempo real
use crate::audio::engine::AudioEngine;
// Importamos las estructuras de datos de planes de reproducción
use crate::audio::plan::{AudioPlan, PlanItem};
// Importamos la estructura de pista atómica
use crate::audio::track::Track;

/// Gestor de la interfaz interactiva de terminal basada en texto (TUI: Text User Interface).
/// Renderiza a 30 FPS en una pantalla alterna (sin ensuciar el historial de la terminal del usuario)
/// y captura pulsaciones de teclado en modo no bloqueante.
pub struct TerminalUi {
    // Índice de la pista actualmente enfocada/seleccionada para ajustes de volumen, panorama o mute
    selected_track_idx: usize,
    // Marca de tiempo del último cuadro visual dibujado en la pantalla
    last_draw: Instant,
    // Ruta de archivo predeterminada donde se persistirá el plan de audio (JSON)
    default_save_path: String,
}
/*
 * ROL DE LA ESTRUCTURA `TerminalUi`:
 * Centraliza el estado de la interfaz de usuario: qué pista tiene el foco del cursor,
 * cuándo redibujar los vúmetros a 30 FPS y dónde guardar los planes de reproducción.
 */

#[allow(dead_code)]
impl TerminalUi {
    /// Construye una nueva instancia de la interfaz de usuario inicializando los contadores
    pub fn new() -> Self {
        // Inicializamos los campos con sus valores por defecto
        Self {
            // La primera pista (índice 0) está seleccionada inicialmente
            selected_track_idx: 0,
            // Inicializamos el temporizador de renderizado al instante actual
            last_draw: Instant::now(),
            // Nombre predeterminado del archivo JSON de persistencia
            default_save_path: "mopet_plan.json".to_string(),
        }
    }
    /*
     * ROL DEL CONSTRUCTOR `new`:
     * Crea el estado inicial de la TUI listo para acoplarse con cualquier motor de audio.
     */

    /// Permite sobreescribir la ruta del archivo JSON donde se guardará el plan
    pub fn set_save_path(&mut self, path: String) {
        // Asignamos la nueva ruta personalizada
        self.default_save_path = path;
    }
    /*
     * ROL DEL MÉTODO `set_save_path`:
     * Permite a la CLI configurar nombres de archivo personalizados si se usan banderas como `--plan`.
     */

    /// Ejecuta el bucle de eventos principal de la interfaz interactiva.
    /// Toma el control de la terminal, captura pulsaciones de teclas en tiempo real,
    /// coordina el avance automático de canciones y redibuja la pantalla a 30 FPS.
    pub fn run(
        &mut self,
        engine: &AudioEngine,
        mut plan: Option<&mut AudioPlan>,
    ) -> anyhow::Result<()> {
        // Activamos el "Raw Mode": las pulsaciones de teclado llegan inmediatamente sin requerir presionar Enter
        enable_raw_mode()?;
        // Obtenemos el descriptor del canal estándar de salida
        let mut out = stdout();
        // Cambiamos a la pantalla alternativa (Alternate Screen) y ocultamos el cursor parpadeante
        execute!(out, EnterAlternateScreen, cursor::Hide)?;

        // Bandera de control para mantener vivo el bucle de ejecución
        let mut running = true;
        // Mensaje de estado informativo inicial para la barra inferior
        let mut status_msg = "Reproductor iniciado. [Espacio] Play/Pausa | [W] Guardar Plan | [Q] Salir".to_string();
        // Temporizador para medir el tiempo de visualización del mensaje de estado
        let mut status_timer = Instant::now();

        // Bucle interactivo principal
        while running {
            // 1. Verificación de finalización de canción para avance automático en el plan (Lazy Loading)
            if let Some(ref mut p) = plan {
                // Obtenemos la pista activa en el motor (ranura 0)
                if let Some(track) = engine.tracks.first() {
                    // Verificamos si la pista terminó de reproducirse
                    if track.is_finished() {
                        // Solicitamos la siguiente canción al plan según el modo de repetición
                        if let Some(next_item) = p.next() {
                            // Clonamos el elemento de destino
                            let next_clone = next_item.clone();
                            // Decodificamos y cargamos en caliente la nueva canción en el búfer de la pista
                            match load_song_into_track(track, &next_clone, engine.sample_rate) {
                                // En caso de éxito, actualizamos el mensaje de estado
                                Ok(_) => {
                                    status_msg = format!("Plan: Reproduciendo siguiente -> {}", next_clone.title);
                                    status_timer = Instant::now();
                                }
                                // En caso de error de lectura o decodificación, informamos en la barra de estado
                                Err(e) => {
                                    status_msg = format!("Error cargando {}: {}", next_clone.title, e);
                                    status_timer = Instant::now();
                                }
                            }
                        } else {
                            // Se alcanzó el final de la lista en modo secuencial: pausamos la pista
                            track.set_paused(true);
                            // Notificamos que el plan ha concluido
                            status_msg = "Plan completado. Todas las canciones han finalizado.".to_string();
                            status_timer = Instant::now();
                        }
                    }
                }
            }
            /*
             * ROL DEL SUB-BLOQUE DE AVANCE AUTOMÁTICO:
             * Funciona como un DJ automatizado. Al detectar que el último cuadro de audio ha sonado,
             * solicita la siguiente pista al planificador y realiza el reemplazo en caliente (Hot-Swap)
             * sin requerir intervención manual del usuario.
             */

            // 2. Consulta no bloqueante de eventos de teclado con timeout de 25 ms
            if event::poll(Duration::from_millis(25))? {
                // Leemos el evento disponible
                if let Event::Key(key) = event::read()? {
                    // Procesamos únicamente el evento de pulsación inicial (ignora repeticiones por soltado)
                    if key.kind == KeyEventKind::Press {
                        // Evaluamos el código de la tecla presionada
                        match key.code {
                            // Tecla Q o Esc: salir del programa
                            KeyCode::Char('q') | KeyCode::Esc => {
                                // Desactivamos la bandera del bucle para terminar
                                running = false;
                            }
                            // Barra espaciadora: alternar Play / Pausa general (Master)
                            KeyCode::Char(' ') => {
                                // Conmutamos la pausa en el motor de audio
                                engine.toggle_master_pause();
                                // Verificamos el nuevo estado de pausa
                                let is_p = engine.master_paused.load(Ordering::Relaxed);
                                // Actualizamos el mensaje de estado
                                status_msg = if is_p {
                                    "Master en PAUSA".to_string()
                                } else {
                                    "Master en REPRODUCCIÓN".to_string()
                                };
                                // Reiniciamos el temporizador del mensaje
                                status_timer = Instant::now();
                            }
                            // Tecla N: saltar a la siguiente canción en la lista del plan
                            KeyCode::Char('n') | KeyCode::Char('N') => {
                                if let Some(ref mut p) = plan {
                                    if let Some(track) = engine.tracks.first() {
                                        if let Some(next_item) = p.next() {
                                            let next_clone = next_item.clone();
                                            let _ = load_song_into_track(track, &next_clone, engine.sample_rate);
                                            status_msg = format!("Siguiente canción [{}/{}]: {}", p.current_index + 1, p.items.len(), next_clone.title);
                                            status_timer = Instant::now();
                                        }
                                    }
                                }
                            }
                            // Tecla B: volver a la canción anterior en la lista del plan
                            KeyCode::Char('b') | KeyCode::Char('B') => {
                                if let Some(ref mut p) = plan {
                                    if let Some(track) = engine.tracks.first() {
                                        if let Some(prev_item) = p.prev() {
                                            let prev_clone = prev_item.clone();
                                            let _ = load_song_into_track(track, &prev_clone, engine.sample_rate);
                                            status_msg = format!("Canción anterior [{}/{}]: {}", p.current_index + 1, p.items.len(), prev_clone.title);
                                            status_timer = Instant::now();
                                        }
                                    }
                                }
                            }
                            // Tecla T: alternar modo de repetición (Secuencial -> LoopAll -> LoopSingle)
                            KeyCode::Char('t') | KeyCode::Char('T') => {
                                if let Some(ref mut p) = plan {
                                    p.repeat_mode = p.repeat_mode.next_mode();
                                    status_msg = format!("Modo Plan cambiado a: {}", p.repeat_mode.label());
                                    status_timer = Instant::now();
                                }
                            }
                            // Tecla W: guardar el plan actual con todas las pistas en archivo JSON
                            KeyCode::Char('w') | KeyCode::Char('W') => {
                                if let Some(ref p) = plan {
                                    match p.save_to_file(&self.default_save_path) {
                                        Ok(_) => {
                                            status_msg = format!("¡Plan guardado exitosamente en '{}'!", self.default_save_path);
                                        }
                                        Err(e) => {
                                            status_msg = format!("Error guardando plan: {}", e);
                                        }
                                    }
                                    status_timer = Instant::now();
                                } else {
                                    status_msg = "No hay un plan activo para guardar.".to_string();
                                    status_timer = Instant::now();
                                }
                            }
                            // Dígitos del 1 al 9: seleccionar la pista correspondiente para control individual
                            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                                let idx = c as usize - '1' as usize;
                                if idx < engine.tracks.len() {
                                    self.selected_track_idx = idx;
                                    status_msg = format!("Seleccionada pista #{}", idx + 1);
                                    status_timer = Instant::now();
                                }
                            }
                            // Flecha Arriba: aumentar volumen de la pista seleccionada en +5%
                            KeyCode::Up => {
                                if let Some(track) = engine.tracks.get(self.selected_track_idx) {
                                    track.adjust_volume(0.05);
                                    let v = track.volume.load(Ordering::Relaxed);
                                    status_msg = format!("Pista #{}: Volumen al {:.0}%", track.id + 1, v * 100.0);
                                    status_timer = Instant::now();
                                }
                            }
                            // Flecha Abajo: disminuir volumen de la pista seleccionada en -5%
                            KeyCode::Down => {
                                if let Some(track) = engine.tracks.get(self.selected_track_idx) {
                                    track.adjust_volume(-0.05);
                                    let v = track.volume.load(Ordering::Relaxed);
                                    status_msg = format!("Pista #{}: Volumen al {:.0}%", track.id + 1, v * 100.0);
                                    status_timer = Instant::now();
                                }
                            }
                            // Flecha Izquierda: balance estéreo hacia la izquierda (-0.1)
                            KeyCode::Left => {
                                if let Some(track) = engine.tracks.get(self.selected_track_idx) {
                                    track.adjust_pan(-0.1);
                                    let p = track.pan.load(Ordering::Relaxed);
                                    status_msg = format!("Pista #{}: Panorama en {:.2}", track.id + 1, p);
                                    status_timer = Instant::now();
                                }
                            }
                            // Flecha Derecha: balance estéreo hacia la derecha (+0.1)
                            KeyCode::Right => {
                                if let Some(track) = engine.tracks.get(self.selected_track_idx) {
                                    track.adjust_pan(0.1);
                                    let p = track.pan.load(Ordering::Relaxed);
                                    status_msg = format!("Pista #{}: Panorama en {:.2}", track.id + 1, p);
                                    status_timer = Instant::now();
                                }
                            }
                            // Tecla M: alternar silencio (Mute) en la pista seleccionada
                            KeyCode::Char('m') | KeyCode::Char('M') => {
                                if let Some(track) = engine.tracks.get(self.selected_track_idx) {
                                    track.toggle_mute();
                                    let m = track.muted.load(Ordering::Relaxed);
                                    status_msg = format!("Pista #{}: {}", track.id + 1, if m { "MUTED" } else { "UNMUTED" });
                                    status_timer = Instant::now();
                                }
                            }
                            // Tecla S: alternar modo Solo en la pista seleccionada
                            KeyCode::Char('s') | KeyCode::Char('S') => {
                                if let Some(track) = engine.tracks.get(self.selected_track_idx) {
                                    track.toggle_solo();
                                    let s = track.solo.load(Ordering::Relaxed);
                                    status_msg = format!("Pista #{}: {}", track.id + 1, if s { "SOLO ACTIVADO" } else { "SOLO DESACTIVADO" });
                                    status_timer = Instant::now();
                                }
                            }
                            // Tecla P: alternar pausa individual de la pista seleccionada
                            KeyCode::Char('p') | KeyCode::Char('P') => {
                                if let Some(track) = engine.tracks.get(self.selected_track_idx) {
                                    track.toggle_pause();
                                    let p = track.paused.load(Ordering::Relaxed);
                                    status_msg = format!("Pista #{}: {}", track.id + 1, if p { "Pausada" } else { "Reanudada" });
                                    status_timer = Instant::now();
                                }
                            }
                            // Tecla L: alternar bucle continuo en la pista seleccionada
                            KeyCode::Char('l') | KeyCode::Char('L') => {
                                if let Some(track) = engine.tracks.get(self.selected_track_idx) {
                                    track.toggle_loop();
                                    let lp = track.looping.load(Ordering::Relaxed);
                                    status_msg = format!("Pista #{}: Bucle {}", track.id + 1, if lp { "ON" } else { "OFF" });
                                    status_timer = Instant::now();
                                }
                            }
                            // Tecla R: reiniciar reproducción al principio (00:00) en todas las pistas
                            KeyCode::Char('r') | KeyCode::Char('R') => {
                                engine.restart_all();
                                status_msg = "Reiniciada posición al segundo 00:00".to_string();
                                status_timer = Instant::now();
                            }
                            // Tecla J: rebobinar 5 segundos hacia atrás (-5s)
                            KeyCode::Char('j') | KeyCode::Char('J') => {
                                engine.seek_all_rel(-5.0);
                                status_msg = "Rebobinar -5 segundos".to_string();
                                status_timer = Instant::now();
                            }
                            // Tecla K: adelantar 5 segundos hacia adelante (+5s)
                            KeyCode::Char('k') | KeyCode::Char('K') => {
                                engine.seek_all_rel(5.0);
                                status_msg = "Adelantar +5 segundos".to_string();
                                status_timer = Instant::now();
                            }
                            // Teclas + o =: subir volumen general (Master) en +5%
                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                engine.adjust_master_volume(0.05);
                                let mv = engine.master_volume.load(Ordering::Relaxed);
                                status_msg = format!("Master Volumen: {:.0}%", mv * 100.0);
                                status_timer = Instant::now();
                            }
                            // Teclas - o _: bajar volumen general (Master) en -5%
                            KeyCode::Char('-') | KeyCode::Char('_') => {
                                engine.adjust_master_volume(-0.05);
                                let mv = engine.master_volume.load(Ordering::Relaxed);
                                status_msg = format!("Master Volumen: {:.0}%", mv * 100.0);
                                status_timer = Instant::now();
                            }
                            // Tecla Tab: alternar cíclicamente la pista seleccionada
                            KeyCode::Tab => {
                                if !engine.tracks.is_empty() {
                                    self.selected_track_idx = (self.selected_track_idx + 1) % engine.tracks.len();
                                }
                            }
                            // Cualquier otra tecla no mapeada se ignora silenciosamente
                            _ => {}
                        }
                    }
                }
            }
            /*
             * ROL DEL DISPATCHER DE TECLADO:
             * Procesa toda la interacción del usuario sin esperas bloqueantes. Al cambiar
             * atómicos de manera instantánea, el motor DSP en el otro hilo refleja los cambios
             * de volumen, balance, pausa o mute en el siguiente búfer de audio (~5 milisegundos).
             */

            // 3. Renderizado de la interfaz gráfica a ~30 cuadros por segundo (cada 30 milisegundos)
            if self.last_draw.elapsed() >= Duration::from_millis(30) {
                // Dibujamos la pantalla completa con los valores más recientes de los vúmetros
                self.draw(engine, plan.as_deref(), &status_msg, &status_timer)?;
                // Reiniciamos la marca de tiempo del cuadro dibujado
                self.last_draw = Instant::now();
            }
        }
        /*
         * ROL DEL BUCLE PRINCIPAL DE LA TUI:
         * Mantiene una cadencia suave de refresco de pantalla mientras atiende eventos
         * de entrada sin consumir más de un 1% de tiempo de CPU.
         */

        // Restauración limpia de la terminal: volvemos a mostrar el cursor y regresamos a la pantalla normal
        execute!(out, cursor::Show, LeaveAlternateScreen)?;
        // Restauramos el modo canónico de la terminal (salimos de raw mode)
        disable_raw_mode()?;
        // Salida exitosa
        Ok(())
    }
    /*
     * ROL DEL MÉTODO `run`:
     * Es el ciclo de vida completo de la sesión de interfaz de usuario. Al garantizar la restauración
     * de la pantalla alterna y del modo raw incluso ante salidas forzadas, evita que la terminal del usuario
     * quede en un estado roto o sin eco de texto.
     */

    /// Dibuja todos los componentes de la interfaz de texto: encabezado, vúmetros, barras de progreso y controles
    fn draw(
        &self,
        engine: &AudioEngine,
        plan: Option<&AudioPlan>,
        status_msg: &str,
        status_timer: &Instant,
    ) -> anyhow::Result<()> {
        // Obtenemos el descriptor de la salida estándar
        let mut out = stdout();
        // Movemos el cursor a la esquina superior izquierda (0, 0) para redibujar sobre el contenido anterior sin parpadeo
        execute!(out, cursor::MoveTo(0, 0))?;

        // Leemos las métricas generales del motor de audio de forma atómica
        let master_p = engine.master_paused.load(Ordering::Relaxed);
        let master_v = engine.master_volume.load(Ordering::Relaxed);
        let m_peak_l = engine.master_peak_l.load(Ordering::Relaxed);
        let m_peak_r = engine.master_peak_r.load(Ordering::Relaxed);

        // Renderizamos la caja del encabezado institucional en color Cian
        execute!(
            out,
            SetForegroundColor(Color::Cyan),
            Print("╔════════════════════════════════════════════════════════════════════════════════════════════╗\r\n"),
            Print("║                 MOPET - REPRODUCTOR MULTIPISTA ULTRA-EFICIENTE (RUST)                      ║\r\n"),
            Print("╚════════════════════════════════════════════════════════════════════════════════════════════╝\r\n"),
            ResetColor
        )?;

        // Si existe un plan activo, dibujamos el banner del plan con modo de repetición y pista actual
        if let Some(p) = plan {
            let total = p.items.len();
            let current = if total > 0 { p.current_index + 1 } else { 0 };
            execute!(
                out,
                SetForegroundColor(Color::Magenta),
                Print(format!("  PLAN: \"{}\" │ Canción [{}/{}] │ Modo: {}\r\n", p.name, current, total, p.repeat_mode.label())),
                ResetColor
            )?;

            // Si hay una siguiente canción en cola, mostramos una vista previa en gris
            if let Some(next_item) = p.items.get(p.current_index + 1) {
                execute!(
                    out,
                    SetForegroundColor(Color::DarkGrey),
                    Print(format!("  Siguiente en cola: {}\r\n", truncate_str(&next_item.title, 60))),
                    ResetColor
                )?;
            }
        }

        // Información de hardware y telemetría de memoria
        execute!(
            out,
            SetForegroundColor(Color::DarkGrey),
            Print(format!(
                "  Hardware: {} Hz │ Canales: {} │ Pistas activas: {} │ Consumo RAM aprox: ~20 MB\r\n",
                engine.sample_rate, engine.channels, engine.tracks.len()
            )),
            ResetColor
        )?;

        // Estado del Master (Reproduciendo en Verde o Pausado en Amarillo)
        let master_state_str = if master_p { "⏸ PAUSADO " } else { "▶ EN PLAY " };
        let master_color = if master_p { Color::Yellow } else { Color::Green };

        // Renderizamos las barras del vúmetro Master de 10 caracteres
        let meter_l = render_vu_meter(m_peak_l, 10);
        let meter_r = render_vu_meter(m_peak_r, 10);

        // Imprimimos la línea de control Master
        execute!(
            out,
            Print("  Master: ["),
            SetForegroundColor(master_color),
            Print(master_state_str),
            ResetColor,
            Print(format!("] │ Vol: {:>3.0}% │ VU L:[{}] R:[{}]\r\n", master_v * 100.0, meter_l, meter_r)),
            SetForegroundColor(Color::DarkGrey),
            Print("  ──────────────────────────────────────────────────────────────────────────────────────────\r\n"),
            ResetColor
        )?;

        // Iteramos y renderizamos cada pista individual en la mezcla
        for (i, track) in engine.tracks.iter().enumerate() {
            // Verificamos si esta pista es la que está seleccionada actualmente
            let is_selected = i == self.selected_track_idx;
            // Leemos atómicamente todos los estados de la pista
            let paused = track.paused.load(Ordering::Relaxed);
            let muted = track.muted.load(Ordering::Relaxed);
            let solo = track.solo.load(Ordering::Relaxed);
            let looping = track.looping.load(Ordering::Relaxed);
            let vol = track.volume.load(Ordering::Relaxed);
            let pan = track.pan.load(Ordering::Relaxed);
            let cur_sec = track.current_pos_secs();
            let dur_sec = track.duration_secs();
            let pct = track.progress_pct();

            // Leemos los picos de los vúmetros de la pista
            let pk_l = track.peak_l.load(Ordering::Relaxed);
            let pk_r = track.peak_r.load(Ordering::Relaxed);
            let t_meter_l = render_vu_meter(pk_l, 8);
            let t_meter_r = render_vu_meter(pk_r, 8);

            // Indicador de selección activa (flecha amarilla ►)
            if is_selected {
                execute!(out, SetForegroundColor(Color::Yellow), Print(" ► "), ResetColor)?;
            } else {
                execute!(out, Print("   "))?;
            }

            // Título de la pista recortado a 32 caracteres para mantener la alineación de columnas
            let track_name = track.get_name();
            let track_color = if is_selected { Color::White } else { Color::Grey };
            execute!(
                out,
                SetForegroundColor(track_color),
                Print(format!("[Pista #{}] {:<32} ", i + 1, truncate_str(&track_name, 32))),
                ResetColor
            )?;

            // Insignias visuales de estado (Badges: PLAY/PAUSA, MUTE, SOLO, LOOP)
            print_badge(&mut out, if paused { "PAUSA" } else { "PLAY" }, if paused { Color::Yellow } else { Color::Green })?;
            print_badge(&mut out, "MUTE", if muted { Color::Red } else { Color::DarkGrey })?;
            print_badge(&mut out, "SOLO", if solo { Color::Magenta } else { Color::DarkGrey })?;
            print_badge(&mut out, "LOOP", if looping { Color::Cyan } else { Color::DarkGrey })?;

            // Salto de línea hacia los detalles de la pista
            execute!(out, Print("\r\n"))?;

            // Formateo del panorama estéreo (Centro C, % Izquierda o % Derecha)
            let pan_str = if pan.abs() < 0.05 {
                "C   ".to_string()
            } else if pan < 0.0 {
                format!("{:.0}%L", pan.abs() * 100.0)
            } else {
                format!("{:.0}%R", pan * 100.0)
            };

            // Generamos la barra de progreso de 24 caracteres y formateamos los minutos:segundos
            let prog_bar = render_progress_bar(pct, 24);
            let cur_time = format_time(cur_sec);
            let dur_time = format_time(dur_sec);

            // Imprimimos la línea de métricas de la pista
            execute!(
                out,
                Print(format!(
                    "     Vol: {:>3.0}% │ Pan: {:<4} │ VU: [{}] [{}] │ {} {}/{} ({:>3.0}%)\r\n",
                    vol * 100.0,
                    pan_str,
                    t_meter_l,
                    t_meter_r,
                    prog_bar,
                    cur_time,
                    dur_time,
                    pct * 100.0
                ))
            )?;
        }
        /*
         * ROL DEL BUCLE DE RENDERIZADO DE PISTAS:
         * Genera la vista de consola de tipo mezclador de audio profesional (estilo DAW),
         * permitiendo supervisar de un solo vistazo el volumen, paneo, estado de mute/solo,
         * vúmetros dinámicos L/R y tiempo de reproducción de cada canal.
         */

        // Línea divisoria inferior
        execute!(
            out,
            SetForegroundColor(Color::DarkGrey),
            Print("  ──────────────────────────────────────────────────────────────────────────────────────────\r\n"),
            ResetColor
        )?;

        // Línea de estado interactivo: muestra notificaciones en verde durante los primeros 4.5 segundos
        let elapsed_status = status_timer.elapsed().as_secs_f32();
        if elapsed_status < 4.5 {
            execute!(
                out,
                SetForegroundColor(Color::Green),
                Print(format!("  STATUS: {}\r\n", status_msg)),
                ResetColor
            )?;
        } else {
            execute!(out, Print("  STATUS: Listo.\r\n"))?;
        }

        // Panel de ayuda con atajos de teclado disponibles
        execute!(
            out,
            SetForegroundColor(Color::DarkCyan),
            Print("  [Espacio] Play/Pausa │ [N] Sig. Canción │ [B] Canción Anterior │ [T] Modo Repetición\r\n"),
            Print("  [W] Guardar Plan (JSON) │ [↑/↓] Vol Pista │ [M] Mute │ [S] Solo │ [J/K] ±5s │ [Q] Salir\r\n"),
            ResetColor
        )?;

        // Vaciamos el búfer para proyectar todos los caracteres en la terminal inmediatamente
        out.flush()?;
        // Éxito al dibujar el fotograma
        Ok(())
    }
    /*
     * ROL DEL MÉTODO `draw`:
     * Compone y despacha el frame visual completo hacia el emulador de terminal.
     * Al mover el cursor al inicio y sobrescribir las líneas existentes sin limpiar la pantalla (`Clear`),
     * elimina el molesto parpadeo (flickering) típico de interfaces de consola en bucle.
     */
}

/// Decodifica un archivo de audio y lo intercambia en caliente dentro de la pista en memoria,
/// reiniciando el cursor de reproducción al inicio de forma inmediata.
pub fn load_song_into_track(track: &Track, item: &PlanItem, target_sr: u32) -> anyhow::Result<()> {
    // Decodificamos el archivo apuntado por la ruta a la frecuencia de muestreo del hardware
    let (samples, sr) = decoder::load_and_decode_file(&item.path, target_sr)?;
    // Realizamos el Hot-Swap de muestras y actualizamos el título de la canción
    track.swap_audio(samples, sr, item.title.clone());
    // Reiniciamos el cursor a 00:00
    track.restart();
    // Quitamos la pausa para arrancar la reproducción inmediatamente
    track.set_paused(false);
    // Operación exitosa
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `load_song_into_track`:
 * Actúa como pegamento entre el plan de reproducción y el motor de audio en vivo.
 * Libera de forma atómica los datos de la canción anterior y alimenta los nuevos samples a la pista.
 */

/// Función auxiliar para dibujar una etiqueta o badge con corchetes y color personalizado
fn print_badge<W: Write>(out: &mut W, text: &str, color: Color) -> anyhow::Result<()> {
    // Escribimos corchete de apertura, texto en color y corchete de cierre
    execute!(
        out,
        Print("["),
        SetForegroundColor(color),
        Print(text),
        ResetColor,
        Print("] ")
    )?;
    // Retornamos éxito
    Ok(())
}
/*
 * ROL DE LA FUNCIÓN `print_badge`:
 * Estandariza la apariencia visual de las etiquetas de estado (PLAY, MUTE, SOLO, LOOP).
 */

/// Renderiza una barra gráfica de vúmetro de pico con ancho configurable utilizando caracteres Unicode (■ y ·)
fn render_vu_meter(peak: f32, width: usize) -> String {
    // Calculamos cuántos caracteres deben estar encendidos en función del nivel de pico acotado a 1.0
    let filled = ((peak.min(1.0)) * width as f32).round() as usize;
    // Reservamos memoria para la cadena
    let mut s = String::with_capacity(width);
    // Llenamos los caracteres de la barra
    for i in 0..width {
        // Si el índice es menor a los caracteres llenos, añadimos bloque sólido
        if i < filled {
            s.push('■');
        } else {
            // De lo contrario, punto apagado
            s.push('·');
        }
    }
    // Retornamos la barra formada
    s
}
/*
 * ROL DE LA FUNCIÓN `render_vu_meter`:
 * Transforma un número flotante de amplitud en una visualización gráfica de vúmetro estéreo en tiempo real.
 */

/// Renderiza una barra de progreso de reproducción con caracteres [===>---]
fn render_progress_bar(pct: f32, width: usize) -> String {
    // Calculamos los caracteres llenos según el porcentaje completado [0.0, 1.0]
    let filled = ((pct.clamp(0.0, 1.0)) * width as f32).round() as usize;
    // Reservamos memoria para los caracteres más los corchetes
    let mut s = String::with_capacity(width + 2);
    // Corchete inicial
    s.push('[');
    // Recorremos la anchura de la barra
    for i in 0..width {
        if i < filled {
            // Porción ya reproducida
            s.push('=');
        } else if i == filled {
            // Cabezal actual de reproducción
            s.push('>');
        } else {
            // Porción restante
            s.push('-');
        }
    }
    // Corchete final
    s.push(']');
    // Devolvemos la barra de progreso
    s
}
/*
 * ROL DE LA FUNCIÓN `render_progress_bar`:
 * Dibuja la clásica barra de transporte de medios para saber visualmente cuánto tiempo falta para terminar la canción.
 */

/// Formatea un tiempo en segundos a formato de minutos y segundos legibles "MM:SS" (ej. "03:42")
fn format_time(seconds: f32) -> String {
    // Obtenemos los segundos enteros no negativos
    let total_secs = seconds.max(0.0) as u32;
    // Calculamos los minutos
    let mins = total_secs / 60;
    // Calculamos los segundos restantes
    let secs = total_secs % 60;
    // Generamos la cadena con formato de 2 dígitos rellenados con cero a la izquierda
    format!("{:02}:{:02}", mins, secs)
}
/*
 * ROL DE LA FUNCIÓN `format_time`:
 * Facilita la lectura de duraciones y posiciones de audio en un formato universalmente familiar.
 */

/// Trunca una cadena de texto agregando puntos suspensivos ("...") si sobrepasa la longitud máxima permitida
fn truncate_str(s: &str, max_len: usize) -> String {
    // Si la cadena excede el límite fijado
    if s.len() > max_len {
        // Tomamos el prefijo recortado restando 3 caracteres para colocar "..."
        format!("{}...", &s[..max_len.saturating_sub(3)])
    } else {
        // Si entra completa, la devolvemos intacta
        s.to_string()
    }
}
/*
 * ROL DE LA FUNCIÓN `truncate_str`:
 * Previene que títulos de archivos largos o con rutas extensas rompan el diseño tabular de la terminal.
 */
