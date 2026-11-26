use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RateLimit(Mutex<(Option<Instant>, usize)>);

impl RateLimit {
    pub const fn new() -> Self {
        Self(Mutex::new((None, 0)))
    }

    /// The next reported event includes the count of intervening events.
    pub fn allow(&self) -> Option<usize> {
        let mut state = self.0.lock().unwrap();
        let now = Instant::now();
        if state
            .0
            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(1))
        {
            state.1 += 1;
            return None;
        }
        state.0 = Some(now);
        Some(std::mem::take(&mut state.1))
    }
}
