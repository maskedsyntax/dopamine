use anyhow::Result;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source, Sample, ChannelCount, SampleRate};
use std::fs::File;
use std::io::BufReader;
use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

pub struct VisualizerSource<I>
where
    I: Source,
{
    inner: I,
    samples: Arc<Vec<AtomicI32>>,
    index: Arc<AtomicUsize>,
}

impl<I> VisualizerSource<I>
where
    I: Source,
{
    pub fn new(inner: I, samples: Arc<Vec<AtomicI32>>, index: Arc<AtomicUsize>) -> Self {
        Self { inner, samples, index }
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
            // Atomic store: zero locking overhead, zero wait.
            let idx = self.index.fetch_add(1, Ordering::Relaxed) % self.samples.len();
            self.samples[idx].store((s * 1000000.0) as i32, Ordering::Relaxed);
        }
        sample
    }
}

impl<I> Source for VisualizerSource<I>
where
    I: Source,
{
    fn current_span_len(&self) -> Option<usize> { self.inner.current_span_len() }
    fn channels(&self) -> ChannelCount { self.inner.channels() }
    fn sample_rate(&self) -> SampleRate { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<Duration> { self.inner.total_duration() }
}

pub struct AudioEngine {
    _sink_handle: MixerDeviceSink,
    player: Player,
    paused: bool,
    volume: f32,
    pub samples: Arc<Vec<AtomicI32>>,
    pub index: Arc<AtomicUsize>,
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let sink_handle = DeviceSinkBuilder::open_default_sink()
            .map_err(|_| anyhow::anyhow!("Failed to open default audio stream"))?;
        let player = Player::connect_new(&sink_handle.mixer());
        player.set_volume(0.5);

        let samples = Arc::new((0..1024).map(|_| AtomicI32::new(0)).collect());
        let index = Arc::new(AtomicUsize::new(0));

        Ok(Self {
            _sink_handle: sink_handle,
            player,
            paused: false,
            volume: 0.5,
            samples,
            index,
        })
    }

    pub fn play(&mut self, path: &str) {
        if let Ok(file) = File::open(path) {
            if let Ok(decoder) = Decoder::try_from(BufReader::new(file)) {
                self.player.clear();
                let viz_source = VisualizerSource::new(decoder, Arc::clone(&self.samples), Arc::clone(&self.index));
                self.player.append(viz_source);
                self.player.set_volume(self.volume);
                self.player.play();
                self.paused = false;
            }
        }
    }

    pub fn toggle(&mut self) {
        if self.player.empty() { return; }
        if self.paused { self.player.play(); } else { self.player.pause(); }
        self.paused = !self.paused;
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.player.set_volume(self.volume);
    }

    pub fn volume(&self) -> f32 { self.volume }
    pub fn is_paused(&self) -> bool { self.paused }
    pub fn is_empty(&self) -> bool { self.player.empty() }
    pub fn position(&self) -> Duration { self.player.get_pos() }
    pub fn seek(&mut self, duration: Duration) { let _ = self.player.try_seek(duration); }
}
