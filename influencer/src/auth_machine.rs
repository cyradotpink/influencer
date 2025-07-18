use crate::message::{self as m, IntoWsMessageJson as _, WsMessageExt as _};

use std::io::{ErrorKind as IoErrorKind, Read, Write};
use thiserror::Error;
use tungstenite::{Error as WsError, Message as WsMessage, WebSocket};

#[derive(Debug)]
enum State {
    Connected,
    GotHello(Option<(String, String)>),
    SentIdentify,
    Ready(u32),
}

/// Errors that might occur while authenticating.
#[derive(Debug, Error)]
pub enum Error {
    // Boxed because clippy is unhappy about how large tungstenite's errors are
    /// An error coming from the underlying WebSocket connection.
    /// Usually fatal, apart from I/O WouldBlock errors.
    #[error("Underlying WebSocket error ({0})")]
    WebSocket(Box<WsError>),
    /// An error that occured while trying to interpret a message.
    /// These are always considered fatal because they mean that the server
    /// or client violated the protocol.
    #[error("Unexpected message ({0})")]
    Decode(#[from] m::DecodeError),
}
impl From<WsError> for Error {
    fn from(value: WsError) -> Self {
        Error::WebSocket(Box::new(value))
    }
}

/// The result of attempting to drive an [`AuthMachine`].
pub enum DriveResult<'a, Stream> {
    /// The stream is ready to be used to communicate with OBS.
    Ready { stream: Stream, rpc_version: u32 },
    /// A non-fatal error occured.
    Interrupted {
        cont: AuthMachine<'a, Stream>,
        error: Error,
    },
    /// A fatal error occurred.
    FatalError { stream: Stream, error: Error },
}
impl<'a, Stream> DriveResult<'a, Stream> {
    /// Convert the result into a [`Result`], discarding some information.
    pub fn ready(self) -> Result<(Stream, u32), Error> {
        match self {
            DriveResult::Ready {
                stream,
                rpc_version,
            } => Ok((stream, rpc_version)),
            DriveResult::Interrupted { error, .. } => Err(error),
            DriveResult::FatalError { error, .. } => Err(error),
        }
    }
}

/// A `tungstenite::WebSocket` or something that is
/// readable, writable and flushable exactly like one.
#[allow(clippy::result_large_err)]
pub trait MessageStream {
    fn read(&mut self) -> Result<WsMessage, WsError>;
    fn write(&mut self, message: WsMessage) -> Result<(), WsError>;
    fn flush(&mut self) -> Result<(), WsError>;
}
impl<Stream: Read + Write> MessageStream for WebSocket<Stream> {
    fn read(&mut self) -> Result<WsMessage, WsError> {
        self.read()
    }
    fn write(&mut self, message: WsMessage) -> Result<(), WsError> {
        self.write(message)
    }
    fn flush(&mut self) -> Result<(), WsError> {
        self.flush()
    }
}

/// An OBS authentication state machine.
#[derive(Debug)]
pub struct AuthMachine<'a, Stream> {
    password: Option<&'a str>,
    event_subscriptions: Option<u32>,
    needs_flush: bool,
    state: State,
    stream: Stream,
    error_is_nonfatal: fn(&Error) -> bool,
}
impl<'a, Stream: MessageStream> AuthMachine<'a, Stream> {
    /// Creates an [`AuthMachine`] that considers all errors
    /// to be fatal.
    pub fn new(
        stream: Stream,
        password: Option<&str>,
        event_subscriptions: Option<u32>,
    ) -> AuthMachine<'_, Stream> {
        fn f(_: &Error) -> bool {
            false
        }
        Self::new_with_custom_fatality(stream, password, event_subscriptions, f)
    }
    /// Creates an [`AuthMachine`] that considers (only!)
    /// I/O WouldBlock errors to be non-fatal.
    pub fn new_non_blocking(
        stream: Stream,
        password: Option<&str>,
        event_subscriptions: Option<u32>,
    ) -> AuthMachine<'_, Stream> {
        fn f(error: &Error) -> bool {
            match error {
                Error::WebSocket(error) => match error.as_ref() {
                    WsError::Io(error) => matches!(error.kind(), IoErrorKind::WouldBlock),
                    _ => false,
                },
                _ => false,
            }
        }
        Self::new_with_custom_fatality(stream, password, event_subscriptions, f)
    }
    /// Allows fine-grained control over which errors should be considered non-fatal.
    /// This is probably not useful.
    pub fn new_with_custom_fatality(
        stream: Stream,
        password: Option<&str>,
        event_subscriptions: Option<u32>,
        error_is_nonfatal: fn(&Error) -> bool,
    ) -> AuthMachine<'_, Stream> {
        AuthMachine {
            password,
            event_subscriptions,
            needs_flush: false,
            state: State::Connected,
            stream,
            error_is_nonfatal,
        }
    }
    pub fn get_stream_mut(&mut self) -> &mut Stream {
        &mut self.stream
    }
    pub fn get_stream(&mut self) -> &mut Stream {
        &mut self.stream
    }
    pub fn abort(self) -> Stream {
        self.stream
    }
    fn step_internal(&mut self) -> Result<(), Error> {
        if self.needs_flush {
            self.stream.flush()?;
            self.needs_flush = false;
            return Ok(());
        }
        match self.state {
            State::Connected => {
                let hello = self.stream.read()?;
                let hello = hello.obs_message_data::<m::Hello>()?;
                let auth = hello
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
                let data = m::Identify {
                    rpc_version: 1,
                    authentication: authentication.as_deref(),
                    event_subscriptions: self.event_subscriptions,
                };
                let msg = data
                    .into_ws_message_json()
                    .map_err(Into::<m::DecodeError>::into)?;
                self.stream.write(msg)?;
                self.state = State::SentIdentify;
                self.needs_flush = true;
            }
            State::SentIdentify => {
                let identified = self.stream.read()?;
                let identified = identified.obs_message_data::<m::Identified>()?;
                self.state = State::Ready(identified.negotiated_rpc_version);
            }
            State::Ready(_) => unreachable!(),
        }
        Ok(())
    }
    /// Drives the authentication process forward until it is completed or
    /// an error occurs.
    pub fn drive(mut self) -> DriveResult<'a, Stream> {
        loop {
            break match self.step_internal() {
                Ok(_) => match self.state {
                    State::Ready(rpc_version) => DriveResult::Ready {
                        stream: self.stream,
                        rpc_version,
                    },
                    _ => {
                        continue;
                    }
                },
                Err(error) => {
                    if (self.error_is_nonfatal)(&error) {
                        DriveResult::Interrupted { cont: self, error }
                    } else {
                        DriveResult::FatalError {
                            stream: self.stream,
                            error,
                        }
                    }
                }
            };
        }
    }
}
