#[cfg(not(feature = "example_async"))]
compile_error!("This example must be compiled with `--features example_async`");

#[cfg(feature = "example_async")]
mod example {
    use async_tungstenite::{WebSocketStream, tokio::TokioAdapter};
    use futures::StreamExt;
    use influencer::{
        auth_machine::AuthMachine,
        message::{self, AnyResponse, IntoWsMessageJson, ServerMessage, WsMessageExt},
    };
    use tokio::net::TcpStream;
    use tokio::runtime;
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
        let password = std::env::args().nth(1);
        let (ws, _rpc_version) = obs_connect(password.as_deref()).await;
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
                        let event = message.obs_message_data::<message::AnyEvent>().unwrap();
                        println!("{:?}", event);
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
                            let data = message.obs_message_data::<AnyResponse>().unwrap();
                            println!("{:?}", data);
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
        password: Option<&str>,
    ) -> (WebSocketStream<TokioAdapter<TcpStream>>, u32) {
        let mut tcp_stream = TcpStream::connect("localhost:4455").await.unwrap();
        // Asynchronously perform the WebSocket handshake on the (borrowed!) TcpStream,
        // but throw away the resulting WebSocketStream.
        // Alternatively, we could drive the handshake ourselves using tungstenite::handshake::client
        async_tungstenite::client_async("ws://localhost:4455", TokioAdapter::new(&mut tcp_stream))
            .await
            .unwrap();
        // Temporarily use a "regular" WebSocket client to drive OBS authentication
        let mut auth = AuthMachine::new(
            WebSocket::from_raw_socket(TokioTcpAdapter::new(&mut tcp_stream), Role::Client, None),
            password,
            None,
        );
        let rpc_version = loop {
            let res = auth.drive();
            match res {
                Ok((_, rpc_version)) => break rpc_version,
                Err((cont, err)) => {
                    if err.would_block() {
                        auth = cont;
                        auth.get_mut().get_mut().wait().await.unwrap();
                    } else {
                        panic!("auth fail: {err}");
                    }
                }
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
