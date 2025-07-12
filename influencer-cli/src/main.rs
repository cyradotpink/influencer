use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use influencer::{ObsSocket, message};
use serde::Serialize;
use std::{
    io::{self, Write, stdout},
    net::TcpStream,
};

fn main() {
    fn parse_req_data(s: &str) -> serde_json::Result<serde_json::Value> {
        serde_json::from_str(s)
    }
    let command = clap::command!()
        .arg(
            Arg::new("ws-addr")
                .value_name("WS address")
                .long("ws-addr")
                .short('a')
                .env("OBS_WS_ADDRESS")
                .default_value("localhost")
                .help("OBS websocket address."),
        )
        .arg(
            Arg::new("ws-port")
                .value_name("WS port")
                .long("ws-port")
                .short('p')
                .env("OBS_WS_PORT")
                .default_value("4455")
                .value_parser(value_parser!(u16))
                .help("OBS websocket port."),
        )
        .arg(
            Arg::new("ws-secret")
                .value_name("WS secret")
                .long("ws-secret")
                .short('s')
                .env("OBS_WS_SECRET")
                .hide_env_values(true)
                .help("OBS websocket secret."),
        )
        .arg(
            Arg::new("compact")
                .long("compact")
                .short('c')
                .action(ArgAction::SetTrue)
                .help("Compact JSON output"),
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
        )
        .subcommand(
            Command::new("events").about("Listen for events").arg(
                Arg::new("event_subs")
                    .value_name("event subscriptions")
                    .help("Event types bitmask")
                    .value_parser(value_parser!(u32)),
            ),
        );
    // todo different subcommands, like for listening to events
    let matches = command.get_matches();
    let pretty = !matches.get_flag("compact");
    match matches.subcommand() {
        Some(("request", sub_matches)) => {
            let request_type = sub_matches.get_one::<String>("req_type").unwrap();
            let request_data: Option<&serde_json::Value> =
                sub_matches.get_one::<serde_json::Value>("data");
            let mut obs = connect(&matches);
            let sub = obs.subscribe();
            let request_id = obs.generate_id();
            obs.write_msg(&message::RequestData {
                request_type,
                request_id: &request_id,
                request_data,
            })
            .unwrap();
            obs.flush_if_needed().unwrap();
            obs.next_response_for_request_id(sub, &request_id).unwrap();
            let (info, data) = obs
                .get_buffered_request_response::<serde_json::Value>(sub)
                .unwrap();
            let data = data.unwrap();
            let response = message::RequestResponseData::from_info_w_data(info, data);
            json_print(pretty, &response).unwrap();
            obs.ack_message(sub);
        }
        Some(("events", sub_matches)) => {
            let event_subscriptions = sub_matches.get_one::<u32>("event_subs").copied();
            let mut obs = connect(&matches);
            let sub = obs.subscribe();
            obs.write_msg(&message::ReidentifyData {
                event_subscriptions,
            })
            .unwrap();
            obs.flush_if_needed().unwrap();
            while let Ok(_) = obs.next_event_message(sub) {
                let info = obs.get_buffered_event_message(sub).unwrap();
                let data = serde_json::from_str::<
                    message::RawMessagePartialD<message::EventDataPartialData<serde_json::Value>>,
                >(obs.get_buffered_text_message(sub).unwrap())
                .unwrap();
                let event = message::EventData::from_parts(info, data.d);
                json_print(pretty, &event).unwrap();
                obs.ack_message(sub);
            }
        }
        _ => unreachable!(),
    }
}

fn json_print<T: Serialize>(pretty: bool, data: &T) -> Result<(), serde_json::Error> {
    let mut out = stdout();
    json_serialize(pretty, data, &mut out)?;
    out.write("\n".as_bytes()).unwrap();
    Ok(())
}

fn json_serialize<T: Serialize, W: Write>(
    pretty: bool,
    data: &T,
    writer: W,
) -> Result<(), serde_json::Error> {
    if pretty {
        data.serialize(&mut serde_json::Serializer::pretty(writer))
    } else {
        data.serialize(&mut serde_json::Serializer::new(writer))
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
            Ok(influencer::Readyness::Ready) => {
                break;
            }
            Ok(_) => {}
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
