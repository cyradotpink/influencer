use crate::message::{self, MessageDataFull as _, MessageDataInfo as _};

use std::io::{Read, Write};
use thiserror::Error;
use tungstenite::WebSocket;

#[derive(Debug)]
enum State {
    Connected,
    GotHello(Option<(String, String)>),
    SentIdentify,
    Ready(u32),
}

#[derive(Debug, Error)]
pub enum MachineError {
    #[error("underlying WebSocket error ({0})")]
    // boxed bc clippy complained
    WebSocket(Box<tungstenite::Error>),
    #[error("unexpected message ({0})")]
    Decode(#[from] message::DecodeError),
}
impl From<tungstenite::Error> for MachineError {
    fn from(value: tungstenite::Error) -> Self {
        MachineError::WebSocket(Box::new(value))
    }
}

#[derive(Debug)]
pub enum MachineResult<'a, Stream> {
    NotReady(AuthMachine<'a, Stream>, Option<MachineError>),
    Ready(WebSocket<Stream>, u32),
}

#[derive(Debug)]
pub struct AuthMachine<'a, Stream> {
    password: Option<&'a str>,
    needs_flush: bool,
    state: State,
    ws: WebSocket<Stream>,
}
impl<'a, Stream: Read + Write> AuthMachine<'a, Stream> {
    pub fn new(ws: WebSocket<Stream>, password: Option<&str>) -> AuthMachine<'_, Stream> {
        AuthMachine {
            password,
            needs_flush: false,
            state: State::Connected,
            ws,
        }
    }
    pub fn abort(self) -> WebSocket<Stream> {
        self.ws
    }
    fn step_internal(&mut self) -> Result<(), MachineError> {
        if self.needs_flush {
            self.ws.flush()?;
            self.needs_flush = false;
            return Ok(());
        }
        match self.state {
            State::Connected => {
                let msg = self.ws.read()?;
                let msg = message::Raw::from_ws_message_json(&msg)?;
                let msg = message::Hello::from_raw_message(msg)?;
                let auth = msg
                    .authentication
                    .map(|v| (v.challenge.to_owned(), v.salt.to_owned()));
                self.state = State::GotHello(auth);
            }
            State::GotHello(ref auth_params) => {
                use base64ct::Encoding;
                use sha2::Digest;
                let mut authentication: Option<String> = None;
                if let Some((challenge, salt)) = auth_params {
                    let auth_string = sha2::Sha256::new()
                        .chain_update(self.password.unwrap_or(""))
                        .chain_update(salt)
                        .finalize();
                    let auth_string = base64ct::Base64::encode_string(&auth_string);
                    let auth_string = sha2::Sha256::new()
                        .chain_update(auth_string)
                        .chain_update(challenge)
                        .finalize();
                    let auth_string = base64ct::Base64::encode_string(&auth_string);
                    authentication = Some(auth_string);
                }
                let data = message::Identify {
                    rpc_version: 1,
                    authentication: authentication.as_deref(),
                    event_subscriptions: Some(0),
                };
                let msg = data
                    .into_raw_message()
                    .to_ws_message_json()
                    .map_err(Into::<message::DecodeError>::into)?;
                self.ws.write(msg)?;
                self.state = State::SentIdentify;
                self.needs_flush = true;
            }
            State::SentIdentify => {
                let msg = self.ws.read()?;
                let msg = message::Raw::from_ws_message_json(&msg)?;
                let msg = message::Identified::from_raw_message(msg)?;
                self.state = State::Ready(msg.negotiated_rpc_version);
            }
            State::Ready(_) => unreachable!(),
        }
        Ok(())
    }
    pub fn step(mut self) -> MachineResult<'a, Stream> {
        let res = self.step_internal();
        if let State::Ready(rpc_version) = self.state {
            MachineResult::Ready(self.ws, rpc_version)
        } else {
            MachineResult::NotReady(self, res.err())
        }
    }
}
