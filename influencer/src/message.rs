use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, de, ser::SerializeMap};

pub mod hello {
    use super::*;
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Authentication<'a> {
        pub challenge: &'a str,
        pub salt: &'a str,
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello<'a> {
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<hello::Authentication<'a>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identify<'a> {
    pub rpc_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_subscriptions: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identified {
    pub negotiated_rpc_version: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reidentify {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_subscriptions: Option<u32>,
}

pub mod event {
    use super::*;
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct InfoPart<'a> {
        pub event_type: &'a str,
        pub event_intent: u32,
    }
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DataPart<T> {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub event_data: Option<T>,
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event<'a, T> {
    pub event_type: &'a str,
    pub event_intent: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_data: Option<T>,
}
impl<'a, T> Event<'a, T> {
    pub fn from_parts(info: event::InfoPart<'a>, data: event::DataPart<T>) -> Self {
        Self {
            event_type: info.event_type,
            event_intent: info.event_intent,
            event_data: data.event_data,
        }
    }
    pub fn from_info_w_data(info: event::InfoPart<'a>, data: Option<T>) -> Self {
        Self::from_parts(info, event::DataPart { event_data: data })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request<'a, T> {
    pub request_type: &'a str,
    pub request_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_data: Option<T>,
}

pub mod response {
    use super::*;
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RequestStatus<'a> {
        pub result: bool,
        pub code: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub comment: Option<&'a str>,
    }
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct InfoPart<'a> {
        pub request_type: &'a str,
        pub request_id: &'a str,
        pub request_status: RequestStatus<'a>,
    }
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DataPart<T> {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub response_data: Option<T>,
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response<'a, T> {
    pub request_type: &'a str,
    pub request_id: &'a str,
    pub request_status: response::RequestStatus<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_data: Option<T>,
}
impl<'a, T> Response<'a, T> {
    pub fn from_parts(info: response::InfoPart<'a>, data: response::DataPart<T>) -> Self {
        Self {
            request_type: info.request_type,
            request_id: info.request_id,
            request_status: info.request_status,
            response_data: data.response_data,
        }
    }
    pub fn from_info_w_data(info: response::InfoPart<'a>, data: Option<T>) -> Self {
        Self::from_parts(
            info,
            response::DataPart {
                response_data: data,
            },
        )
    }
}

pub mod request_batch {
    use super::*;
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RequestsItem<'a, T> {
        pub request_type: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub request_id: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub request_data: Option<T>,
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBatch<'a, T> {
    pub request_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub halt_on_failure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_type: Option<i32>,
    pub requests: T,
}
pub type RequestBatchVec<'a, T> = RequestBatch<'a, Vec<request_batch::RequestsItem<'a, T>>>;

pub mod response_batch {
    use super::*;
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct InfoPart<'a> {
        pub request_id: &'a str,
    }
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ResultsItem<'a, T> {
        pub request_type: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub request_id: Option<&'a str>,
        #[serde(borrow)]
        pub request_status: response::RequestStatus<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub response_data: Option<T>,
    }
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ResultsPart<T> {
        pub results: T,
    }
    pub type RequestBatchResponseDataPartialResultsVec<'a, T> =
        ResultsPart<Vec<ResultsItem<'a, T>>>;
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseBatch<'a, T> {
    pub request_id: &'a str,
    pub results: T,
}
pub type RequestBatchResponseVec<'a, T> =
    ResponseBatch<'a, Vec<response_batch::ResultsItem<'a, T>>>;
impl<'a, T> ResponseBatch<'a, T> {
    pub fn from_parts(
        info: response_batch::InfoPart<'a>,
        results: response_batch::ResultsPart<T>,
    ) -> Self {
        Self {
            request_id: info.request_id,
            results: results.results,
        }
    }
}

pub mod raw {
    use super::*;
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OpPart {
        pub op: i32,
    }
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DPart<T> {
        pub d: T,
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Raw<T> {
    pub op: i32,
    pub d: T,
}

#[derive(Debug)]
pub enum ServerMessage<'a> {
    Hello(Hello<'a>),
    Identified(Identified),
    Event(event::InfoPart<'a>),
    RequestResponse(response::InfoPart<'a>),
    RequestBatchResponse(response_batch::InfoPart<'a>),
}
impl<'a> ServerMessage<'a> {
    pub fn from_json_str(json: &'a str) -> Result<ServerMessage<'a>, serde_json::Error> {
        let op_part: raw::OpPart = serde_json::from_str(json)?;
        let mut de = serde_json::Deserializer::from_str(json);
        extract_message_data_auto(&mut de, op_part.op)
    }
    pub fn opcode(&self) -> i32 {
        match self {
            ServerMessage::Hello(_) => 0,
            ServerMessage::Identified(_) => 2,
            ServerMessage::Event(_) => 5,
            ServerMessage::RequestResponse(_) => 7,
            ServerMessage::RequestBatchResponse(_) => 9,
        }
    }
}

pub fn serialize_message<T: Serialize, S>(
    op: i32,
    data: &T,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut ser_map = serializer.serialize_map(Some(2))?;
    ser_map.serialize_entry("op", &op)?;
    ser_map.serialize_key("d")?;
    ser_map.serialize_value(data)?;
    ser_map.end()
}

pub fn extract_message_data_auto<'de, D>(
    deserializer: D,
    op: i32,
) -> Result<ServerMessage<'de>, D::Error>
where
    D: Deserializer<'de>,
{
    macro_rules! match_op {
        ($variant:ident,$data_type:path) => {
            ServerMessage::$variant(
                deserializer.deserialize_map(MessageDataVisitor::<$data_type>::new())?,
            )
        };
    }
    match op {
        0 => Ok(match_op!(Hello, Hello)),
        2 => Ok(match_op!(Identified, Identified)),
        5 => Ok(match_op!(Event, event::InfoPart)),
        7 => Ok(match_op!(RequestResponse, response::InfoPart)),
        9 => Ok(match_op!(RequestBatchResponse, response_batch::InfoPart)),
        invalid => Err(de::Error::invalid_value(
            de::Unexpected::Signed(invalid.into()),
            &"valid OBS Server->Client message OpCode",
        )),
    }
}

struct MessageDataVisitor<Data> {
    _p: PhantomData<Data>,
}

impl<Data> MessageDataVisitor<Data> {
    fn new() -> Self {
        Self { _p: PhantomData }
    }
}
impl<'de, Data: Deserialize<'de>> de::Visitor<'de> for MessageDataVisitor<Data> {
    type Value = Data;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a map")
    }
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        let mut d: Option<Data> = None;
        while let Some(k) = map.next_key::<&str>()? {
            match k {
                "d" => {
                    let _ = d.insert(map.next_value()?);
                }
                _ => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            };
        }
        match d {
            Some(d) => Ok(d),
            None => Err(de::Error::missing_field("d")),
        }
    }
}

pub trait AsRawMessage {
    type Target: Serialize;
    fn as_raw_message(&self) -> Raw<&Self::Target>;
}
macro_rules! make_as_raw_message_fn {
    ($op:literal) => {
        type Target = Self;
        fn as_raw_message(&self) -> Raw<&Self> {
            Raw { op: $op, d: self }
        }
    };
}
impl<T: Serialize> AsRawMessage for Raw<T> {
    type Target = T;
    fn as_raw_message(&self) -> Raw<&T> {
        Raw {
            op: self.op,
            d: &self.d,
        }
    }
}
impl<'a> AsRawMessage for Hello<'a> {
    make_as_raw_message_fn!(0);
}
impl AsRawMessage for Reidentify {
    make_as_raw_message_fn!(3);
}
impl<'a, T: Serialize> AsRawMessage for Request<'a, T> {
    make_as_raw_message_fn!(6);
}
impl<'a, T: Serialize> AsRawMessage for RequestBatch<'a, T> {
    make_as_raw_message_fn!(8);
}
