#[cfg(not(feature = "example_async"))]
compile_error!("This example must be compiled with `--features example_async`");

// Demonstrates usage of the library in non-blocking contexts
// Takes password, port, host as first, second, third command line argument
// Defaults to no password, 4455, localhost
// Makes a GetVersion request and waits to receive 10 events

#[cfg(feature = "example_async")]
mod example {
    use async_tungstenite::{WebSocketStream, tokio::TokioAdapter};
    use futures::StreamExt as _;
    use influencer::{
        auth::AuthMachine,
        message::{self, AnyResponse, IntoWsMessageJson as _, ServerMessage, WsMessageExt as _},
    };
    use tokio::{net::TcpStream, runtime};
    use tungstenite::{Message, WebSocket, protocol::Role};

    #[derive(Debug)]
    struct TokioTcpAdapter<'a> {
        pub inner: &'a mut TcpStream,
        pub wait_read: bool,
        pub wait_write: bool,
    }
    impl<'a> TokioTcpAdapter<'a> {
        fn new(inner: &'a mut TcpStream) -> Self {
            Self {
                inner,
                wait_read: false,
                wait_write: false,
            }
        }
        pub async fn wait(&mut self) -> std::io::Result<()> {
            if self.wait_read {
                self.inner.readable().await?;
                self.wait_read = false;
            }
            if self.wait_write {
                self.inner.writable().await?;
                self.wait_write = false;
            }
            Ok(())
        }
    }
    impl<'a> std::io::Read for TokioTcpAdapter<'a> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let res = self.inner.try_read(buf);
            if let Err(ref err) = res {
                if let std::io::ErrorKind::WouldBlock = err.kind() {
                    self.wait_read = true;
                }
            };
            res
        }
    }
    impl<'a> std::io::Write for TokioTcpAdapter<'a> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let res = self.inner.try_write(buf);
            if let Err(ref err) = res {
                if let std::io::ErrorKind::WouldBlock = err.kind() {
                    self.wait_write = true;
                }
            };
            res
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    pub fn main() {
        let rt = runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async_main());
    }

    async fn async_main() {
        let mut args = std::env::args().skip(1);
        let (ws, rpc_version) = obs_connect(args.next(), args.next(), args.next()).await;
        println!("Connected! Server selected RPC version {}", rpc_version);
        let (ws_sender, mut ws_receiver) = ws.split();
        let (tx, ws_rx1) = tokio::sync::broadcast::channel::<Message>(8);
        let ws_rx2 = tx.subscribe();
        tokio::task::spawn(async move {
            loop {
                let message = ws_receiver.next().await.unwrap().unwrap();
                if tx.send(message).is_err() {
                    break;
                }
            }
        });
        let (ws_tx1, mut rx) = tokio::sync::mpsc::channel::<Message>(8);
        tokio::task::spawn(async move {
            loop {
                match rx.recv().await {
                    Some(message) => ws_sender.send(message).await.unwrap(),
                    None => break,
                }
            }
        });
        let event_listener_task = tokio::task::spawn(async move {
            let mut rx = ws_rx1;
            let mut n = 0;
            while n < 10 {
                let message = rx.recv().await.unwrap();
                match message.any_obs_server_message() {
                    Ok(ServerMessage::Event(_)) => {
                        n += 1;
                        let event = serde_json::to_string_pretty(
                            &message.obs_message_data::<message::AnyEvent>().unwrap(),
                        )
                        .unwrap();
                        println!("{event}");
                    }
                    _ => {}
                }
            }
            println!("Got 10 events!");
        });
        let get_info_task = tokio::task::spawn(async move {
            let mut rx = ws_rx2;
            ws_tx1
                .send(
                    message::Request::<()> {
                        request_type: "GetVersion",
                        request_id: ":3",
                        request_data: None,
                    }
                    .into_ws_message_json()
                    .unwrap(),
                )
                .await
                .unwrap();
            loop {
                let message = rx.recv().await.unwrap();
                match message.any_obs_server_message() {
                    Ok(ServerMessage::Response(info)) => {
                        if info.request_id == ":3" {
                            let data = serde_json::to_string_pretty(
                                &message.obs_message_data::<AnyResponse>().unwrap(),
                            )
                            .unwrap();
                            println!("{data}");
                            break;
                        }
                    }
                    _ => {}
                }
            }
        });
        event_listener_task.await.unwrap();
        get_info_task.await.unwrap();
    }

    async fn obs_connect(
        password: Option<String>,
        port: Option<String>,
        host: Option<String>,
    ) -> (WebSocketStream<TokioAdapter<TcpStream>>, u32) {
        let port = port.unwrap_or_else(|| "4455".to_string());
        let host = host.unwrap_or_else(|| "localhost".to_string());
        let mut tcp_stream = TcpStream::connect(&format!("{host}:{port}")).await.unwrap();
        // Asynchronously perform the WebSocket handshake on the (borrowed!) TcpStream,
        // but throw away the resulting WebSocketStream.
        // Alternatively, we could drive the handshake ourselves using the
        // tungstenite::handshake::client module
        async_tungstenite::client_async(
            &format!("ws://{host}:{port}"),
            TokioAdapter::new(&mut tcp_stream),
        )
        .await
        .unwrap();
        // Temporarily use a "regular" WebSocket client to drive OBS authentication
        let mut auth = AuthMachine::new_non_blocking(
            WebSocket::from_raw_socket(TokioTcpAdapter::new(&mut tcp_stream), Role::Client, None),
            password.as_deref(),
            None,
        );
        let rpc_version = loop {
            let res = auth.drive();
            use influencer::auth::DriveResult;
            match res {
                DriveResult::FatalError { error, .. } => panic!("{error}"),
                DriveResult::Interrupted { cont, .. } => {
                    auth = cont;
                    auth.get_stream_mut().get_mut().wait().await.unwrap();
                }
                DriveResult::Ready { rpc_version, .. } => break rpc_version,
            }
        };
        // Finally, with handshake and authentication completed,
        // transfer ownership of the TcpStream to a new WebSocketStream.
        let ws =
            WebSocketStream::from_raw_socket(TokioAdapter::new(tcp_stream), Role::Client, None)
                .await;
        (ws, rpc_version)
    }
}

#[cfg(feature = "example_async")]
use example::main;

#[cfg(not(feature = "example_async"))]
fn main() {
    unreachable!()
}
