pub mod http;
pub mod network;
pub mod packet_domain;

use atat::atat_derive::AtatUrc;
use atat::atat_derive::{AtatCmd, AtatResp};

#[derive(Clone, AtatResp)]
pub struct NoResponse;

#[derive(Clone, AtatCmd)]
#[at_cmd("", NoResponse, timeout_ms = 1000)]
pub struct AT;

#[derive(Clone, AtatUrc)]
pub enum Urc {
    #[at_urc("+Dummy")]
    DummyIndication(DummyIndication),
    // disabled as there is a URC Response conflit in atat https://github.com/FactbirdHQ/atat/issues/223
    // #[at_urc("+CREG")]
    // NetworkRegistrationStatusIndication(network::NetworkRegistrationStatus),
    #[at_urc("+HTTPACTION")]
    HttpActionResponseIndication(http::HttpActionResponse),
}

#[derive(Clone, AtatResp)]
pub struct DummyIndication;
