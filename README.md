# influencer

OBS WebSocket CLI and Rust library.

### Motivation

OBS hotkeys don't work globally on Wayland (yet). With this tool, you can register command-executing system shortcuts as a workaround.

## CLI Usage

Run `influencer help`. The CLI, thanks to clap, describes itself pretty well. Currently, you can send a request (and receive the response) or receive events. An interactive mode may be added in the future. The distinguishing quirk of this particular OBS CLI is that it is oblivious to the details of individual request/event types (See [OBS's documentation](https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md) for a list). It will accept _any_ request types and data you give it. In that sense, it's kind of a lower level interface, which you may or may not find useful.

### Examples
```sh
# Saving OBS's active replay buffer
influencer --password p4ssw0rd request SaveReplayBuffer
```

```sh
# Configuring the connection using environment variables
export OBS_WS_HOST="127.0.0.1"
export OBS_WS_PORT="6969"
export OBS_WS_PASSWORD="p4ssw0rd"
# Setting a named input's volume to -10dB
influencer request SetInputVolume \
    '{"inputName": "Desktop Audio", "inputVolumeDb": -10}'
```

```sh
# Listening for the default set of event types,
# using a compact (single-line) JSON representation
# for each event emitted to stdout
influencer --compact events
```

## Library Usage

This is a very thin library. It helps you authenticate with the OBS WebSocket server (using the state machine in the `auth` module), and provides types and basic utilites for creating (/serializing) and deserializing OBS WebSocket protocol messages. Bring your own [tungstenite](https://crates.io/crates/tungstenite)`::WebSocket`. Non-blocking sockets are supported – see the `async.rs` example.
