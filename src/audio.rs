use anyhow::Result;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source, Sample, ChannelCount, SampleRate, source::SeekError};
use std::fs::File;
use std::io::BufReader;
use std::time::{Duration, Instant};
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

use cpal::traits::{DeviceTrait, HostTrait};

pub struct AudioEngine {
    _sink_handle: MixerDeviceSink,
    players: [Player; 2],
    active_idx: usize,
    paused: bool,
    volume: f32,
    playback_speed: f32,
    current_path: Option<String>,
    seek_offset: Duration,
    pub eq_bands: [f32; 10], // Gain in dB (-10 to +10)
    pub eq_enabled: bool,
    pub fading: Option<(usize, usize, Instant)>, // (out_idx, in_idx, start_time)
    pub samples: Arc<Vec<AtomicI32>>,
    pub index: Arc<AtomicUsize>,
}

impl AudioEngine {
    pub fn list_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.output_devices()
            .map(|devices| {
                devices
                    .map(|d| d.name().unwrap_or_else(|_| "Unknown Device".to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_device(&mut self, device_name: &str) -> Result<()> {
        let host = cpal::default_host();
        let devices = host.output_devices()?;
        let device = devices
            .filter_map(|d| {
                let name = d.name().ok()?;
                if name == device_name { Some(d) } else { None }
            })
            .next()
            .ok_or_else(|| anyhow::anyhow!("Device not found"))?;

        // Re-initialize sink handle with selected device
        let sink_handle = DeviceSinkBuilder::from_device(device)?
            .with_error_callback(|_| {})
            .open_sink_or_fallback()
            .map_err(|_| anyhow::anyhow!("Failed to open selected audio stream"))?;
        
        // Re-create players on the new sink
        let p1 = Player::connect_new(&sink_handle.mixer());
        let p2 = Player::connect_new(&sink_handle.mixer());
        
        self._sink_handle = sink_handle;
        self.players = [p1, p2];
        
        // Resume playback if it was active
        if let Some(path) = self.current_path.clone() {
            let pos = self.position();
            self.play(&path);
            let _ = self.seek(pos);
        }
        
        Ok(())
    }
    pub fn new() -> Result<Self> {
        let sink_handle = DeviceSinkBuilder::from_default_device()
            .map_err(|_| anyhow::anyhow!("Failed to open default audio stream"))?
            .with_error_callback(|_| {})
            .open_sink_or_fallback()
            .map_err(|_| anyhow::anyhow!("Failed to open default audio stream"))?;
        
        let p1 = Player::connect_new(&sink_handle.mixer());
        let p2 = Player::connect_new(&sink_handle.mixer());
        p1.set_volume(0.5);
        p2.set_volume(0.0);

        let samples = Arc::new((0..1024).map(|_| AtomicI32::new(0)).collect());
        let index = Arc::new(AtomicUsize::new(0));

        Ok(Self {
            _sink_handle: sink_handle,
            players: [p1, p2],
            active_idx: 0,
            paused: false,
            volume: 0.5,
            playback_speed: 1.0,
            current_path: None,
            seek_offset: Duration::default(),
            eq_bands: [0.0; 10],
            eq_enabled: false,
            fading: None,
            samples,
            index,
        })
    }

    fn active(&self) -> &Player { &self.players[self.active_idx] }
    fn inactive(&self) -> &Player { &self.players[1 - self.active_idx] }

    pub fn play(&mut self, path: &str) {
        self.current_path = Some(path.to_string());
        self.seek_offset = Duration::default();
        if let Ok(file) = File::open(path) {
            if let Ok(decoder) = Decoder::try_from(BufReader::new(file)) {
                self.players[self.active_idx].clear();
                let viz_source = VisualizerSource::new(decoder, Arc::clone(&self.samples), Arc::clone(&self.index));
                
                let mut source: Box<dyn Source<Item = Sample> + Send> = Box::new(viz_source);
                if self.eq_enabled {
                    if self.eq_bands[0] < -2.0 { source = Box::new(source.high_pass(80)); }
                    if self.eq_bands[9] < -2.0 { source = Box::new(source.low_pass(12000)); }
                }

                self.players[self.active_idx].append(source);
                self.players[self.active_idx].set_volume(self.volume);
                self.players[self.active_idx].set_speed(self.playback_speed);
                self.players[self.active_idx].play();
                self.paused = false;
                
                self.players[1 - self.active_idx].clear();
            }
        }
    }

    pub fn preload(&mut self, path: &str) {
        if let Ok(file) = File::open(path) {
            if let Ok(decoder) = Decoder::try_from(BufReader::new(file)) {
                let inactive_idx = 1 - self.active_idx;
                self.players[inactive_idx].clear();
                // Note: Preloaded tracks don't get the visualizer wrapper until they are active
                // because we only have one set of visualizer samples/index atomics.
                // We'll wrap it during the swap.
                self.players[inactive_idx].append(decoder);
                self.players[inactive_idx].set_volume(0.0);
                self.players[inactive_idx].set_speed(self.playback_speed);
                // We don't call .play() yet, or we call it and keep volume at 0.
                // Rodio players usually need .play() to be ready.
                self.players[inactive_idx].play();
            }
        }
    }

    pub fn swap_players(&mut self, next_path: String) {
        let old_idx = self.active_idx;
        let new_idx = 1 - self.active_idx;
        
        self.active_idx = new_idx;
        self.current_path = Some(next_path);
        self.seek_offset = Duration::default();
        
        // Start fading
        self.fading = Some((old_idx, new_idx, Instant::now()));
        
        if let Some(p) = &self.current_path {
            let path = p.clone();
            self.play_on_idx(new_idx, &path);
        }
    }

    fn play_on_idx(&mut self, idx: usize, path: &str) {
        if let Ok(file) = File::open(path) {
            if let Ok(decoder) = Decoder::try_from(BufReader::new(file)) {
                self.players[idx].clear();
                let viz_source = VisualizerSource::new(decoder, Arc::clone(&self.samples), Arc::clone(&self.index));
                
                let mut source: Box<dyn Source<Item = Sample> + Send> = Box::new(viz_source);
                if self.eq_enabled {
                    if self.eq_bands[0] < -2.0 { source = Box::new(source.high_pass(80)); }
                    if self.eq_bands[9] < -2.0 { source = Box::new(source.low_pass(12000)); }
                }

                self.players[idx].append(source);
                // Volume starts at 0 for fade in
                self.players[idx].set_volume(0.0);
                self.players[idx].set_speed(self.playback_speed);
                self.players[idx].play();
            }
        }
    }

    pub fn update_fades(&mut self) {
        if let Some((out_idx, in_idx, start)) = self.fading {
            let elapsed = start.elapsed().as_secs_f32();
            let duration = 2.0; // 2 second crossfade
            
            if elapsed >= duration {
                self.players[in_idx].set_volume(self.volume);
                self.players[out_idx].set_volume(0.0);
                self.players[out_idx].clear();
                self.fading = None;
            } else {
                let progress = elapsed / duration;
                self.players[in_idx].set_volume(self.volume * progress);
                self.players[out_idx].set_volume(self.volume * (1.0 - progress));
            }
        }
    }

    pub fn toggle(&mut self) {
        let p = &self.players[self.active_idx];
        if p.empty() { return; }
        if self.paused { p.play(); } else { p.pause(); }
        self.paused = !self.paused;
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.players[self.active_idx].set_volume(self.volume);
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.playback_speed = speed.clamp(0.5, 2.0);
        self.players[0].set_speed(self.playback_speed);
        self.players[1].set_speed(self.playback_speed);
    }

    pub fn volume(&self) -> f32 { self.volume }
    pub fn playback_speed(&self) -> f32 { self.playback_speed }
    pub fn is_paused(&self) -> bool { self.paused }
    pub fn is_empty(&self) -> bool { self.players[self.active_idx].empty() }
    
    pub fn position(&self) -> Duration { 
        self.seek_offset + self.players[self.active_idx].get_pos() 
    }
    
    pub fn seek(&mut self, duration: Duration) -> Result<()> {
        let path = match &self.current_path {
            Some(p) => p.clone(),
            None => return Err(anyhow::anyhow!("No track playing")),
        };

        if let Ok(file) = File::open(&path) {
            if let Ok(mut decoder) = Decoder::try_from(BufReader::new(file)) {
                let _ = decoder.try_seek(duration);
                self.seek_offset = duration;
                
                self.players[self.active_idx].clear();
                let viz_source = VisualizerSource::new(decoder, Arc::clone(&self.samples), Arc::clone(&self.index));
                
                let mut source: Box<dyn Source<Item = Sample> + Send> = Box::new(viz_source);
                if self.eq_enabled {
                    if self.eq_bands[0] < -2.0 { source = Box::new(source.high_pass(80)); }
                    if self.eq_bands[9] < -2.0 { source = Box::new(source.low_pass(12000)); }
                }

                self.players[self.active_idx].append(source);
                self.players[self.active_idx].set_volume(self.volume);
                self.players[self.active_idx].set_speed(self.playback_speed);
                self.players[self.active_idx].play();
                self.paused = false;
                return Ok(());
            }
        }
        
        Err(anyhow::anyhow!("Seek failed"))
    }

    pub fn stop(&mut self) {
        self.players[0].clear();
        self.players[1].clear();
    }
}
