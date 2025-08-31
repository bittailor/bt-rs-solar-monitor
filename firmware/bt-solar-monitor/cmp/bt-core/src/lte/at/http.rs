use atat_derive::{AtatCmd, AtatEnum, AtatResp};

#[derive(Clone, AtatCmd)]
#[at_cmd("+HTTPINIT", super::NoResponse)]
pub struct StartHttpService;

#[derive(Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

#[derive(Clone, AtatCmd)]
#[at_cmd("+HTTPREAD", super::NoResponse)] // URC 
pub struct HttpRead {
    #[at_arg(position = 0)]
    pub offset: u32,
    #[at_arg(position = 1)]
    pub lenght: u32,
}

#[derive(Clone, AtatResp)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct HttpActionResponse {
    #[at_arg(position = 0)]
    pub method: HttpMethod,
    #[at_arg(position = 1)]
    pub status_code: u16,
    #[at_arg(position = 2)]
    pub data_length: u32,
}

#[derive(Clone, AtatResp)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct HttpReadData {
    #[at_arg(position = 0)]
    pub data: heapless::Vec<u8, 512>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum HttpParameter<'a> {
    Url(&'a str),
    ContentType(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq, AtatEnum)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

fn parse_query_http_read_response(line: &[u8]) -> Result<QueryHttpReadResponse, atat::Error> {
    debug!("parse_query_http_read_response {=[u8]:a}", line);
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

#[cfg(test)]
mod tests {
    use super::*;
    use atat::AtatCmd;

    const CMD_BUFFER_SIZE: usize = 1024;

    fn write_to_string<Cmd: AtatCmd>(cmd: &Cmd) -> String {
        let mut buf = [0u8; CMD_BUFFER_SIZE];
        let len = cmd.write(&mut buf);
        core::str::from_utf8(&buf[..len]).unwrap().into()
    }

    fn get_resp(line: &str) -> Result<&[u8], atat::InternalError> {
        Ok(line.as_bytes())
    }

    #[test]
    fn cmd_set_http_parameter_url() {
        let cmd = SetHttpParameter {
            parameter: HttpParameter::Url("http://api.solar.bockmattli.ch/api/v1/solar"),
        };
        assert_eq!(
            write_to_string(&cmd),
            "AT+HTTPPARA=\"URL\",\"http://api.solar.bockmattli.ch/api/v1/solar\"\r"
        );
    }

    #[test]
    fn cmd_set_http_parameter_content_type() {
        let cmd = SetHttpParameter {
            parameter: HttpParameter::ContentType("text/plain"),
        };
        assert_eq!(
            write_to_string(&cmd),
            "AT+HTTPPARA=\"CONTENT\",\"text/plain\"\r"
        );
    }

    #[test]
    fn resp_query_http_read() {
        let cmd = QueryHttpRead {};
        let response = cmd
            .parse(get_resp("+HTTPREAD: LEN,93"))
            .expect("Failed to parse response");
        assert_eq!(response.data_length, 93);
    }
}
