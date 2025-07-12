pub mod idk {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    pub struct InputVolumeChanged<'a> {
        pub input_name: &'a str,
    }
}

pub mod message;
mod obs_socket;
mod subscriber_queue;

pub use obs_socket::{ObsSocket, Readyness};
