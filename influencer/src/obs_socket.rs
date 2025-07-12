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
    ($name:ident, $buffered_name:ident, $msg_variant:ident, $data_type:path) => {
        #[allow(mismatched_lifetime_syntaxes)]
        pub fn $buffered_name(&self, cursor_id: usize) -> Option<$data_type> {
            let msg = self.get_buffered_valid_message(cursor_id)?;
            match msg {
                crate::message::ServerMessage::$msg_variant(data) => Some(data),
                _ => None,
            }
        }
        #[allow(mismatched_lifetime_syntaxes)]
        pub fn $name(&mut self, cursor_id: usize) -> Result<(), tungstenite::Error> {
            while self.$buffered_name(cursor_id).is_none() {
                self.ack_get_message_raw(cursor_id)?;
            }
            Ok(())
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
    pub fn get_buffered_message_raw(&self, cursor_id: usize) -> Option<&WsMessage> {
        self.msgs.peek(cursor_id)
    }
    pub fn get_buffered_text_message(&self, cursor_id: usize) -> Option<&str> {
        let msg = self.get_buffered_message_raw(cursor_id)?;
        match msg {
            WsMessage::Text(utf8_bytes) => Some(utf8_bytes.as_str()),
            _ => None,
        }
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
    pub fn ack_get_message_raw(
        &mut self,
        cursor_id: usize,
    ) -> Result<&WsMessage, tungstenite::Error> {
        self.ack_message(cursor_id);
        self.get_message_raw(cursor_id)
    }
    pub fn get_text_message(&mut self, cursor_id: usize) -> Result<&str, tungstenite::Error> {
        while self.get_buffered_text_message(cursor_id).is_none() {
            self.ack_get_message_raw(cursor_id)?;
        }
        Ok(self.get_buffered_text_message(cursor_id).unwrap())
    }
    fn get_buffered_valid_message<'a>(
        &'a self,
        cursor_id: usize,
    ) -> Option<message::ServerMessage<'a>> {
        let msg = self.get_buffered_text_message(cursor_id)?;
        match message::ServerMessage::from_json_str(msg) {
            Ok(msg) => Some(msg),
            Err(_) => None,
        }
    }
    pub fn next_valid_message<'a>(
        &'a mut self,
        cursor_id: usize,
    ) -> Result<(), tungstenite::Error> {
        while self.get_buffered_valid_message(cursor_id).is_none() {
            self.ack_get_message_raw(cursor_id)?;
        }
        Ok(())
    }
    make_get_message_fn!(
        next_hello_msg,
        get_buffered_hello_msg,
        Hello,
        message::Hello
    );
    make_get_message_fn!(
        next_identified_message,
        get_buffered_identified_message,
        Identified,
        message::Identified
    );
    make_get_message_fn!(
        next_event_message,
        get_buffered_event_message,
        Event,
        message::event::InfoPart
    );
    make_get_message_fn!(
        next_response_message,
        get_buffered_response_message,
        RequestResponse,
        message::response::InfoPart
    );
    make_get_message_fn!(
        next_response_batch_message,
        get_buffered_response_batch_message,
        RequestBatchResponse,
        message::response_batch::InfoPart
    );
    pub fn get_buffered_response<'a, T: Deserialize<'a>>(
        &'a self,
        cursor_id: usize,
    ) -> Option<(
        message::response::InfoPart<'a>,
        Result<Option<T>, serde_json::Error>,
    )> {
        let info = self.get_buffered_response_message(cursor_id)?;
        let data = serde_json::from_str::<message::raw::DPart<message::response::DataPart<T>>>(
            self.get_buffered_text_message(cursor_id).unwrap(),
        )
        .map(|v| v.d.response_data);
        Some((info, data))
    }
    pub fn next_response_for_request_id(
        &mut self,
        cursor_id: usize,
        req_id: &str,
    ) -> Result<(), tungstenite::Error> {
        while match self.get_buffered_response_message(cursor_id) {
            Some(msg) => msg.request_id != req_id,
            None => true,
        } {
            self.ack_get_message_raw(cursor_id)?;
        }
        Ok(())
    }
    pub fn next_response_for_batch_request_id(
        &mut self,
        cursor_id: usize,
        req_id: &str,
    ) -> Result<(), tungstenite::Error> {
        while match self.get_buffered_response_batch_message(cursor_id) {
            Some(msg) => msg.request_id != req_id,
            None => true,
        } {
            self.ack_get_message_raw(cursor_id)?;
        }
        Ok(())
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
    pub fn flush(&mut self) -> Result<bool, tungstenite::Error> {
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
        if self.flush()? {
            return Ok(self.auth_state.to_readyness());
        }
        let password = password.unwrap_or("");
        let new_state = match &self.auth_state {
            AuthState::None => {
                self.next_hello_msg(cursor_id)?;
                let msg = self.get_buffered_hello_msg(cursor_id).unwrap();
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
                let data = message::Identify {
                    rpc_version: 1,
                    authentication: authentication.as_ref().map(|v| v.as_str()),
                    event_subscriptions: Some(0),
                };
                let msg = serde_json::to_string(&message::Raw { op: 1, d: data }).unwrap();
                self.write_msg_plain(WsMessage::text(msg))?;
                AuthState::IdentifySent
            }
            AuthState::IdentifySent => {
                self.next_identified_message(cursor_id)?;
                AuthState::Identified
            }
            AuthState::Identified => unreachable!(),
        };
        self.auth_state = new_state;
        Ok(self.auth_state.to_readyness())
    }
}
