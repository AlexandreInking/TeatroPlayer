//! TeatroPlayer — núcleo.
//!
//! Este crate es una librería además de un binario: así el motor de audio, el
//! formato de paquete y la sesión se pueden testear sin abrir una ventana ni
//! tocar la tarjeta de sonido.
//!
//! Sin IA, 100 % determinista: ver `Docs/12-determinismo-y-sin-ia.md`.

pub mod diagnostico;
pub mod engine;
pub mod eventos;
pub mod paquete;
pub mod serde_util;
pub mod sesion;
pub mod show;
