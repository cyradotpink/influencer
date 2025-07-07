# influencer

OBS WebSocket CLI and Rust library.

I made this because OBS hotkeys don't work globally on Wayland right now. This way I can register command-executing shortcuts in my system instead.

## CLI Usage

Run `influencer help`. You'll figure out the rest. Only sending requests (and receiving the response) is supported right now. See [OBS's documentation](https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md) for a list of request types and shapes. If you're a Nushell user, the wrapper command in `obs-request.nu` might be useful to you. The tool handles connection errors by panicking. Sorry.

## Library Usage

There's no documentation right now. Sorry again. The level of abstraction (and amount of effort) is relatively low, there's no fancy code generation or anything like that. The goal was to create a minimal synchronous API that's general enough to build further abstractions - including non-blocking and async ones - on top of. A major issue to be aware of is that if you register multiple subscribers, but one subscriber stops consuming messages while others continue, all messages that at least one subscriber hasn't acknowledged stay buffered forever. So be careful or fix my code I guess.
