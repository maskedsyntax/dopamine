use anyhow::Result;
use rodio::{
    ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Sample, SampleRate, Source,
    source::SeekError,
};
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const EQ_FREQUENCIES: [f32; 10] = [
    60.0, 170.0, 310.0, 600.0, 1_000.0, 3_000.0, 6_000.0, 12_000.0, 14_000.0, 16_000.0,
];

#[derive(Clone, Copy, Default)]
struct FilterState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    fn peaking(sample_rate: f32, frequency: f32, gain_db: f32) -> Self {
        let frequency = frequency.min(sample_rate * 0.45);
        let a = 10.0_f32.powf(gain_db / 40.0);
        let omega = 2.0 * std::f32::consts::PI * frequency / sample_rate;
        let alpha = omega.sin() / 2.0;
        let cosine = omega.cos();
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cosine) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cosine) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    fn process(self, sample: f32, state: &mut FilterState) -> f32 {
        let output = self.b0 * sample + self.b1 * state.x1 + self.b2 * state.x2
            - self.a1 * state.y1
            - self.a2 * state.y2;
        state.x2 = state.x1;
        state.x1 = sample;
        state.y2 = state.y1;
        state.y1 = output;
        output
    }
}

struct EqualizerSource<I: Source> {
    inner: I,
    filters: [Biquad; 10],
    states: Vec<FilterState>,
    channels: usize,
    channel: usize,
}

impl<I: Source> EqualizerSource<I> {
    fn new(inner: I, gains: [f32; 10]) -> Self {
        let channels = inner.channels().get() as usize;
        let sample_rate = inner.sample_rate().get() as f32;
        let filters = std::array::from_fn(|index| {
            Biquad::peaking(sample_rate, EQ_FREQUENCIES[index], gains[index])
        });
        Self {
            inner,
            filters,
            states: vec![FilterState::default(); channels * filters.len()],
            channels,
            channel: 0,
        }
    }
}

impl<I: Source> Iterator for EqualizerSource<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let mut sample = self.inner.next()?;
        for (band, filter) in self.filters.iter().copied().enumerate() {
            sample = filter.process(
                sample,
                &mut self.states[band * self.channels + self.channel],
            );
        }
        self.channel = (self.channel + 1) % self.channels;
        Some(sample.clamp(-1.0, 1.0))
    }
}

impl<I: Source> Source for EqualizerSource<I> {
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
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)
    }
}

pub struct VisualizerSource<I>
where
    I: Source,
{
    inner: I,
    samples: Arc<Vec<AtomicI32>>,
    index: Arc<AtomicUsize>,
    channels: usize,
    channel: usize,
    frame_sum: f32,
}

