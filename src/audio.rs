use anyhow::Result;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::fs::File;
use std::io::BufReader;

pub struct AudioEngine {
    _sink_handle: MixerDeviceSink,
    player: Player,
    paused: bool,
    volume: f32,
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let sink_handle = DeviceSinkBuilder::open_default_sink()
            .map_err(|_| anyhow::anyhow!("Failed to open default audio stream"))?;
        let player = Player::connect_new(&sink_handle.mixer());
        player.set_volume(0.5);

        Ok(Self {
            _sink_handle: sink_handle,
            player,
            paused: false,
            volume: 0.5,
        })
    }

    pub fn play(&mut self, path: &str) {
        if let Ok(file) = File::open(path) {
            if let Ok(decoder) = Decoder::try_from(BufReader::new(file)) {
                self.player.clear();
                self.player.append(decoder);
                self.player.set_volume(self.volume);
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

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.player.set_volume(self.volume);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_empty(&self) -> bool {
        self.player.empty()
    }
}
