use crate::subscriber_queue::SubscriberQueue;
use serde_json::Value;
use std::io::{Read, Write};
use tungstenite::{Message, WebSocket};

#[derive(Debug)]
pub enum ObsAuthState {
    None,
    HelloReceived(Option<(String, String)>),
    IdentifySent,
    Identified,
}

#[derive(Debug)]
pub struct ObsSocket<Stream> {
    ws: WebSocket<Stream>,
    msgs: SubscriberQueue<Message>,
    auth_state: ObsAuthState,
    unflushed: bool,
}
// all fns here do at most exactly one of reading, writing or flushing (once)
// this should make it easy-ish to use them with a nonblocking socket
impl<Stream: Read + Write> ObsSocket<Stream> {
    pub fn new(ws: WebSocket<Stream>) -> Self {
        let msgs = SubscriberQueue::new();
        ObsSocket {
            ws,
            msgs,
            auth_state: ObsAuthState::None,
            unflushed: false,
        }
    }
    pub fn ws_ref(&self) -> &WebSocket<Stream> {
        &self.ws
    }
    pub fn ws_mut(&mut self) -> &mut WebSocket<Stream> {
        &mut self.ws
    }
    pub fn subscribe(&mut self) -> usize {
        self.msgs.subscribe()
    }
    pub fn unsubscribe(&mut self, cursor_id: usize) {
        self.msgs.unsubscribe(cursor_id);
    }
    pub fn get_message_raw(&mut self, cursor_id: usize) -> Result<&Message, tungstenite::Error> {
        if self.msgs.peek(cursor_id).is_none() {
            let msg = self.ws.read()?;
            self.msgs.write(msg);
        }
        Ok(self.msgs.peek(cursor_id).unwrap())
    }
    pub fn ack_message(&mut self, cursor_id: usize) -> bool {
        self.msgs.ack(cursor_id)
    }
    // completely ignores messages that aren't JSON or don't have "op" and "d" values.
    // this lets me avoid having to create my own error type.
    pub fn get_message(&mut self, cursor_id: usize) -> Result<(i64, Value), tungstenite::Error> {
        loop {
            let msg = self.get_message_raw(cursor_id)?;
            let msg = match msg {
                Message::Text(utf8_bytes) => utf8_bytes,
                _ => {
                    self.ack_message(cursor_id);
                    continue;
                }
            };
            let mut msg: Value = match serde_json::from_slice(msg.as_bytes()) {
                Ok(msg) => msg,
                Err(_) => {
                    self.ack_message(cursor_id);
                    continue;
                }
            };
            // can't ack before here bc we were still depending on a ref to msgs
            self.ack_message(cursor_id);
            let (msg_op, msg) = match (|| -> Option<_> {
                Some((msg.get("op")?.as_i64()?, msg.get_mut("d")?.take()))
            })() {
                Some(v) => v,
                None => {
                    continue;
                }
            };
            break Ok((msg_op, msg));
        }
    }
    pub fn get_op_message(
        &mut self,
        cursor_id: usize,
        op: i64,
    ) -> Result<Value, tungstenite::Error> {
        loop {
            let (msg_op, msg) = self.get_message(cursor_id)?;
            if msg_op == op {
                break Ok(msg);
            }
        }
    }
    pub fn get_response_msg(
        &mut self,
        cursor_id: usize,
        req_id: &str,
    ) -> Result<(Value, Option<Value>), tungstenite::Error> {
        loop {
            let mut msg = self.get_op_message(cursor_id, 7)?;
            let msg_req_id = match (|| -> Option<_> { Some(msg.get("requestId")?.as_str()?) })() {
                Some(id) => id,
                None => continue,
            };
            if msg_req_id != req_id {
                continue;
            }
            // should probably attempt to parse at least the status into an actual data structure
            // also could let the caller choose how to parse the data
            let status = msg
                .get_mut("requestStatus")
                .map(Value::take)
                .unwrap_or_else(|| Value::Null);
            let data = msg.get_mut("responseData").map(Value::take);
            break Ok((status, data));
        }
    }
    pub fn write_msg(&mut self, msg: Message) -> Result<(), tungstenite::Error> {
        self.ws.write(msg)?;
        self.unflushed = true;
        Ok(())
    }
    pub fn flush_if_needed(&mut self) -> Result<bool, tungstenite::Error> {
        if !self.unflushed {
            return Ok(false);
        }
        self.ws.flush()?;
        self.unflushed = false;
        Ok(true)
    }
    pub fn write_request_msg(
        &mut self,
        req_type: &str,
        req_id: &str,
        req_data: Option<&Value>, // would be better to let the caller supply anything serializable
    ) -> Result<(), tungstenite::Error> {
        let msg = serde_json::json!({
            "op": 6,
            "d": {
                "requestType": req_type,
                "requestId": req_id,
                "requestData": req_data
            }
        });
        self.write_msg(Message::text(msg.to_string()))
    }
    // should probably expose more info than just whether or not authentication is completed
    pub fn step_auth(
        &mut self,
        cursor_id: usize,
        password: Option<&str>,
    ) -> Result<bool, tungstenite::Error> {
        if matches!(&self.auth_state, ObsAuthState::Identified) {
            return Ok(true);
        }
        if self.flush_if_needed()? {
            return Ok(false);
        }
        let password = password.unwrap_or("");
        match &self.auth_state {
            ObsAuthState::None => {
                let msg = self.get_op_message(cursor_id, 0)?;
                let auth_params = (|| -> Option<_> {
                    let auth = msg.get("authentication")?;
                    Some((
                        auth.get("challenge")?.as_str()?.to_owned(),
                        auth.get("salt")?.as_str()?.to_owned(),
                    ))
                })();
                self.auth_state = ObsAuthState::HelloReceived(auth_params);
                Ok(false)
            }
            ObsAuthState::HelloReceived(auth_params) => {
                let mut identify_msg = serde_json::json!({
                    "op": 1,
                    "d": { "rpcVersion": 1, "eventSubscriptions": 0 }
                });
                use base64ct::Encoding;
                use sha2::Digest;
                if let Some((challenge, salt)) = auth_params {
                    let auth_string = sha2::Sha256::new()
                        .chain_update(password)
                        .chain_update(salt)
                        .finalize();
                    let auth_string = base64ct::Base64::encode_string(&auth_string);
                    let auth_string = sha2::Sha256::new()
                        .chain_update(&auth_string)
                        .chain_update(challenge)
                        .finalize();
                    let auth_string = base64ct::Base64::encode_string(&auth_string);
                    identify_msg
                        .get_mut("d")
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .insert("authentication".to_string(), Value::String(auth_string));
                }
                self.write_msg(Message::text(serde_json::to_string(&identify_msg).unwrap()))?;
                self.auth_state = ObsAuthState::IdentifySent;
                Ok(false)
            }
            ObsAuthState::IdentifySent => {
                self.get_op_message(cursor_id, 2)?;
                self.auth_state = ObsAuthState::Identified;
                Ok(true)
            }
            ObsAuthState::Identified => unreachable!(),
        }
    }
}
