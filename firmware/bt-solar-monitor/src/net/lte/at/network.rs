use atat::atat_derive::{AtatCmd, AtatResp};
use atat_derive::AtatEnum;
use defmt::Format;
use heapless::String;

#[derive(Clone, AtatCmd)]
#[at_cmd("+CREG?", NetworkRegistrationStatus)]
pub struct GetNetworkRegistrationStatus;

#[derive(Clone, AtatCmd)]
#[at_cmd("+CREG", super::NoResponse)]
pub struct SetNetworkRegistrationStatus {
    #[at_arg(position = 0)]
    pub n: NetworkRegistrationUrcConfig,
}

// --- types

#[derive(Clone, AtatResp, Format)]
pub struct NetworkRegistrationStatus {
    #[at_arg(position = 0)]
    pub n: NetworkRegistrationUrcConfig,
    #[at_arg(position = 1)]
    pub stat: NetworkRegistrationStat,
    #[at_arg(position = 2)]
    pub lac: Option<String<4>>,
    #[at_arg(position = 3)]
    pub ci: Option<String<8>>,
}

#[derive(Clone, PartialEq, Eq, AtatEnum, Format)]
pub enum NetworkRegistrationUrcConfig {
    /// 0 disable network registration unsolicited result code
    UrcDisabled = 0,
    /// 1 enable network registration unsolicited result code +CREG: <stat>.
    UrcEnabled = 1,
    /// 2 enable network registration and location information unsolicitedresult code +CREG: <stat>[,<lac>,<ci>].
    UrcVerbose = 2,
}

#[derive(Debug, Clone, PartialEq, Eq, AtatEnum, Format)]
pub enum NetworkRegistrationStat {
    /// not registered, the MT is not currently searching a new operator to register to
    NotRegistered = 0,
    /// registered, home network
    Registered = 1,
    /// not registered, but the MT is currently searching a new operator to register to
    NotRegisteredSearching = 2,
    /// registration denied
    RegistrationDenied = 3,
    /// unknown
    Unknown = 4,
    /// registered, roaming
    RegisteredRoaming = 5,
    /// registered for "SMS only", home network (applicable only when E-UTRAN)
    RegisteredSmsOnly = 6,
}
