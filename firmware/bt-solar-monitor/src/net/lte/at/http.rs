use atat_derive::{AtatCmd, AtatEnum, AtatResp};
use defmt::Format;
use heapless::String;

#[derive(Clone, AtatCmd)]
#[at_cmd("+HTTPINIT", super::NoResponse)]
pub struct StartHttpService;

#[derive(Clone, Format)]
pub struct SetHttpParameter<'a> {
    pub parameter: HttpParameter<'a>,
}

#[derive(Clone, AtatCmd)]
#[at_cmd("+HTTPACTION", super::NoResponse)] // URC 
pub struct HttpAction {
    #[at_arg(position = 0)]
    pub method: HttpMethod,
}

#[derive(Clone, AtatCmd)]
#[at_cmd("+HTTPREAD?", QueryHttpReadResponse, parse=parse_query_http_read_response)] // URC
pub struct QueryHttpRead {}

#[derive(Clone, AtatResp)]
pub struct QueryHttpReadResponse {
    pub data_length: u32,
}

fn parse_query_http_read_response(line: &[u8]) -> Result<QueryHttpReadResponse, atat::Error> {
    defmt::debug!("parse_query_http_read_response {=[u8]:a}", line);
    let parts = line.split(|b| *b == b',');
    match parts.last() {
        Some(length) => {
            let length = str::from_utf8(length)
                .map_err(|_| atat::Error::Parse)?
                .parse::<u32>()
                .map_err(|_| atat::Error::Parse)?;
            Ok(QueryHttpReadResponse {
                data_length: length,
            })
        }
        None => Err(atat::Error::Parse),
    }
}

#[derive(Clone, AtatCmd)]
#[at_cmd("+HTTPREAD", super::NoResponse)] // URC 
pub struct HttpRead {
    #[at_arg(position = 0)]
    pub offset: u32,
    #[at_arg(position = 1)]
    pub lenght: u32,
}

#[derive(Clone, AtatResp, Format)]
pub struct HttpActionResponse {
    #[at_arg(position = 0)]
    pub method: HttpMethod,
    #[at_arg(position = 1)]
    pub status_code: u16,
    #[at_arg(position = 2)]
    pub data_length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Format)]
pub enum HttpParameter<'a> {
    Url(&'a str),
    ContentType(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq, AtatEnum, Format)]
pub enum HttpMethod {
    Get = 0,
    Post = 1,
    Head = 2,
    Delete = 3,
}

// manual impls

impl<'a> atat::AtatCmd for SetHttpParameter<'a> {
    type Response = super::NoResponse;
    const MAX_LEN: usize = 265;

    fn write(&self, buf: &mut [u8]) -> usize {
        match atat::serde_at::to_slice(
            self,
            "+HTTPPARA",
            buf,
            atat::serde_at::SerializeOptions {
                value_sep: true,
                cmd_prefix: "AT",
                termination: "\r",
                quote_escape_strings: true,
            },
        ) {
            Ok(s) => s,
            Err(_) => panic!("Failed to serialize command"),
        }
    }

    fn parse(
        &self,
        resp: Result<&[u8], atat::InternalError>,
    ) -> Result<Self::Response, atat::Error> {
        match resp {
            Ok(resp) => atat::serde_at::from_slice::<super::NoResponse>(resp)
                .map_err(|_| atat::Error::Parse),
            Err(e) => Err(e.into()),
        }
    }
}

impl<'a> atat::serde_at::serde::Serialize for SetHttpParameter<'a> {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: atat::serde_at::serde::Serializer,
    {
        let mut serde_state = atat::serde_at::serde::Serializer::serialize_struct(
            serializer,
            "SetHttpParameter",
            3usize,
        )?;
        match &self.parameter {
            HttpParameter::Url(url) => {
                atat::serde_at::serde::ser::SerializeStruct::serialize_field(
                    &mut serde_state,
                    "type",
                    "URL",
                )?;
                atat::serde_at::serde::ser::SerializeStruct::serialize_field(
                    &mut serde_state,
                    "url",
                    *url,
                )?;
            }
            HttpParameter::ContentType(content_type) => {
                atat::serde_at::serde::ser::SerializeStruct::serialize_field(
                    &mut serde_state,
                    "type",
                    "CONTENT",
                )?;
                atat::serde_at::serde::ser::SerializeStruct::serialize_field(
                    &mut serde_state,
                    "url",
                    *content_type,
                )?;
            }
        }
        atat::serde_at::serde::ser::SerializeStruct::end(serde_state)
    }
}
