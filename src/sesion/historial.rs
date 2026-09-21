//! Deshacer / rehacer (T-UI-008).
//!
//! Es genérico a propósito: guarda **instantáneas del estado completo**, no
//! "comandos inversos". Con una sesión de teatro (un puñado de entradas) una
//! instantánea es barata y, sobre todo, **no se puede equivocar**: no hay que
//! escribir ni mantener la operación inversa de cada cambio.
//!
//! Regla de uso: llamar a `registrar` **antes** de modificar el estado.

/// Historial de instantáneas.
#[derive(Debug)]
pub struct Historial<T> {
    pasado: Vec<T>,
    futuro: Vec<T>,
    limite: usize,
}

impl<T: Clone> Historial<T> {
    /// `limite` es cuántos pasos se recuerdan. Cuando se supera, se olvida el
    /// más viejo: sin tope, una sesión larga acumularía memoria para nada.
    pub fn nuevo(limite: usize) -> Self {
        Self { pasado: Vec::new(), futuro: Vec::new(), limite: limite.max(1) }
    }

    /// Guarda el estado tal como está **ahora**, antes de cambiarlo.
    ///
    /// También vacía el futuro: si deshaces y luego haces algo nuevo, rehacer
    /// deja de tener sentido.
    pub fn registrar(&mut self, estado: &T) {
        self.pasado.push(estado.clone());
        if self.pasado.len() > self.limite {
            self.pasado.remove(0);
        }
        self.futuro.clear();
    }

    pub fn puede_deshacer(&self) -> bool {
        !self.pasado.is_empty()
    }

    pub fn puede_rehacer(&self) -> bool {
        !self.futuro.is_empty()
    }

    /// Devuelve el estado al que hay que volver, o `None` si no hay nada.
    ///
    /// `actual` se guarda para poder rehacer después.
    pub fn deshacer(&mut self, actual: &T) -> Option<T> {
        let anterior = self.pasado.pop()?;
        self.futuro.push(actual.clone());
        Some(anterior)
    }

    pub fn rehacer(&mut self, actual: &T) -> Option<T> {
        let siguiente = self.futuro.pop()?;
        self.pasado.push(actual.clone());
        Some(siguiente)
    }

    pub fn limpiar(&mut self) {
        self.pasado.clear();
        self.futuro.clear();
    }

    pub fn pasos(&self) -> (usize, usize) {
        (self.pasado.len(), self.futuro.len())
    }
}

impl<T: Clone> Default for Historial<T> {
    fn default() -> Self {
        Self::nuevo(50)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cinco_cambios_cinco_undos_y_cinco_redos() {
        // El estado es un número: lo que importa es el mecanismo, no el tipo.
        let mut h: Historial<i32> = Historial::nuevo(50);
        let mut estado = 0;

        for paso in 1..=5 {
            h.registrar(&estado); // antes de cambiar
            estado = paso;
        }
        assert_eq!(estado, 5);

        // 5 undos -> estado inicial
        for _ in 0..5 {
            if let Some(anterior) = h.deshacer(&estado) {
                estado = anterior;
            }
        }
        assert_eq!(estado, 0, "tras 5 undos debe volver al inicio");
        assert!(!h.puede_deshacer());

        // 5 redos -> estado final
        for _ in 0..5 {
            if let Some(siguiente) = h.rehacer(&estado) {
                estado = siguiente;
            }
        }
        assert_eq!(estado, 5, "tras 5 redos debe volver al final");
        assert!(!h.puede_rehacer());
    }

    #[test]
    fn deshacer_sin_nada_no_hace_nada() {
        let mut h: Historial<i32> = Historial::nuevo(10);
        assert!(!h.puede_deshacer());
        assert!(h.deshacer(&1).is_none());
        assert!(h.rehacer(&1).is_none());
    }

    #[test]
    fn un_cambio_nuevo_borra_el_futuro() {
        let mut h: Historial<i32> = Historial::nuevo(10);
        let mut estado = 1;
        h.registrar(&estado);
        estado = 2;
        // deshacer -> 1, y ahora cambiar a 3 en vez de rehacer
        estado = h.deshacer(&estado).unwrap();
        assert_eq!(estado, 1);
        assert!(h.puede_rehacer());

        h.registrar(&estado);
        estado = 3;
        assert_eq!(estado, 3);
        assert!(!h.puede_rehacer(), "un cambio nuevo invalida el futuro");
    }

    #[test]
    fn el_limite_de_pasos_se_respeta() {
        let mut h: Historial<i32> = Historial::nuevo(3);
        for i in 0..10 {
            h.registrar(&i);
        }
        assert_eq!(h.pasos().0, 3, "sólo debe recordar 3 pasos");
    }

    #[test]
    fn funciona_con_estructuras_no_tan_triviales() {
        // Una sesión son entradas; se prueba con un vector de nombres.
        let mut h: Historial<Vec<String>> = Historial::nuevo(10);
        let mut estado = vec!["a".to_string()];
        h.registrar(&estado);
        estado.push("b".to_string());
        h.registrar(&estado);
        estado.remove(0);

        estado = h.deshacer(&estado).unwrap();
        assert_eq!(estado, vec!["a".to_string(), "b".to_string()]);
        estado = h.deshacer(&estado).unwrap();
        assert_eq!(estado, vec!["a".to_string()]);
    }
}
