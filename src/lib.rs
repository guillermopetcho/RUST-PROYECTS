// Módulo público de procesamiento, decodificación y mezcla de audio
pub mod audio;
// Módulo público de la interfaz gráfica de terminal e interacción
pub mod ui;

// Re-exportamos los tipos y estructuras principales para facilitar el uso de MOPET como biblioteca en Rust
pub use audio::{decoder, engine::AudioEngine, plan, synth, AudioPlan, PlanItem, RepeatMode, Track};
// Re-exportamos la estructura de interfaz de terminal interactiva
pub use ui::TerminalUi;
/*
 * ROL DEL ARCHIVO `lib.rs`:
 * Define la raíz de la biblioteca (crate library) de MOPET. Permite que otros proyectos
 * o etapas importen los módulos del reproductor y motor DSP como dependencias limpias.
 */
