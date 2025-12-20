#![allow(async_fn_in_trait)]

use crate::at::AtError;
pub mod sim_com_a67;

#[derive(Debug, Eq, PartialEq)]
pub enum CellularError {
    Timeout,
    AtError(AtError),
    GpioError,
    Encoding(),
}

#[cfg(feature = "defmt")]
impl defmt::Format for CellularError {
    fn format(&self, f: defmt::Formatter) {
        match self {
            CellularError::Timeout => defmt::write!(f, "Timeout"),
            CellularError::AtError(e) => defmt::write!(f, "AtError({:?})", e),
            CellularError::GpioError => defmt::write!(f, "GpioError"),
            CellularError::Encoding() => defmt::write!(f, "Encoding Error"),
        }
    }
}

impl From<AtError> for CellularError {
    fn from(err: AtError) -> Self {
        CellularError::AtError(err)
    }
}

impl From<embassy_time::TimeoutError> for CellularError {
    fn from(_err: embassy_time::TimeoutError) -> Self {
        CellularError::Timeout
    }
}

impl embedded_io_async::Error for CellularError {
    fn kind(&self) -> embedded_io_async::ErrorKind {
        match self {
            CellularError::Timeout => embedded_io_async::ErrorKind::TimedOut,
            CellularError::AtError(_) => embedded_io_async::ErrorKind::Other,
            CellularError::GpioError => embedded_io_async::ErrorKind::Other,
            CellularError::Encoding() => embedded_io_async::ErrorKind::Other,
        }
    }
}
