use serde::{Deserialize, Serialize, de};
use std::marker::PhantomData;
use tungstenite::Message as WsMessage;

trait KBoolExt {
    fn k_ok_or_else<E, F: FnOnce() -> E>(self, f: F) -> Result<(), E>;
}
impl KBoolExt for bool {
    fn k_ok_or_else<E, F: FnOnce() -> E>(self, f: F) -> Result<(), E> {
        if self { Ok(()) } else { Err(f()) }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("Unexpected opcode ({0})")]
    OpCodeMismatch(i32),
    #[error("Not a text message")]
    NotText,
    #[error("JSON deserialize failed ({0})")]
    Json(#[from] serde_json::Error),
}

pub trait MessageData: Sized {
    const OP: i32;
}
pub trait MessageDataInfo: MessageData {
    fn from_raw_message(raw: Raw<Self>) -> Result<Self, DecodeError> {
        (raw.op == Self::OP)
            .k_ok_or_else(|| DecodeError::OpCodeMismatch(raw.op))
            .map(|_| raw.d)
    }
}
pub trait FromWsMessageJson<'a>: Sized {
    fn from_ws_message_json(msg: &'a WsMessage) -> Result<Self, DecodeError>;
}
impl<'de, T: Deserialize<'de> + MessageDataInfo> FromWsMessageJson<'de> for T {
    fn from_ws_message_json(msg: &'de WsMessage) -> Result<Self, DecodeError> {
        let raw = Raw::from_ws_message_json(msg)?;
        Self::from_raw_message(raw)
    }
}
pub trait WsMessageExt {
    fn obs_message_data<'a, T: FromWsMessageJson<'a>>(&'a self) -> Result<T, DecodeError>;
    fn any_obs_server_message<'a>(&'a self) -> Result<ServerMessage<'a>, DecodeError>;
}
impl WsMessageExt for WsMessage {
    fn obs_message_data<'a, T: FromWsMessageJson<'a>>(&'a self) -> Result<T, DecodeError> {
        T::from_ws_message_json(self)
    }
    fn any_obs_server_message<'a>(&'a self) -> Result<ServerMessage<'a>, DecodeError> {
        match self {
            WsMessage::Text(text) => Ok(ServerMessage::from_json_str(text.as_str())?),
            _ => Err(DecodeError::NotText),
        }
    }
}
pub trait MessageDataFull: MessageData {
    fn into_raw_message(self) -> Raw<Self> {
        Raw {
            op: Self::OP,
            d: self,
        }
    }
}
pub trait IntoWsMessageJson {
    fn into_ws_message_json(self) -> Result<WsMessage, serde_json::Error>;
}
impl<T: Serialize + MessageDataFull> IntoWsMessageJson for T {
    fn into_ws_message_json(self) -> Result<WsMessage, serde_json::Error> {
        self.into_raw_message().to_ws_message_json()
    }
}
impl<T: MessageDataFull> MessageDataInfo for T {}
macro_rules! impl_message_data {
    (impl<$($gen:tt),*> $type:ty, $op:literal) => {
        impl<$($gen),*> MessageData for $type {
            const OP: i32 = $op;
        }
    };
    ($type:ty, $op:literal) => {
        impl MessageData for $type {
            const OP: i32 = $op;
        }
    };
}
macro_rules! impl_message_data_full {
    (impl<$($gen:tt),*> $type:ty, $op:literal) => {
        impl_message_data!(impl<$($gen),*> $type, $op);
        impl<$($gen),*> MessageDataFull for $type {}
    };
    ($type:ty, $op:literal) => {
        impl_message_data!($type, $op);
        impl MessageDataFull for $type {}
    };
}
macro_rules! impl_message_data_info {
    (impl<$($gen:tt),*> $type:ty, $op:literal) => {
        impl_message_data!(impl<$($gen),*> $type, $op);
        impl<$($gen),*> MessageDataInfo for $type {}
    };
    ($type:ty, $op:literal) => {
        impl_message_data!($type, $op);
        impl MessageDataInfo for $type {}
    };
}

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
impl_message_data_full!(Hello<'_>, 0);

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identify<'a> {
    pub rpc_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_subscriptions: Option<u32>,
}
impl_message_data_full!(Identify<'_>, 1);

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identified {
    pub negotiated_rpc_version: u32,
}
impl_message_data_full!(Identified, 2);

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reidentify {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_subscriptions: Option<u32>,
}
impl_message_data_full!(Reidentify, 3);

pub mod event {
    use super::*;
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct InfoPart<'a> {
        pub event_type: &'a str,
        pub event_intent: u32,
    }
    impl_message_data_info!(InfoPart<'_>, 5);
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DataPart<T> {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub event_data: Option<T>,
    }
    impl_message_data!(impl<T> DataPart<T>, 5);
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
pub type AnyEvent<'a> = Event<'a, serde_json::Value>;
impl_message_data_full!(impl<T> Event<'_, T>, 5);

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request<'a, T> {
    pub request_type: &'a str,
    pub request_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_data: Option<T>,
}
impl_message_data_full!(impl<T> Request<'_, T>, 6);

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
    impl_message_data_info!(InfoPart<'_>, 7);
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DataPart<T> {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub response_data: Option<T>,
    }
    impl_message_data!(impl<T> DataPart<T>, 7);
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
impl_message_data_full!(impl<T> Response<'_, T>, 7);
pub type AnyResponse<'a> = Response<'a, serde_json::Value>;

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
impl_message_data_full!(impl<T> RequestBatch<'_, T>, 8);
pub type RequestBatchVec<'a, T> = RequestBatch<'a, Vec<request_batch::RequestsItem<'a, T>>>;

pub mod response_batch {
    use super::*;
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct InfoPart<'a> {
        pub request_id: &'a str,
    }
    impl_message_data_info!(InfoPart<'_>, 9);
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
    impl_message_data!(impl<T> ResultsPart<T>, 9);
    pub type ResultsPartVec<'a, T> = ResultsPart<Vec<ResultsItem<'a, T>>>;
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseBatch<'a, T> {
    pub request_id: &'a str,
    pub results: T,
}
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
impl_message_data_full!(impl<T> ResponseBatch<'_, T>, 9);
pub type ResponseBatchVec<'a, T> = ResponseBatch<'a, Vec<response_batch::ResultsItem<'a, T>>>;
pub type AnyResponseBatch<'a> = ResponseBatchVec<'a, serde_json::Value>;

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
impl<T: Serialize> Raw<T> {
    pub fn to_ws_message_json(&self) -> Result<WsMessage, serde_json::Error> {
        Ok(WsMessage::text(serde_json::to_string(self)?))
    }
}
impl<'de, T: Deserialize<'de>> Raw<T> {
    pub fn from_ws_message_json(ws_message: &'de WsMessage) -> Result<Self, DecodeError> {
        let text = match ws_message {
            WsMessage::Text(text) => text,
            _ => return Err(DecodeError::NotText),
        };
        Ok(serde_json::from_str(text)?)
    }
}

#[derive(Debug)]
pub enum ServerMessage<'a> {
    Hello(Hello<'a>),
    Identified(Identified),
    Event(event::InfoPart<'a>),
    Response(response::InfoPart<'a>),
    ResponseBatch(response_batch::InfoPart<'a>),
}
impl<'a> ServerMessage<'a> {
    pub fn from_json_str(json: &'a str) -> Result<ServerMessage<'a>, serde_json::Error> {
        let op_part: raw::OpPart = serde_json::from_str(json)?;
        let mut de = serde_json::Deserializer::from_str(json);
        extract_message_data_auto(&mut de, op_part.op)
    }
    pub fn opcode(&self) -> i32 {
        match self {
            ServerMessage::Hello(_) => Hello::OP,
            ServerMessage::Identified(_) => Identified::OP,
            ServerMessage::Event(_) => event::InfoPart::OP,
            ServerMessage::Response(_) => response::InfoPart::OP,
            ServerMessage::ResponseBatch(_) => response_batch::InfoPart::OP,
        }
    }
}

pub fn extract_message_data_auto<'de, D>(
    deserializer: D,
    op: i32,
) -> Result<ServerMessage<'de>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    macro_rules! match_op {
        ($variant:ident,$data_type:path) => {
            ServerMessage::$variant(
                deserializer.deserialize_map(MessageDataVisitor::<$data_type>::new())?,
            )
        };
    }
    match op {
        Hello::OP => Ok(match_op!(Hello, Hello)),
        Identified::OP => Ok(match_op!(Identified, Identified)),
        event::InfoPart::OP => Ok(match_op!(Event, event::InfoPart)),
        response::InfoPart::OP => Ok(match_op!(Response, response::InfoPart)),
        response_batch::InfoPart::OP => Ok(match_op!(ResponseBatch, response_batch::InfoPart)),
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
