//! Ayudas de serialización compartidas entre el motor y la sesión.
//!
//! Están aquí y no dentro de cada módulo porque el motor (`CueSpec`) y la
//! sesión (`Cue`, `AudioRef`) tienen que escribir las duraciones y los
//! decibelios exactamente igual.

use std::time::Duration;

use serde::{Deserialize, Deserializer, Serializer};

use crate::engine::model::MilliDb;

/// `Duration` <-> milisegundos enteros.
///
/// Enteros y no segundos con decimales: dos guardados del mismo espectáculo
/// deben producir el mismo texto (ver `Docs/12`).
pub mod ms {
    use super::*;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_millis() as u64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_millis(u64::deserialize(d)?))
    }
}

/// `MilliDb` <-> decibelios con decimales.
///
/// En memoria son milidecibelios enteros; en el JSON salen como `-3.0`, que es
/// lo que especifica `Docs/04`.
pub mod db {
    use super::*;

    pub fn serialize<S: Serializer>(v: &MilliDb, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(v.as_db() as f64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<MilliDb, D::Error> {
        Ok(MilliDb::from_db(f64::deserialize(d)? as f32))
    }
}