impl<I> VisualizerSource<I>
where
    I: Source,
{
    pub fn new(inner: I, samples: Arc<Vec<AtomicI32>>, index: Arc<AtomicUsize>) -> Self {
        let channels = inner.channels().get() as usize;
        Self {
            inner,
            samples,
            index,
            channels,
            channel: 0,
            frame_sum: 0.0,
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
            self.frame_sum += s;
            self.channel += 1;
            if self.channel == self.channels {
                let mono = self.frame_sum / self.channels as f32;
                let idx = self.index.fetch_add(1, Ordering::Relaxed) % self.samples.len();
                self.samples[idx].store((mono * 1_000_000.0) as i32, Ordering::Relaxed);
                self.channel = 0;
                self.frame_sum = 0.0;
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
    applied_volume: f32,
    volume_ramp: Option<(f32, f32, Instant)>,
    playback_speed: f32,
    current_path: Option<String>,
    preloaded_path: Option<String>,
    seek_offset: Duration,
    pub eq_bands: [f32; 10], // Gain in dB (-10 to +10)
    pub eq_enabled: bool,
    pub fading: Option<(usize, usize, Instant)>, // (out_idx, in_idx, start_time)
    pub samples: Arc<Vec<AtomicI32>>,
    pub index: Arc<AtomicUsize>,
}

fn smoothstep(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

impl AudioEngine {
    pub fn list_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.output_devices()
            .map(|devices| {
                devices
                    .map(|d| {
                        d.description()
                            .map(|desc| desc.name().to_string())
                            .unwrap_or_else(|_| "Unknown Device".to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_device(&mut self, device_name: &str) -> Result<()> {
        let position = self.position();
        let was_paused = self.paused;
        let host = cpal::default_host();
        let devices = host.output_devices()?;
        let device = devices
            .filter_map(|d| {
                let name = d.description().ok()?.name().to_string();
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
        let p1 = Player::connect_new(sink_handle.mixer());
        let p2 = Player::connect_new(sink_handle.mixer());

        self._sink_handle = sink_handle;
        self.players = [p1, p2];

        // Resume playback if it was active
        if let Some(path) = self.current_path.clone() {
            self.play(&path)?;
            self.seek(position)?;
            if was_paused {
                self.pause();
            }
        }

        Ok(())
    }
    pub fn new() -> Result<Self> {
        let sink_handle = DeviceSinkBuilder::from_default_device()
            .map_err(|_| anyhow::anyhow!("Failed to open default audio stream"))?
            .with_error_callback(|_| {})
            .open_sink_or_fallback()
            .map_err(|_| anyhow::anyhow!("Failed to open default audio stream"))?;

        let p1 = Player::connect_new(sink_handle.mixer());
        let p2 = Player::connect_new(sink_handle.mixer());
        p1.set_volume(0.5);
        p2.set_volume(0.0);

        let samples = Arc::new((0..1024).map(|_| AtomicI32::new(0)).collect());
        let index = Arc::new(AtomicUsize::new(0));

        Ok(Self {
            _sink_handle: sink_handle,
            players: [p1, p2],
            active_idx: 0,
            paused: true,
            volume: 0.5,
            applied_volume: 0.5,
            volume_ramp: None,
            playback_speed: 1.0,
            current_path: None,
            preloaded_path: None,
            seek_offset: Duration::default(),
            eq_bands: [0.0; 10],
            eq_enabled: false,
            fading: None,
            samples,
            index,
        })
    }

    pub fn play(&mut self, path: &str) -> Result<()> {
        let file = File::open(path)?;
        let decoder = Decoder::try_from(BufReader::new(file))?;
        let viz_source =
            VisualizerSource::new(decoder, Arc::clone(&self.samples), Arc::clone(&self.index));

        let source = self.apply_equalizer(Box::new(viz_source));

        self.players[self.active_idx].clear();
        self.players[self.active_idx].append(source);
        self.players[self.active_idx].set_volume(self.applied_volume);
        self.players[self.active_idx].set_speed(self.playback_speed);
        self.players[self.active_idx].play();
        self.players[1 - self.active_idx].clear();
        self.preloaded_path = None;
        self.current_path = Some(path.to_string());
        self.seek_offset = Duration::default();
        self.paused = false;
        Ok(())
    }

    pub fn preload(&mut self, path: &str) -> Result<()> {
        let file = File::open(path)?;
        let decoder = Decoder::try_from(BufReader::new(file))?;
        let inactive_idx = 1 - self.active_idx;
        self.players[inactive_idx].clear();
        let viz_source =
            VisualizerSource::new(decoder, Arc::clone(&self.samples), Arc::clone(&self.index));
        let source = self.apply_equalizer(Box::new(viz_source));
        self.players[inactive_idx].append(source);
        self.players[inactive_idx].set_volume(0.0);
        self.players[inactive_idx].set_speed(self.playback_speed);
        self.players[inactive_idx].pause();
        self.preloaded_path = Some(path.to_string());
        Ok(())
    }

    pub fn swap_players(&mut self, next_path: String) -> Result<()> {
        let old_idx = self.active_idx;
        let new_idx = 1 - self.active_idx;

        if self.preloaded_path.as_deref() == Some(next_path.as_str()) {
            self.players[new_idx].set_volume(0.0);
            self.players[new_idx].play();
        } else {
            self.play_on_idx(new_idx, &next_path)?;
        }
        self.active_idx = new_idx;
        self.current_path = Some(next_path);
        self.preloaded_path = None;
        self.seek_offset = Duration::default();
        self.fading = Some((old_idx, new_idx, Instant::now()));
        self.paused = false;
        Ok(())
    }

    fn play_on_idx(&mut self, idx: usize, path: &str) -> Result<()> {
        let file = File::open(path)?;
        let decoder = Decoder::try_from(BufReader::new(file))?;
        let viz_source =
            VisualizerSource::new(decoder, Arc::clone(&self.samples), Arc::clone(&self.index));

        let source = self.apply_equalizer(Box::new(viz_source));

        self.players[idx].clear();
        self.players[idx].append(source);
        self.players[idx].set_volume(0.0);
        self.players[idx].set_speed(self.playback_speed);
        self.players[idx].play();
        Ok(())
    }

    fn apply_equalizer(
        &self,
        source: Box<dyn Source<Item = Sample> + Send>,
    ) -> Box<dyn Source<Item = Sample> + Send> {
        if self.eq_enabled {
            Box::new(EqualizerSource::new(source, self.eq_bands))
        } else {
            source
        }
    }

    pub fn update_fades(&mut self) {
        let applied_volume = self.update_volume_ramp();
        if let Some((out_idx, in_idx, start)) = self.fading {
            let elapsed = start.elapsed().as_secs_f32();
            let duration = 2.0; // 2 second crossfade

            if elapsed >= duration {
                self.players[in_idx].set_volume(applied_volume);
                self.players[out_idx].set_volume(0.0);
                self.players[out_idx].clear();
                self.fading = None;
            } else {
                let progress = elapsed / duration;
                self.players[in_idx].set_volume(applied_volume * progress);
                self.players[out_idx].set_volume(applied_volume * (1.0 - progress));
            }
        } else {
            self.players[self.active_idx].set_volume(applied_volume);
        }
    }

    fn update_volume_ramp(&mut self) -> f32 {
        let Some((from, to, started)) = self.volume_ramp else {
            return self.applied_volume;
        };
        let progress = (started.elapsed().as_secs_f32() / 0.12).clamp(0.0, 1.0);
        // Smoothstep avoids an abrupt gain slope at either end of the ramp.
        let eased = smoothstep(progress);
        self.applied_volume = from + (to - from) * eased;
        if progress >= 1.0 {
            self.applied_volume = to;
            self.volume_ramp = None;
        }
        self.applied_volume
    }

    pub fn toggle(&mut self) {
        let p = &self.players[self.active_idx];
        if p.empty() {
            return;
        }
        if self.paused {
            p.play();
        } else {
            p.pause();
        }
        self.paused = !self.paused;
    }

    pub fn pause(&mut self) {
        if !self.players[self.active_idx].empty() {
            self.players[self.active_idx].pause();
            self.paused = true;
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        let target = volume.clamp(0.0, 1.0);
        self.update_volume_ramp();
        self.volume = target;
        self.volume_ramp = Some((self.applied_volume, target, Instant::now()));
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.playback_speed = speed.clamp(0.5, 2.0);
        self.players[0].set_speed(self.playback_speed);
        self.players[1].set_speed(self.playback_speed);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }
    pub fn playback_speed(&self) -> f32 {
        self.playback_speed
    }
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    pub fn is_empty(&self) -> bool {
        self.players[self.active_idx].empty()
    }

    pub fn position(&self) -> Duration {
        self.seek_offset + self.players[self.active_idx].get_pos()
    }

    pub fn seek(&mut self, duration: Duration) -> Result<()> {
        let path = match &self.current_path {
            Some(p) => p.clone(),
            None => return Err(anyhow::anyhow!("No track playing")),
        };

        let was_paused = self.paused;
        let file = File::open(&path)?;
        let mut decoder = Decoder::try_from(BufReader::new(file))?;
        decoder.try_seek(duration)?;
        self.seek_offset = duration;

        self.players[self.active_idx].set_volume(0.0);
        self.players[self.active_idx].clear();
        let viz_source =
            VisualizerSource::new(decoder, Arc::clone(&self.samples), Arc::clone(&self.index));

        let source = self.apply_equalizer(Box::new(viz_source));

        self.players[self.active_idx].append(source);
        self.players[self.active_idx].set_volume(0.0);
        self.players[self.active_idx].set_speed(self.playback_speed);
        self.applied_volume = 0.0;
        self.volume_ramp = Some((0.0, self.volume, Instant::now()));
        if was_paused {
            self.players[self.active_idx].pause();
        } else {
            self.players[self.active_idx].play();
        }
        self.paused = was_paused;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.players[0].clear();
        self.players[1].clear();
        self.current_path = None;
        self.preloaded_path = None;
        self.seek_offset = Duration::default();
        self.fading = None;
        self.paused = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use std::num::NonZero;

    #[test]
    fn flat_equalizer_preserves_samples() {
        let samples = vec![0.0, 0.25, -0.5, 0.75, -1.0];
        let source = SamplesBuffer::new(
            NonZero::new(1).unwrap(),
            NonZero::new(44_100).unwrap(),
            samples.clone(),
        );
        let output: Vec<_> = EqualizerSource::new(source, [0.0; 10]).collect();
        for (actual, expected) in output.iter().zip(samples) {
            assert!((actual - expected).abs() < 0.0001);
        }
    }

    #[test]
    fn boosted_equalizer_changes_an_impulse() {
        let source = SamplesBuffer::new(
            NonZero::new(1).unwrap(),
            NonZero::new(44_100).unwrap(),
            vec![1.0, 0.0, 0.0, 0.0],
        );
        let mut gains = [0.0; 10];
        gains[4] = 6.0;
        let output: Vec<_> = EqualizerSource::new(source, gains).collect();
        assert!(output[1].abs() > 0.0001);
    }

    #[test]
    fn visualizer_downmixes_channels_into_mono_frames() {
        let source = SamplesBuffer::new(
            NonZero::new(2).unwrap(),
            NonZero::new(44_100).unwrap(),
            vec![1.0, -1.0, 0.5, 0.5],
        );
        let samples = Arc::new((0..4).map(|_| AtomicI32::new(0)).collect());
        let index = Arc::new(AtomicUsize::new(0));

        let _: Vec<_> =
            VisualizerSource::new(source, Arc::clone(&samples), Arc::clone(&index)).collect();

        assert_eq!(index.load(Ordering::Relaxed), 2);
        assert_eq!(samples[0].load(Ordering::Relaxed), 0);
        assert_eq!(samples[1].load(Ordering::Relaxed), 500_000);
    }

    #[test]
    fn volume_ramp_is_bounded_and_smooth() {
        assert_eq!(smoothstep(-1.0), 0.0);
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(0.5), 0.5);
        assert_eq!(smoothstep(1.0), 1.0);
        assert_eq!(smoothstep(2.0), 1.0);
        assert!(smoothstep(0.25) < 0.25);
        assert!(smoothstep(0.75) > 0.75);
    }
}
