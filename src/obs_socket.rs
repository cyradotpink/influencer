use crate::{
    message::{self, AsRawMessage},
    subscriber_queue::SubscriberQueue,
};
use serde::Deserialize;
use std::io::{Read, Write};
use tungstenite::{Message as WsMessage, WebSocket};

#[derive(Debug)]
pub enum Readyness {
    Connected,
    GotHello,
    SentIdentify,
    Ready,
}

#[derive(Debug)]
enum AuthState {
    None,
    HelloReceived(Option<(String, String)>),
    IdentifySent,
    Identified,
}
impl AuthState {
    fn to_readyness(&self) -> Readyness {
        match self {
            AuthState::None => Readyness::Connected,
            AuthState::HelloReceived(_) => Readyness::GotHello,
            AuthState::IdentifySent => Readyness::SentIdentify,
            AuthState::Identified => Readyness::Ready,
        }
    }
}

macro_rules! make_get_message_fn {
    ($name:ident, $msg_variant:ident, $data_type:ident) => {
        #[allow(mismatched_lifetime_syntaxes)]
        pub fn $name(
            &mut self,
            cursor_id: usize,
        ) -> Result<message::$data_type, tungstenite::Error> {
            fn internal<Stream: Read + Write>(
                obs: &mut ObsSocket<Stream>,
                cursor_id: usize,
            ) -> Result<Option<message::$data_type>, tungstenite::Error> {
                let msg = obs.get_any_valid_message(cursor_id)?;
                match msg {
                    crate::message::ServerMessage::$msg_variant(data) => Ok(Some(data)),
                    _ => Ok(None),
                }
            }
            while internal(self, cursor_id)?.is_none() {
                self.ack_message(cursor_id);
            }
            internal(self, cursor_id).map(Option::unwrap)
        }
    };
}

