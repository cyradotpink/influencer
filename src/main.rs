use clap::{Arg, ArgMatches, Command, value_parser};
use influencer::ObsSocket;
use std::{io, net::TcpStream};

fn main() {
    fn parse_req_data(s: &str) -> serde_json::Result<serde_json::Value> {
        serde_json::from_str(s)
    }
    let command = clap::command!()
        .arg(
            Arg::new("ws-addr")
                .long("ws-addr")
                .short('a')
                .env("OBS_WS_ADDRESS")
                .default_value("localhost")
                .help("OBS websocket address."),
        )
        .arg(
            Arg::new("ws-port")
                .long("ws-port")
                .short('p')
                .env("OBS_WS_PORT")
                .default_value("4455")
                .value_parser(value_parser!(u16))
                .help("OBS websocket port."),
        )
        .arg(
            Arg::new("ws-secret")
                .long("ws-secret")
                .short('s')
                .env("OBS_WS_SECRET")
                .hide_env_values(true)
                .help("OBS websocket secret."),
        )
        .subcommand_required(true)
        .subcommand(
            Command::new("request")
                .about("Send a request and wait for a response")
                .arg(
                    Arg::new("req_type")
                        .required(true)
                        .value_name("request type")
                        .help("OBS WebSocket protocol request type name."),
                )
                .arg(
                    Arg::new("data")
                        .help("JSON data for the request.")
                        .value_parser(parse_req_data),
                ),
        );
    // todo different subcommands, like for listening to events
    let matches = command.get_matches();
    match matches.subcommand() {
        Some(("request", sub_matches)) => {
            let req_type: &String = sub_matches.get_one("req_type").unwrap();
            let data: Option<&serde_json::Value> = sub_matches.get_one("data");
            let mut obs = connect(&matches);
            let sub = obs.subscribe();
            obs.write_request_msg(req_type, "0", data).unwrap();
            obs.flush_if_needed().unwrap();
            let (status, data) = obs.get_response_msg(sub, "0").unwrap();
            println!("status {}", status);
            if let Some(data) = data {
                println!("data {}", data)
            }
        }
        _ => unreachable!(),
    }
}

fn connect(matches: &ArgMatches) -> ObsSocket<TcpStream> {
    let addr: &String = matches.get_one("ws-addr").unwrap();
    let port: &u16 = matches.get_one("ws-port").unwrap();
    let secret = matches.get_one::<String>("ws-secret").map(|v| v.as_str());
    let stream = TcpStream::connect((addr.as_str(), *port)).expect("TCP connection failed");
    let (ws, _res) = tungstenite::client::client(&format!("ws://{}:{}", addr, port), stream)
        .expect("WebSocket handshake failed");
    let mut obs = ObsSocket::new(ws);
    let sub = obs.subscribe();
    loop {
        match obs.step_auth(sub, secret) {
            Ok(true) => {
                break;
            }
            Ok(false) => {}
            Err(tungstenite::Error::Io(err)) => match err.kind() {
                io::ErrorKind::WouldBlock => panic!("Stream is in nonblocking mode"),
                _ => panic!("IO error: {}", err),
            },
            Err(err) => {
                panic!("WebSocket error: {}", err)
            }
        }
    }
    obs
}
