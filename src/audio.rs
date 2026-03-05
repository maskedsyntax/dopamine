use anyhow::Result;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source, Sample, ChannelCount, SampleRate};
use std::fs::File;
use std::io::BufReader;
use std::time::Duration;
use std::sync::{Arc, Mutex};

pub struct VisualizerSource<I>
where
    I: Source,
{
    inner: I,
    samples: Arc<Mutex<Vec<f32>>>,
    batch_buffer: Vec<f32>,
}

impl<I> VisualizerSource<I>
where
    I: Source,
{
    pub fn new(inner: I, samples: Arc<Mutex<Vec<f32>>>) -> Self {
        Self { 
            inner, 
            samples,
            batch_buffer: Vec::with_capacity(512),
        }
    }
}

impl<I> Iterator for VisualizerSource<I>
where
    I: Source,
{
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next();
        if let Some(s) = sample {
            self.batch_buffer.push(s);
            if self.batch_buffer.len() >= 512 {
                // NEVER block the audio thread. If we can't get the lock, skip it.
                if let Ok(mut samples) = self.samples.try_lock() {
                    // Fast copy of the latest batch
                    let len = samples.len();
                    let batch_len = self.batch_buffer.len();
                    if len >= batch_len {
                        samples.copy_within(batch_len..len, 0);
                        samples[len-batch_len..].copy_from_slice(&self.batch_buffer);
                    }
                }
                self.batch_buffer.clear();
            }
        }
        sample
    }
}

impl<I> Source for VisualizerSource<I>
where
    I: Source,
{
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

pub struct AudioEngine {
    _sink_handle: MixerDeviceSink,
    player: Player,
    paused: bool,
    volume: f32,
    pub samples: Arc<Mutex<Vec<f32>>>,
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
            samples: Arc::new(Mutex::new(vec![0.0; 1024])),
        })
    }

    pub fn play(&mut self, path: &str) {
        if let Ok(file) = File::open(path) {
            if let Ok(decoder) = Decoder::try_from(BufReader::new(file)) {
                self.player.clear();
                let viz_source = VisualizerSource::new(decoder, Arc::clone(&self.samples));
                self.player.append(viz_source);
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

    pub fn position(&self) -> Duration {
        self.player.get_pos()
    }

    pub fn seek(&mut self, duration: Duration) {
        let _ = self.player.try_seek(duration);
    }
}
