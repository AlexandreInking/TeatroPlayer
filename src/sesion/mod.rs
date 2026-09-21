//! Modelo de sesión y persistencia (hito SES).

pub mod guardado;
pub mod historial;
pub mod links;
pub mod modelo;

pub use guardado::{cargar_sesion, guardar_sesion, AutoSaver, Estado};
pub use historial::Historial;
pub use modelo::{Cue, ModoApertura, Sesion, VERSION};
