use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use influencer::{ObsSocket, message};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write, stdout},
    net::TcpStream,
    thread::sleep,
    time::Duration,
};

fn main() {
    fn parse_req_data(s: &str) -> serde_json::Result<serde_json::Value> {
        serde_json::from_str(s)
    }
    #[derive(Debug, Clone, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RequestsItem<T> {
        pub request_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub request_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub request_data: Option<T>,
    }
    fn parse_batch_data(s: &str) -> serde_json::Result<Vec<RequestsItem<serde_json::Value>>> {
        serde_json::from_str(s)
    }
    let command = clap::command!()
        .arg(
            Arg::new("ws-addr")
                .value_name("ADDRESS")
                .long("ws-addr")
                .short('a')
                .env("OBS_WS_ADDRESS")
                .default_value("localhost")
                .help("OBS websocket address."),
        )
        .arg(
            Arg::new("ws-port")
                .value_name("PORT")
                .long("ws-port")
                .short('p')
                .env("OBS_WS_PORT")
                .default_value("4455")
                .value_parser(value_parser!(u16))
                .help("OBS websocket port."),
        )
        .arg(
            Arg::new("ws-password")
                .value_name("PASSWORD")
                .long("ws-password")
                .short('s')
                .env("OBS_WS_PASSWORD")
                .hide_env_values(true)
                .help("OBS websocket password."),
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
                        .value_name("NAME")
                        .required(true)
                        .help("OBS WebSocket protocol request type name."),
                )
                .arg(
                    Arg::new("data")
                        .value_name("DATA")
                        .help("JSON data for the request.")
                        .value_parser(parse_req_data),
                ),
        )
        .subcommand(
            Command::new("batch")
                .about("Send a batch of requests and wait for a response")
                .arg(
                    Arg::new("halt-on-failure")
                        .long("halt-on-failure")
                        .action(ArgAction::SetTrue)
                        .help("Stop processing requests after the first failure"),
                )
                .arg(
                    Arg::new("execution-type")
                        .long("execution-type")
                        .value_name("TYPE")
                        .allow_negative_numbers(true)
                        .value_parser(value_parser!(i32))
                        .help("Stop processing requests after the first failure"),
                )
                .arg(
                    Arg::new("requests")
                        .value_name("DATA")
                        .help("JSON array of requests")
                        .required(true)
                        .value_parser(parse_batch_data),
                ),
        )
        .subcommand(
            Command::new("events").about("Listen for events").arg(
                Arg::new("event-subs")
                    .value_name("BITMASK")
                    .help("Event types bitmask")
                    .default_value("1023")
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
            obs.write_msg(&message::Request {
                request_type,
                request_id: &request_id,
                request_data,
            })
            .unwrap();
            obs.flush().unwrap();
            obs.next_response_for_request_id(sub, &request_id).unwrap();
            let (info, data) = obs.get_buffered_response::<serde_json::Value>(sub).unwrap();
            let data = data.unwrap();
            let response = message::Response::from_info_w_data(info, data);
            json_print(pretty, &response).unwrap();
            obs.ack_message(sub);
        }
        Some(("batch", sub_matches)) => {
            let requests_list = sub_matches
                .get_one::<Vec<RequestsItem<serde_json::Value>>>("requests")
                .unwrap();
            let execution_type = sub_matches.get_one::<i32>("execution-type").copied();
            let halt_on_failure = sub_matches.get_flag("halt-on-failure");
            let mut obs = connect(&matches);
            let sub = obs.subscribe();
            let request_id = obs.generate_id();
            obs.write_msg(&message::RequestBatch {
                request_id: &request_id,
                halt_on_failure: Some(halt_on_failure),
                execution_type: execution_type,
                requests: requests_list,
            })
            .unwrap();
            obs.flush().unwrap();
            obs.next_response_for_batch_request_id(sub, &request_id)
                .unwrap();
            let info = obs.get_buffered_response_batch_message(sub).unwrap();
            let data: message::raw::DPart<
                message::response_batch::ResultsPartVec<serde_json::Value>,
            > = serde_json::from_str(obs.get_buffered_text_message(sub).unwrap()).unwrap();
            let response = message::ResponseBatch::from_parts(info, data.d);
            json_print(pretty, &response).unwrap();
            obs.ack_message(sub);
        }
        Some(("events", sub_matches)) => {
            let event_subscriptions = sub_matches.get_one::<u32>("event-subs").copied();
            let mut obs = connect(&matches);
            let sub = obs.subscribe();
            obs.write_msg(&message::Reidentify {
                event_subscriptions,
            })
            .unwrap();
            obs.flush().unwrap();
            while let Ok(_) = obs.next_event_message(sub) {
                let info = obs.get_buffered_event_message(sub).unwrap();
                let data = serde_json::from_str::<
                    message::raw::DPart<message::event::DataPart<serde_json::Value>>,
                >(obs.get_buffered_text_message(sub).unwrap())
                .unwrap();
                let event = message::Event::from_parts(info, data.d);
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
    let password = matches.get_one::<String>("ws-password").map(|v| v.as_str());
    let stream = TcpStream::connect((addr.as_str(), *port)).expect("TCP connection failed");
    let (ws, _res) = tungstenite::client::client(&format!("ws://{}:{}", addr, port), stream)
        .expect("WebSocket handshake failed");
    let mut obs = ObsSocket::new(ws);
    let sub = obs.subscribe();
    loop {
        match obs.step_auth(sub, password) {
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
