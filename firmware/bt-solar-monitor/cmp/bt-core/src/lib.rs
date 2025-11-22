#![cfg_attr(target_os = "none", no_std)]

pub(crate) mod fmt;

pub mod at;
pub mod net;
pub mod sensor;

#[cfg(test)]
pub mod tests {

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
