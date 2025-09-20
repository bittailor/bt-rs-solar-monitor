use crate::at::AtError;
pub mod sim_com_a67;

#[derive(Debug, Eq, PartialEq)]
pub enum CellularError {
    Timeout,
    AtError(AtError),
    GpioError,
}

#[cfg(feature = "defmt")]
impl defmt::Format for CellularError {
    fn format(&self, f: defmt::Formatter) {
        match self {
            CellularError::Timeout => defmt::write!(f, "Timeout"),
            CellularError::AtError(e) => defmt::write!(f, "AtError({:?})", e),
            CellularError::GpioError => defmt::write!(f, "GpioError"),
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
