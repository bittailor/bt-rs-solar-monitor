#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum NetworkRegistrationUrcConfig {
    /// 0 disable network registration unsolicited result code.
    UrcDisabled = 0,
    /// 1 enable network registration unsolicited result code +CREG: <stat>.
    UrcEnabled = 1,
    /// enable network registration and location information unsolicited result code +CREG: <stat>[,<lac>,<ci>].
    UrcVerbose = 2,
}

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
