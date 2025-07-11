use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, de, ser::SerializeMap};

pub trait AsRawMessage {
    type Target: Serialize;
    fn as_raw_message(&self) -> RawMessage<&Self::Target>;
}
macro_rules! make_as_raw_message_fn {
    ($op:literal) => {
        type Target = Self;
        fn as_raw_message(&self) -> RawMessage<&Self> {
            RawMessage { op: $op, d: self }
        }
    };
}
impl<T: Serialize> AsRawMessage for RawMessage<T> {
    type Target = T;
    fn as_raw_message(&self) -> RawMessage<&T> {
        RawMessage {
            op: self.op,
            d: &self.d,
        }
    }
}
impl<'a> AsRawMessage for HelloData<'a> {
    make_as_raw_message_fn!(0);
}
impl AsRawMessage for ReidentifyData {
    make_as_raw_message_fn!(3);
}
impl<'a, T: Serialize> AsRawMessage for RequestData<'a, T> {
    make_as_raw_message_fn!(6);
}
impl<'a, T: Serialize> AsRawMessage for RequestBatchData<'a, T> {
    make_as_raw_message_fn!(8);
}

#[derive(Debug, Deserialize, Serialize)]
struct PhantomDeserialize;
impl<'a, 'de> Deserialize<'de> for &'a PhantomDeserialize {
    fn deserialize<D>(_: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(&PhantomDeserialize)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloDataAuthentication<'a> {
    pub challenge: &'a str,
    pub salt: &'a str,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloData<'a> {
    #[serde(borrow)]
    pub authentication: Option<HelloDataAuthentication<'a>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifyData<'a> {
    pub rpc_version: u32,
    pub authentication: Option<&'a str>,
    pub event_subscriptions: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifiedData {
    pub negotiated_rpc_version: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReidentifyData {
    pub event_subscriptions: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDataPartialInfo<'a> {
    pub event_type: &'a str,
    pub event_intent: u32,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDataPartialData<T> {
    pub event_data: T,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventData<'a, T> {
    pub event_type: &'a str,
    pub event_intent: u32,
    pub event_data: T,
}
impl<'a, T> EventData<'a, T> {
    pub fn from_parts(info: EventDataPartialInfo<'a>, data: EventDataPartialData<T>) -> Self {
        Self {
            event_type: info.event_type,
            event_intent: info.event_intent,
            event_data: data.event_data,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestData<'a, T> {
    pub request_type: &'a str,
    pub request_id: &'a str,
    pub request_data: Option<T>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResponseDataPartialInfoStatus<'a> {
    pub result: bool,
    pub code: i32,
    pub comment: Option<&'a str>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResponseDataPartialInfo<'a> {
    pub request_type: &'a str,
    pub request_id: &'a str,
    pub request_status: RequestResponseDataPartialInfoStatus<'a>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResponseDataPartialData<T> {
    pub response_data: Option<T>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResponseData<'a, T> {
    pub request_type: &'a str,
    pub request_id: &'a str,
    pub request_status: RequestResponseDataPartialInfoStatus<'a>,
    pub response_data: Option<T>,
}
impl<'a, T> RequestResponseData<'a, T> {
    pub fn from_parts(
        info: RequestResponseDataPartialInfo<'a>,
        data: RequestResponseDataPartialData<T>,
    ) -> Self {
        Self {
            request_type: info.request_type,
            request_id: info.request_id,
            request_status: info.request_status,
            response_data: data.response_data,
        }
    }
    pub fn from_info_w_data(info: RequestResponseDataPartialInfo<'a>, data: T) -> Self {
        Self::from_parts(
            info,
            RequestResponseDataPartialData {
                response_data: Some(data),
            },
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBatchDataRequestsInner<'a, T> {
    pub request_type: &'a str,
    pub request_id: Option<&'a str>,
    pub request_data: Option<T>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBatchData<'a, T> {
    pub request_id: &'a str,
    pub halt_on_failure: Option<bool>,
    pub execution_type: Option<i32>,
    pub requests: Vec<RequestBatchDataRequestsInner<'a, T>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBatchResponseDataPartialInfo<'a> {
    pub request_id: &'a str,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBatchResponseDataPartialResultsInner<'a, T> {
    pub request_type: &'a str,
    pub request_id: Option<&'a str>,
    #[serde(borrow)]
    pub request_status: RequestResponseDataPartialInfoStatus<'a>,
    pub response_data: Option<T>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBatchResponseDataPartialResults<'a, T> {
    #[serde(borrow)]
    pub results: Vec<RequestBatchResponseDataPartialResultsInner<'a, T>>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBatchResponseData<'a, T> {
    pub request_id: &'a str,
    pub results: Vec<RequestBatchResponseDataPartialResultsInner<'a, T>>,
}
impl<'a, T> RequestBatchResponseData<'a, T> {
    pub fn from_parts(
        info: RequestBatchResponseDataPartialInfo<'a>,
        results: RequestBatchResponseDataPartialResults<'a, T>,
    ) -> Self {
        Self {
            request_id: info.request_id,
            results: results.results,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMessagePartialOp {
    pub op: i32,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMessagePartialD<T> {
    pub d: T,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMessage<T> {
    pub op: i32,
    pub d: T,
}

#[derive(Debug)]
pub enum ServerMessage<'a> {
    Hello(HelloData<'a>),
    Identified(IdentifiedData),
    Event(EventDataPartialInfo<'a>),
    RequestResponse(RequestResponseDataPartialInfo<'a>),
    RequestBatchResponse(RequestBatchResponseDataPartialInfo<'a>),
}
impl<'a> ServerMessage<'a> {
    pub fn from_json_bytes(json: &'a [u8]) -> Result<ServerMessage<'a>, serde_json::Error> {
        let op_part: RawMessagePartialOp = serde_json::from_slice(json)?;
        let mut de = serde_json::Deserializer::from_slice(json);
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
        0 => Ok(match_op!(Hello, HelloData)),
        2 => Ok(match_op!(Identified, IdentifiedData)),
        5 => Ok(match_op!(Event, EventDataPartialInfo)),
        7 => Ok(match_op!(RequestResponse, RequestResponseDataPartialInfo)),
        9 => Ok(match_op!(
            RequestBatchResponse,
            RequestBatchResponseDataPartialInfo
        )),
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
