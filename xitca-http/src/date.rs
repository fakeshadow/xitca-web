use std::{
    cell::RefCell,
    fmt::{self, Write},
    rc::Rc,
    time::{Duration, SystemTime},
};

use httpdate::HttpDate;
use tokio::{
    task::JoinHandle,
    time::{interval, Instant},
};

/// Trait for getting current date/time.
///
/// This is usally used by a low resolution of timer to reduce frequent syscall to OS.
pub trait DateTime {
    /// The size hint of slice by Self::date method.
    const DATE_VALUE_LENGTH: usize;

    /// closure would receive byte slice representation of [HttpDate].
    fn with_date<F, O>(&self, f: F) -> O
    where
        F: FnOnce(&[u8]) -> O;

    fn now(&self) -> Instant;
}

/// Struct with Date update periodically at 500 milli seconds interval.
pub(crate) struct DateTimeService {
    state: Rc<RefCell<DateTimeState>>,
    handle: JoinHandle<()>,
}

impl Drop for DateTimeService {
    fn drop(&mut self) {
        // stop the timer update async task on drop.
        self.handle.abort();
    }
}

impl DateTimeService {
    pub(crate) fn new() -> Self {
        // shared date and timer for Date and update async task.
        let state = Rc::new(RefCell::new(DateTimeState::new()));
        let state_clone = Rc::clone(&state);
        // spawn an async task sleep for 1 sec and update date in a loop.
        // handle is used to stop the task on Date drop.
        let handle = tokio::task::spawn_local(async move {
            let mut interval = interval(Duration::from_millis(500));
            let state = &*state_clone;
            loop {
                let _ = interval.tick().await;
                *state.borrow_mut() = DateTimeState::new();
            }
        });

        Self { state, handle }
    }

    #[inline(always)]
    pub(crate) fn get(&self) -> &DateTimeHandle {
        &*self.state
    }

    #[cfg(feature = "http2")]
    #[inline(always)]
    pub(crate) fn get_shared(&self) -> &SharedDateTimeHandle {
        &self.state
    }
}

pub(crate) type DateTimeHandle = RefCell<DateTimeState>;

#[cfg(feature = "http2")]
pub(crate) type SharedDateTimeHandle = Rc<DateTimeHandle>;

/// The length of byte representation of [HttpDate].
pub const DATE_VALUE_LENGTH: usize = 29;

/// struct contains byte representation of [HttpDate] and [Instant].
#[derive(Copy, Clone)]
pub struct DateTimeState {
    pub date: [u8; DATE_VALUE_LENGTH],
    pub now: Instant,
}

impl Default for DateTimeState {
    fn default() -> Self {
        Self::new()
    }
}

impl DateTimeState {
    pub fn new() -> Self {
        let mut date = Self {
            date: [0; DATE_VALUE_LENGTH],
            now: Instant::now(),
        };
        let _ = write!(&mut date, "{}", HttpDate::from(SystemTime::now()));
        date
    }
}

impl fmt::Write for DateTimeState {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.date[..].copy_from_slice(s.as_bytes());
        Ok(())
    }
}

impl DateTime for DateTimeHandle {
    const DATE_VALUE_LENGTH: usize = DATE_VALUE_LENGTH;

    // TODO: remove this allow
    #[allow(dead_code)]
    #[inline(always)]
    fn with_date<F, O>(&self, f: F) -> O
    where
        F: FnOnce(&[u8]) -> O,
    {
        let date = self.borrow();
        f(&date.date[..])
    }

    #[inline(always)]
    fn now(&self) -> Instant {
        self.borrow().now
    }
}