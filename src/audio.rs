use anyhow::Result;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::fs::File;
use std::io::BufReader;

pub struct AudioEngine {
    _sink_handle: MixerDeviceSink,
    player: Player,
    paused: bool,
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let sink_handle = DeviceSinkBuilder::open_default_sink()
            .map_err(|_| anyhow::anyhow!("Failed to open default audio stream"))?;
        let player = Player::connect_new(&sink_handle.mixer());

        Ok(Self {
            _sink_handle: sink_handle,
            player,
            paused: false,
        })
    }

    pub fn play(&mut self, path: &str) {
        if let Ok(file) = File::open(path) {
            if let Ok(decoder) = Decoder::try_from(BufReader::new(file)) {
                self.player.clear();
                self.player.append(decoder);
                self.player.play();
                self.paused = false;
            }
        }
    }

    pub fn toggle(&mut self) {
        if self.player.empty() {
            return;
        }
        if self.paused {
            self.player.play();
        } else {
            self.player.pause();
        }
        self.paused = !self.paused;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }
}
