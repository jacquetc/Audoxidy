//! Puente entre el canal de control externo y las suscripciones de Iced.
//!
//! El canal tipado (`integrations::control`) no depende de ninguna interfaz;
//! este módulo adapta su receptor a un stream de `iced::futures` y le da una
//! identidad estable para `Subscription::run_with`.

use crate::integrations::control::ExternalControlEvent;

/// Receptor envuelto para usarse como identidad de una suscripción de iced.
///
/// La identidad de la receta (`Hash`) debe ser constante: si cambiara entre
/// evaluaciones de `subscription()`, el runtime cancelaría y volvería a
/// arrancar el stream, creando un hilo puente nuevo en cada actualización.
/// Por eso el hash solo incluye el `TypeId` del evento y nunca el estado del
/// canal.
#[derive(Clone)]
pub struct ExternalControlReceiver(pub crossbeam::channel::Receiver<ExternalControlEvent>);

impl std::hash::Hash for ExternalControlReceiver {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::any::TypeId::of::<ExternalControlEvent>().hash(state);
    }
}

/// Adapta el receptor síncrono a un stream consumible por una suscripción de iced.
///
/// Un hilo puente dedicado se encarga del `recv()` bloqueante y reenvía cada
/// evento al canal asíncrono ilimitado; así ningún worker de tokio queda
/// bloqueado esperando eventos. Si el stream de la suscripción se reemplaza o
/// se descarta, `unbounded_send` falla y el hilo puente termina.
pub fn external_control_stream(
    rx: crossbeam::channel::Receiver<ExternalControlEvent>,
) -> impl iced::futures::Stream<Item = ExternalControlEvent> + Send + 'static {
    let (tx, out) = iced::futures::channel::mpsc::unbounded();

    // Si el hilo no puede crearse, `tx` se descarta con él y `out` termina:
    // no se propaga un pánico desde el camino de la suscripción. El fallo se
    // registra a nivel error porque el puente de control externo quedaría muerto.
    let spawn_result = std::thread::Builder::new()
        .name("ext-control-bridge".into())
        .spawn(move || {
            while let Ok(event) = rx.recv() {
                if tx.unbounded_send(event).is_err() {
                    // El stream de la suscripción fue reemplazado/dropeado.
                    break;
                }
            }
        });
    if let Err(e) = spawn_result {
        tracing::error!("No se pudo crear el hilo puente de control externo: {e}");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiver_hash_is_stable() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let (_tx_a, rx_a) = crossbeam::channel::unbounded();
        let (_tx_b, rx_b) = crossbeam::channel::unbounded();

        let mut hasher_clone = DefaultHasher::new();
        ExternalControlReceiver(rx_a.clone()).hash(&mut hasher_clone);
        let mut hasher_same = DefaultHasher::new();
        ExternalControlReceiver(rx_a).hash(&mut hasher_same);
        let mut hasher_other = DefaultHasher::new();
        ExternalControlReceiver(rx_b).hash(&mut hasher_other);

        assert_eq!(hasher_clone.finish(), hasher_same.finish());
        assert_eq!(hasher_clone.finish(), hasher_other.finish());
    }

    #[test]
    fn the_stream_forwards_events_and_ends_when_the_sender_is_dropped() {
        use iced::futures::StreamExt;

        let (tx, rx) = crate::integrations::control::external_control_channel();
        let mut stream = Box::pin(external_control_stream(rx));
        tx.send(ExternalControlEvent::Toggle);
        assert_eq!(
            iced::futures::executor::block_on(stream.next()),
            Some(ExternalControlEvent::Toggle)
        );

        drop(tx);
        assert_eq!(iced::futures::executor::block_on(stream.next()), None);
    }
}
