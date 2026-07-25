//! Event / notification bus capability provider (CPE-270).
//!
//! A brokered channel for a sidecar to emit notifications/status/progress *to* the
//! host, and for the host to send lifecycle signals *to* a sidecar. Everything is
//! host-mediated — there is no sidecar-to-sidecar delivery — which preserves
//! isolation. A sidecar must hold `Capability::Events` for its emissions to be
//! delivered; ungranted events are dropped.

use sidecar_contract::{Envelope, Event, HostSignal, Level, Message};

/// The host's handler for events a sidecar emits. Implemented by the explorer to turn
/// these into toasts, progress bars, and status badges.
pub trait EventSink: Send + Sync {
    fn notify(&self, sidecar_id: &str, level: Level, message: &str);
    fn progress(&self, sidecar_id: &str, id: &str, fraction: f32);
    fn status(&self, sidecar_id: &str, state: &str);
}

/// Routes sidecar-emitted [`Event`]s to a host [`EventSink`], enforcing the Events
/// grant, and encodes host→sidecar [`HostSignal`]s.
pub struct EventRouter<S: EventSink> {
    sink: S,
}

impl<S: EventSink> EventRouter<S> {
    pub fn new(sink: S) -> Self {
        Self { sink }
    }

    /// Forward one event from `sidecar_id` to the sink. `granted` is whether the
    /// sidecar holds `Capability::Events`. Returns `false` (dropped) if not granted.
    pub fn deliver(&self, sidecar_id: &str, granted: bool, event: &Event) -> bool {
        if !granted {
            return false;
        }
        match event {
            Event::Notify { level, message } => self.sink.notify(sidecar_id, *level, message),
            Event::Progress { id, fraction } => self.sink.progress(sidecar_id, id, *fraction),
            Event::Status { state } => self.sink.status(sidecar_id, state),
        }
        true
    }
}

/// Encode a host→sidecar signal as an [`Envelope`] to write over the connection.
/// Signals are unsolicited, so they use correlation id `0`.
pub fn signal_envelope(signal: HostSignal) -> Envelope {
    Envelope::new(0, Message::Signal(signal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<String>>,
    }
    impl EventSink for RecordingSink {
        fn notify(&self, sidecar_id: &str, level: Level, message: &str) {
            self.events
                .lock()
                .unwrap()
                .push(format!("notify:{sidecar_id}:{level:?}:{message}"));
        }
        fn progress(&self, sidecar_id: &str, id: &str, fraction: f32) {
            self.events
                .lock()
                .unwrap()
                .push(format!("progress:{sidecar_id}:{id}:{fraction}"));
        }
        fn status(&self, sidecar_id: &str, state: &str) {
            self.events
                .lock()
                .unwrap()
                .push(format!("status:{sidecar_id}:{state}"));
        }
    }

    #[test]
    fn granted_events_are_forwarded_by_kind() {
        let router = EventRouter::new(RecordingSink::default());
        assert!(router.deliver("s1", true, &Event::Notify { level: Level::Warn, message: "hi".into() }));
        assert!(router.deliver("s1", true, &Event::Progress { id: "job".into(), fraction: 0.25 }));
        assert!(router.deliver("s1", true, &Event::Status { state: "busy".into() }));
        let recorded = router.sink.events.lock().unwrap().clone();
        assert_eq!(
            recorded,
            vec![
                "notify:s1:Warn:hi".to_string(),
                "progress:s1:job:0.25".to_string(),
                "status:s1:busy".to_string(),
            ]
        );
    }

    #[test]
    fn ungranted_events_are_dropped() {
        let router = EventRouter::new(RecordingSink::default());
        let delivered = router.deliver("s1", false, &Event::Status { state: "x".into() });
        assert!(!delivered);
        assert!(router.sink.events.lock().unwrap().is_empty());
    }

    #[test]
    fn host_signals_encode_and_round_trip() {
        let env = signal_envelope(HostSignal::ThemeChanged { dark: true });
        let back = Envelope::from_json(&env.to_json().unwrap()).unwrap();
        assert!(matches!(
            back.message,
            Message::Signal(HostSignal::ThemeChanged { dark: true })
        ));
        assert_eq!(back.id, 0);
    }
}
