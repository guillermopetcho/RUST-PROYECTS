// Submódulo público de decodificación y remuestreo de contenedores de audio (MP3, MP4, WAV, FLAC, etc.)
pub mod decoder;
// Submódulo público del motor DSP en tiempo real y salida de sonido con CPAL
pub mod engine;
// Submódulo público de planes de reproducción, listas de canciones y serialización JSON
pub mod plan;
// Submódulo público de generadores procedurales de audio de demostración
pub mod synth;
// Submódulo público de pistas concurrentes con atómicos y soporte Hot-Swap
pub mod track;

// Re-exportamos la estructura del motor de audio de baja latencia
#[allow(unused_imports)]
pub use engine::AudioEngine;
// Re-exportamos los tipos del plan de reproducción
#[allow(unused_imports)]
pub use plan::{AudioPlan, PlanItem, RepeatMode};
// Re-exportamos la estructura de pista atómica
#[allow(unused_imports)]
pub use track::Track;
/*
 * ROL DEL ARCHIVO `audio/mod.rs`:
 * Actúa como punto de convergencia y fachada (Facade Pattern) para el subsistema de audio de MOPET,
 * exponiendo ordenadamente el decodificador, motor DSP, generadores sintéticos, pistas y planes.
 */
