#![cfg_attr(target_os = "none", no_std)]

use embassy_sync::{
    blocking_mutex::raw::RawMutex,
    mutex::{Mutex, MutexGuard},
};

pub(crate) mod fmt;

pub mod at;
pub mod net;
pub mod sensor;
pub mod time;

struct LoggingMutexGuard<'a, M, T>
where
    M: RawMutex,
    T: ?Sized,
{
    guard: Option<MutexGuard<'a, M, T>>,
    tag: &'static str,
}

impl<'a, M: RawMutex, T: ?Sized> LoggingMutexGuard<'a, M, T> {
    pub async fn new(mutex: &'a Mutex<M, T>, tag: &'static str) -> Self {
        trace!("Mutex[{}] acquire ..", tag);
        let guard = mutex.lock().await;
        trace!("Mutex[{}] .. acquired", tag);
        Self { guard: Some(guard), tag }
    }
}

impl<'a, M: RawMutex, T: ?Sized> core::ops::Deref for LoggingMutexGuard<'a, M, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap()
    }
}

impl<'a, M: RawMutex, T: ?Sized> core::ops::DerefMut for LoggingMutexGuard<'a, M, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().unwrap()
    }
}

impl<'a, M: RawMutex, T: ?Sized> Drop for LoggingMutexGuard<'a, M, T> {
    fn drop(&mut self) {
        trace!("Mutex[{}] releasing ..", self.tag);
        drop(self.guard.take().unwrap());
        trace!("Mutex[{}] .. released", self.tag);
    }
}

#[cfg(test)]
pub mod tests {

    #[cfg(feature = "log")]
    #[cfg_attr(feature = "log", ctor::ctor)]
    fn init() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_thread_names(true)
            .with_level(true)
            .pretty()
            .init();
    }
}