#[derive(Debug)]
pub struct ObsSocket<Stream> {
    ws: WebSocket<Stream>,
    msgs: SubscriberQueue<WsMessage>,
    auth_state: AuthState,
    unflushed: bool,
    next_req_id: u64,
}
// all fns here do at most exactly one of reading, writing or flushing (once). this should make it easy-ish
// to use them with a nonblocking socket (Some RefCell/similar wrapping may be required).
// fns that "get" a message do not remove the relevant message from the given subscriber's queue!
// this allows them to return references to the message (or you to inspect the same message multiple times).
// you should call ack_message before you try to get another message (although you might get away without it
// if the second get_* call's filter doesn't match the message from the first one, since non-matching messages
// are skipped.)
impl<Stream: Read + Write> ObsSocket<Stream> {
    pub fn new(ws: WebSocket<Stream>) -> Self {
        let msgs = SubscriberQueue::new();
        ObsSocket {
            ws,
            msgs,
            auth_state: AuthState::None,
            unflushed: false,
            next_req_id: 0,
        }
    }
    pub fn ws_ref(&self) -> &WebSocket<Stream> {
        &self.ws
    }
    pub fn ws_mut(&mut self) -> &mut WebSocket<Stream> {
        &mut self.ws
    }
    pub fn generate_id(&mut self) -> String {
        let req_id = self.next_req_id;
        self.next_req_id += 1;
        format!("{:016x}", req_id)
    }
    pub fn subscribe(&mut self) -> usize {
        self.msgs.subscribe()
    }
    pub fn unsubscribe(&mut self, cursor_id: usize) {
        self.msgs.unsubscribe(cursor_id);
    }
    pub fn get_message_raw(&mut self, cursor_id: usize) -> Result<&WsMessage, tungstenite::Error> {
        if self.msgs.peek(cursor_id).is_none() {
            let msg = self.ws.read()?;
            self.msgs.write(msg);
        }
        Ok(self.msgs.peek(cursor_id).unwrap())
    }
    pub fn ack_message(&mut self, cursor_id: usize) -> bool {
        self.msgs.ack(cursor_id)
    }
    fn get_any_valid_message_internal<'a>(
        &'a mut self,
        cursor_id: usize,
    ) -> Result<Option<message::ServerMessage<'a>>, tungstenite::Error> {
        let msg = self.get_message_raw(cursor_id)?;
        let msg = match msg {
            WsMessage::Text(utf8_bytes) => utf8_bytes,
            _ => {
                return Ok(None);
            }
        };
        let msg = match message::ServerMessage::from_json_bytes(msg.as_bytes()) {
            Ok(msg) => msg,
            Err(_) => {
                return Ok(None);
            }
        };
        Ok(Some(msg))
    }
    pub fn get_any_valid_message<'a>(
        &'a mut self,
        cursor_id: usize,
    ) -> Result<message::ServerMessage<'a>, tungstenite::Error> {
        while self.get_any_valid_message_internal(cursor_id)?.is_none() {
            self.ack_message(cursor_id);
        }
        self.get_any_valid_message_internal(cursor_id)
            .map(Option::unwrap)
    }
    make_get_message_fn!(get_hello_msg, Hello, HelloData);
    make_get_message_fn!(get_identified_msg, Identified, IdentifiedData);
    make_get_message_fn!(get_event_msg, Event, EventDataPartialInfo);
    make_get_message_fn!(
        get_request_response_msg,
        RequestResponse,
        RequestResponseDataPartialInfo
    );
    make_get_message_fn!(
        get_request_batch_response_msg,
        RequestBatchResponse,
        RequestBatchResponseDataPartialInfo
    );
    pub fn get_request_response_for_id<'de, T: Deserialize<'de>>(
        &'de mut self,
        cursor_id: usize,
        req_id: &str,
    ) -> Result<
        (
            message::RequestResponseDataPartialInfo<'de>,
            Result<Option<T>, serde_json::Error>,
        ),
        tungstenite::Error,
    > {
        loop {
            let info = self.get_request_response_msg(cursor_id)?;
            if info.request_id == req_id {
                break;
            }
            self.ack_message(cursor_id);
        }
        let msg_bytes = match self.get_message_raw(cursor_id).unwrap() {
            WsMessage::Text(utf8_bytes) => utf8_bytes.as_bytes(),
            _ => unreachable!(),
        };
        let info = serde_json::from_slice::<
            message::RawMessagePartialD<message::RequestResponseDataPartialInfo>,
        >(msg_bytes)
        .unwrap()
        .d;
        let data = serde_json::from_slice::<
            message::RawMessagePartialD<message::RequestResponseDataPartialData<T>>,
        >(msg_bytes)
        .map(|v| v.d.response_data);
        Ok((info, data))
    }
    pub fn write_msg_plain(&mut self, msg: WsMessage) -> Result<(), tungstenite::Error> {
        self.ws.write(msg)?;
        self.unflushed = true;
        Ok(())
    }
    // panics if serializing fails. use write_msg_plain if that's an issue.
    pub fn write_msg<T>(&mut self, msg: &T) -> Result<(), tungstenite::Error>
    where
        T: AsRawMessage,
    {
        let msg = msg.as_raw_message();
        let msg = serde_json::to_string(&msg).unwrap();
        self.write_msg_plain(WsMessage::text(msg))
    }
    pub fn flush_if_needed(&mut self) -> Result<bool, tungstenite::Error> {
        if !self.unflushed {
            return Ok(false);
        }
        self.ws.flush()?;
        self.unflushed = false;
        Ok(true)
    }
    pub fn step_auth(
        &mut self,
        cursor_id: usize,
        password: Option<&str>,
    ) -> Result<Readyness, tungstenite::Error> {
        if matches!(&self.auth_state, AuthState::Identified) {
            return Ok(Readyness::Ready);
        }
        if self.flush_if_needed()? {
            return Ok(self.auth_state.to_readyness());
        }
        let password = password.unwrap_or("");
        let new_state = match &self.auth_state {
            AuthState::None => {
                let msg = self.get_hello_msg(cursor_id)?;
                let auth = msg
                    .authentication
                    .map(|v| (v.challenge.to_owned(), v.salt.to_owned()));
                AuthState::HelloReceived(auth)
            }
            AuthState::HelloReceived(auth_params) => {
                use base64ct::Encoding;
                use sha2::Digest;
                let mut authentication: Option<String> = None;
                if let Some((challenge, salt)) = auth_params {
                    let auth_string = sha2::Sha256::new()
                        .chain_update(password)
                        .chain_update(&salt)
                        .finalize();
                    let auth_string = base64ct::Base64::encode_string(&auth_string);
                    let auth_string = sha2::Sha256::new()
                        .chain_update(&auth_string)
                        .chain_update(&challenge)
                        .finalize();
                    let auth_string = base64ct::Base64::encode_string(&auth_string);
                    authentication = Some(auth_string);
                }
                let data = message::IdentifyData {
                    rpc_version: 1,
                    authentication: authentication.as_ref().map(|v| v.as_str()),
                    event_subscriptions: Some(0),
                };
                let msg = serde_json::to_string(&message::RawMessage { op: 1, d: data }).unwrap();
                self.write_msg_plain(WsMessage::text(msg))?;
                AuthState::IdentifySent
            }
            AuthState::IdentifySent => {
                self.get_identified_msg(cursor_id)?;
                AuthState::Identified
            }
            AuthState::Identified => unreachable!(),
        };
        self.auth_state = new_state;
        Ok(self.auth_state.to_readyness())
    }
}
