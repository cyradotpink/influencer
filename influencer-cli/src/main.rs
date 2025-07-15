use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use influencer::{
    auth_machine,
    message::{self, FromWsMessageJson as _, IntoWsMessageJson as _},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Write, stdout},
    net::TcpStream,
    process::exit,
};
use tungstenite::WebSocket;

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
            let request_data = sub_matches.get_one::<serde_json::Value>("data");
            let request_id = "uwu";
            let request = message::Request {
                request_type,
                request_id,
                request_data,
            };
            let mut ws = connect(&matches);
            ws.send(request.into_ws_message_json().unwrap()).unwrap();
            let response = ws.read().unwrap();
            let response = message::AnyResponse::from_ws_message_json(&response).unwrap();
            assert_eq!(response.request_id, request_id);
            json_print(pretty, &response).unwrap();
        }
        Some(("batch", sub_matches)) => {
            let requests_list = sub_matches
                .get_one::<Vec<RequestsItem<serde_json::Value>>>("requests")
                .unwrap();
            let execution_type = sub_matches.get_one::<i32>("execution-type").copied();
            let halt_on_failure = sub_matches.get_flag("halt-on-failure");
            let request_id = "uwu";
            let request = message::RequestBatch {
                request_id,
                halt_on_failure: Some(halt_on_failure),
                execution_type,
                requests: requests_list,
            };
            let mut ws = connect(&matches);
            ws.send(request.into_ws_message_json().unwrap()).unwrap();
            let response = ws.read().unwrap();
            let response = message::AnyResponseBatch::from_ws_message_json(&response).unwrap();
            assert_eq!(response.request_id, request_id);
            json_print(pretty, &response).unwrap();
        }
        Some(("events", sub_matches)) => {
            let event_subscriptions = sub_matches.get_one::<u32>("event-subs").copied();
            let reidentify = message::Reidentify {
                event_subscriptions,
            };
            let mut ws = connect(&matches);
            ws.send(reidentify.into_ws_message_json().unwrap()).unwrap();
            message::Identified::from_ws_message_json(&ws.read().unwrap()).unwrap();
            loop {
                match ws.read() {
                    Ok(message) => {
                        let event = message::AnyEvent::from_ws_message_json(&message);
                        let event = match event {
                            Ok(event) => event,
                            Err(err) => {
                                println!(
                                    "Error: {err}\nWhile interpreting this message: {message}"
                                );
                                return;
                            }
                        };
                        json_print(pretty, &event).unwrap();
                    }
                    Err(err) => {
                        println!("Error: {err}");
                        return;
                    }
                }
            }
        }
        _ => unreachable!(),
    }
}

fn json_print<T: Serialize>(pretty: bool, data: &T) -> Result<(), serde_json::Error> {
    let mut out = stdout();
    json_serialize(pretty, data, &mut out)?;
    writeln!(out).unwrap();
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

fn connect(matches: &ArgMatches) -> WebSocket<TcpStream> {
    let addr: &String = matches.get_one("ws-addr").unwrap();
    let port: &u16 = matches.get_one("ws-port").unwrap();
    let password = matches.get_one::<String>("ws-password").map(|v| v.as_str());
    let stream = TcpStream::connect((addr.as_str(), *port));
    let stream = match stream {
        Ok(stream) => stream,
        Err(err) => {
            println!("TCP Connection failed: {err}");
            exit(1);
        }
    };
    let (ws, _res) = tungstenite::client::client(format!("ws://{addr}:{port}"), stream)
        .expect("WebSocket handshake failed");
    let mut auth = auth_machine::AuthMachine::new(ws, password);
    loop {
        use auth_machine::MachineResult::*;
        match auth.step() {
            NotReady(machine, Some(machine_error)) => {
                println!("Error during auth: {machine_error}\nMachine state was:\n{machine:#?}");
                exit(1);
            }
            NotReady(machine, None) => {
                auth = machine;
            }
            Ready(ws, _) => break ws,
        }
    }
}
