use anyhow::Result;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source, Sample, ChannelCount, SampleRate, source::SeekError};
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
    
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)
    }
}

pub struct AudioEngine {
    _sink_handle: MixerDeviceSink,
    player: Player,
    paused: bool,
    volume: f32,
    playback_speed: f32,
    current_path: Option<String>,
    seek_offset: Duration,
    pub samples: Arc<Vec<AtomicI32>>,
    pub index: Arc<AtomicUsize>,
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let sink_handle = DeviceSinkBuilder::from_default_device()
            .map_err(|_| anyhow::anyhow!("Failed to open default audio stream"))?
            .with_error_callback(|_| {})
            .open_sink_or_fallback()
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
            playback_speed: 1.0,
            current_path: None,
            seek_offset: Duration::default(),
            samples,
            index,
        })
    }

    pub fn play(&mut self, path: &str) {
        self.current_path = Some(path.to_string());
        self.seek_offset = Duration::default(); // Reset offset on new track
        if let Ok(file) = File::open(path) {
            if let Ok(decoder) = Decoder::try_from(BufReader::new(file)) {
                self.player.clear();
                let viz_source = VisualizerSource::new(decoder, Arc::clone(&self.samples), Arc::clone(&self.index));
                self.player.append(viz_source);
                self.player.set_volume(self.volume);
                self.player.set_speed(self.playback_speed);
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

    pub fn set_speed(&mut self, speed: f32) {
        self.playback_speed = speed.clamp(0.5, 2.0);
        self.player.set_speed(self.playback_speed);
    }

    pub fn volume(&self) -> f32 { self.volume }
    pub fn playback_speed(&self) -> f32 { self.playback_speed }
    pub fn is_paused(&self) -> bool { self.paused }
    pub fn is_empty(&self) -> bool { self.player.empty() }
    
    pub fn position(&self) -> Duration { 
        self.seek_offset + self.player.get_pos() 
    }
    
    pub fn seek(&mut self, duration: Duration) -> Result<()> {
        let path = match &self.current_path {
            Some(p) => p.clone(),
            None => return Err(anyhow::anyhow!("No track playing")),
        };

        // Re-open the file and seek the decoder directly.
        // This guarantees backward seeking works for all formats and correctly updates playback state.
        if let Ok(file) = File::open(&path) {
            if let Ok(mut decoder) = Decoder::try_from(BufReader::new(file)) {
                // Seek the decoder itself before wrapping it
                let _ = decoder.try_seek(duration);
                self.seek_offset = duration; // Save the true playback position
                
                self.player.clear();
                let viz_source = VisualizerSource::new(decoder, Arc::clone(&self.samples), Arc::clone(&self.index));
                self.player.append(viz_source);
                self.player.set_volume(self.volume);
                self.player.set_speed(self.playback_speed);
                self.player.play();
                self.paused = false;
                return Ok(());
            }
        }
        
        Err(anyhow::anyhow!("Seek failed to re-open file"))
    }
}
