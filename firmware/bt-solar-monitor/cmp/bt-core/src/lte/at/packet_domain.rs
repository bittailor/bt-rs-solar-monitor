use atat::atat_derive::AtatCmd;
use atat_derive::AtatLen;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, AtatLen)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ContextId(pub u8);

#[derive(Clone, AtatCmd)]
#[at_cmd("+CGDCONT", super::NoResponse)]
pub struct SetPDPContextDefinition<'a> {
    #[at_arg(position = 0)]
    pub cid: ContextId,
    #[at_arg(position = 1, len = 6)]
    pub pdp_type: &'a str,
    #[at_arg(position = 2, len = 99)]
    pub apn: &'a str,
}
