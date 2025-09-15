use nom::{IResult, Parser, bytes::complete::tag};

use crate::lte::AtError;

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum NetworkRegistrationUrcConfig {
    /// 0 disable network registration unsolicited result code.
    UrcDisabled = 0,
    /// 1 enable network registration unsolicited result code +CREG: <stat>.
    UrcEnabled = 1,
    /// enable network registration and location information unsolicited result code +CREG: <stat>[,<lac>,<ci>].
    UrcVerbose = 2,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NetworkRegistrationState {
    /// 0 not registered, ME is not currently searching a new operator to register to.
    NotRegistered = 0,
    /// 1 registered, home network.
    Registered = 1,
    /// 2 not registered, but ME is currently searching a new operator to register to.
    NotRegisteredSearching = 2,
    /// 3 registration denied.
    RegistrationDenied = 3,
    /// 4 unknown.
    Unknown = 4,
    /// 5 registered, roaming.
    RegisteredRoaming = 5,
    /// 6 registered for "SMS only", home network (applicable only whenE-UTRAN)
    RegisteredSmsOnly = 6,
}

fn sperator(input: &str) -> IResult<&str, ()> {
    let (input, _) = tag(",")(input)?;
    Ok((input, ()))
}

fn number(input: &str) -> IResult<&str, u32> {
    nom::character::complete::u32(input)
}

fn nr_value(input: &str) -> IResult<&str, (NetworkRegistrationUrcConfig, NetworkRegistrationState)> {
    let (input, (n, _, stat)) = (number, sperator, number).parse(input)?;
    let n = match n {
        0 => NetworkRegistrationUrcConfig::UrcDisabled,
        1 => NetworkRegistrationUrcConfig::UrcEnabled,
        2 => NetworkRegistrationUrcConfig::UrcVerbose,
        _ => return Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Fail))),
    };
    let stat = match stat {
        0 => NetworkRegistrationState::NotRegistered,
        1 => NetworkRegistrationState::Registered,
        2 => NetworkRegistrationState::NotRegisteredSearching,
        3 => NetworkRegistrationState::RegistrationDenied,
        4 => NetworkRegistrationState::Unknown,
        5 => NetworkRegistrationState::RegisteredRoaming,
        6 => NetworkRegistrationState::RegisteredSmsOnly,
        _ => return Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Fail))),
    };
    Ok((input, (n, stat)))
}

// +CREG: <n>,<stat>[,<lac>,<ci>]
// +CREG: 0,1
pub fn parse_network_registration_response(input: &str) -> Result<(NetworkRegistrationUrcConfig, NetworkRegistrationState), AtError> {
    let (_, (_, (n, stat))) = (tag("+CREG: "), nr_value).parse(input)?;
    Ok((n, stat))
}

pub struct GetNetworkRegistrationStatus {}

impl GetNetworkRegistrationStatus {
    pub fn execute() -> Result<(NetworkRegistrationUrcConfig, NetworkRegistrationState), AtError> {
        todo!()
    }
}

impl From<nom::Err<nom::error::Error<&str>>> for AtError {
    fn from(_err: nom::Err<nom::error::Error<&str>>) -> Self {
        AtError::Error
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_add() {
        let (n, stat) = parse_network_registration_response("+CREG: 0,1").unwrap();
        assert_eq!(n, NetworkRegistrationUrcConfig::UrcDisabled);
        assert_eq!(stat, NetworkRegistrationState::Registered);
    }
}
