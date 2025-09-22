use embedded_io_async::{Read, Write};
use heapless::format;

use crate::{
    at::{AtClient, AtError},
    at_request,
};
use nom::{Parser, bytes::complete::tag};

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

impl TryFrom<u32> for NetworkRegistrationUrcConfig {
    type Error = AtError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(NetworkRegistrationUrcConfig::UrcDisabled),
            1 => Ok(NetworkRegistrationUrcConfig::UrcEnabled),
            2 => Ok(NetworkRegistrationUrcConfig::UrcVerbose),
            _ => Err(AtError::EnumParseError(format!("Invalid NetworkRegistrationUrcConfig value: {}", value).unwrap_or_default())),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

impl TryFrom<u32> for NetworkRegistrationState {
    type Error = AtError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(NetworkRegistrationState::NotRegistered),
            1 => Ok(NetworkRegistrationState::Registered),
            2 => Ok(NetworkRegistrationState::NotRegisteredSearching),
            3 => Ok(NetworkRegistrationState::RegistrationDenied),
            4 => Ok(NetworkRegistrationState::Unknown),
            5 => Ok(NetworkRegistrationState::RegisteredRoaming),
            6 => Ok(NetworkRegistrationState::RegisteredSmsOnly),
            11 => Ok(NetworkRegistrationState::NotRegisteredSearching), // ???
            _ => Err(AtError::EnumParseError(format!("Invalid NetworkRegistrationState value: {}", value).unwrap_or_default())),
        }
    }
}

// +CREG: <n>,<stat>[,<lac>,<ci>]
// +CREG: 0,1
pub async fn get_network_registration<'ch, Stream: Read + Write + 'ch>(
    ctr: &impl AtClient<'ch, Stream>,
) -> Result<(NetworkRegistrationUrcConfig, NetworkRegistrationState), AtError> {
    let response = at_request!("AT+CREG?").send(ctr).await?;
    let (_, (_, n, _, stat)) = (tag("+CREG: "), nom::character::complete::u32, tag(","), nom::character::complete::u32).parse(response.line(0)?)?;
    Ok((n.try_into()?, stat.try_into()?))
}

#[cfg(test)]
pub mod mocks {
    /*
    use super::*;
    use crate::at::mocks::mock_request;

        #[tokio::test]
        async fn test_network_registration() -> Result<(), AtError> {
            let mock = mock_request("AT+CREG?", &["+CREG: 0,0"]);
            let (n, stat) = get_network_registration(&mock).await?;
            assert_eq!(n, NetworkRegistrationUrcConfig::UrcDisabled);
            assert_eq!(stat, NetworkRegistrationState::NotRegistered);

            let mock = mock_request("AT+CREG?", &["+CREG: 0,1"]);
            let (n, stat) = get_network_registration(&mock).await?;
            assert_eq!(n, NetworkRegistrationUrcConfig::UrcDisabled);
            assert_eq!(stat, NetworkRegistrationState::Registered);

            let mock = mock_request("AT+CREG?", &["+CREG: 0,11"]);
            let (n, stat) = get_network_registration(&mock).await?;
            assert_eq!(n, NetworkRegistrationUrcConfig::UrcDisabled);
            assert_eq!(stat, NetworkRegistrationState::NotRegisteredSearching);

            Ok(())
        }
    */
}
