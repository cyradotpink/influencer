use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use influencer::{
    auth_machine,
    message::{self as m, IntoWsMessageJson as _, WsMessageExt as _},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Write, stdout},
    net::TcpStream,
};
use tungstenite::WebSocket;

fn main() -> Result<(), anyhow::Error> {
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
        .styles(style::CLAP_STYLING)
        .arg(
            Arg::new("host")
                .value_name("HOST")
                .long("host")
                .short('H')
                .env("OBS_WS_HOST")
                .default_value("localhost")
                .help("OBS websocket host"),
        )
        .arg(
            Arg::new("port")
                .value_name("PORT")
                .long("port")
                .short('p')
                .env("OBS_WS_PORT")
                .default_value("4455")
                .value_parser(value_parser!(u16))
                .help("OBS websocket port"),
        )
        .arg(
            Arg::new("password")
                .value_name("PASSWORD")
                .long("password")
                .short('s')
                .env("OBS_WS_PASSWORD")
                .hide_env_values(true)
                .help("OBS websocket password"),
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
                    .value_parser(value_parser!(u32)),
            ),
        );
    let matches = command.get_matches();
    let pretty = !matches.get_flag("compact");
    match matches.subcommand() {
        Some(("request", sub_matches)) => {
            let request_id = ":3";
            let request = m::Request {
                request_type: sub_matches.get_one::<String>("req_type").unwrap(),
                request_id,
                request_data: sub_matches.get_one::<serde_json::Value>("data"),
            };
            let mut ws = connect(&matches, Some(0))?;
            ws.send(request.into_ws_message_json()?)?;
            let response = ws.read()?;
            let response = response.obs_message_data::<m::AnyResponse>()?;
            assert_eq!(response.request_id, request_id);
            json_print(pretty, &response)?;
        }
        Some(("batch", sub_matches)) => {
            let requests_list = sub_matches
                .get_one::<Vec<RequestsItem<serde_json::Value>>>("requests")
                .unwrap();
            let execution_type = sub_matches.get_one::<i32>("execution-type").copied();
            let halt_on_failure = sub_matches.get_flag("halt-on-failure");
            let request_id = ":3";
            let request = m::RequestBatch {
                request_id,
                halt_on_failure: Some(halt_on_failure),
                execution_type,
                requests: requests_list,
            };
            let mut ws = connect(&matches, Some(0))?;
            ws.send(request.into_ws_message_json()?)?;
            let response = ws.read()?;
            let response = response.obs_message_data::<m::AnyResponseBatch>()?;
            assert_eq!(response.request_id, request_id);
            json_print(pretty, &response)?;
        }
        Some(("events", sub_matches)) => {
            let event_subscriptions = sub_matches.get_one::<u32>("event-subs").copied();
            let mut ws = connect(&matches, event_subscriptions)?;
            loop {
                let event = ws.read()?;
                let event = event.obs_message_data::<m::AnyEvent>()?;
                json_print(pretty, &event)?;
            }
        }
        _ => unreachable!(),
    }
    Ok(())
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

fn connect(
    matches: &ArgMatches,
    event_subscriptions: Option<u32>,
) -> anyhow::Result<WebSocket<TcpStream>> {
    let host: &String = matches.get_one("host").unwrap();
    let port: &u16 = matches.get_one("port").unwrap();
    let password = matches.get_one::<String>("password").map(|v| v.as_str());
    let stream = TcpStream::connect((host.as_str(), *port))?;
    let (ws, _res) = tungstenite::client::client(format!("ws://{host}:{port}"), stream)?;
    let auth = auth_machine::AuthMachine::new(ws, password, event_subscriptions);
    let (ws, _) = auth.drive().ready()?;
    Ok(ws)
}

mod style {
    // taken from https://github.com/crate-ci/clap-cargo/blob/master/src/style.rs
    use clap::builder::styling::{AnsiColor, Effects, Style};

    const HEADER: Style = AnsiColor::Green.on_default().effects(Effects::BOLD);
    const USAGE: Style = AnsiColor::Green.on_default().effects(Effects::BOLD);
    const LITERAL: Style = AnsiColor::Cyan.on_default().effects(Effects::BOLD);
    const PLACEHOLDER: Style = AnsiColor::Cyan.on_default();
    const ERROR: Style = AnsiColor::Red.on_default().effects(Effects::BOLD);
    const VALID: Style = AnsiColor::Cyan.on_default().effects(Effects::BOLD);
    const INVALID: Style = AnsiColor::Yellow.on_default().effects(Effects::BOLD);

    pub const CLAP_STYLING: clap::builder::styling::Styles =
        clap::builder::styling::Styles::styled()
            .header(HEADER)
            .usage(USAGE)
            .literal(LITERAL)
            .placeholder(PLACEHOLDER)
            .error(ERROR)
            .valid(VALID)
            .invalid(INVALID);
}
