//! Motor de audio de TeatroPlayer.
//!
//! No es un motor de sonido propio: es la capa fina que orquesta rodio.
//! Ver ADR-002 en `Docs/02-stack-tecnico.md` §7.

pub mod backend;
pub mod envelope;
pub mod live;
pub mod model;
pub mod rodio_backend;
