# influencer

OBS WebSocket CLI and Rust library.

I made this because OBS hotkeys don't work globally on Wayland (yet). With this tool, I can register command-executing system shortcuts as a workaround.

## CLI Usage

Run `influencer help`. The CLI describes itself relatively well. Currently, you can send a request (and receive the response) or receive events. See [OBS's documentation](https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md) for a lists of request/event types.

### Examples
```sh
# Saving OBS's active replay buffer
influencer --ws-password p4ssw0rd request SaveReplayBuffer
```

```sh
# Configuring the connection using environment variables
export OBS_WS_ADDRESS="127.0.0.1"
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

No documentation right now because the API isn't fully baked yet.
