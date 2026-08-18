use crate::models::Track;
use anyhow::Result;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum MediaCommand {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    SeekBy(i64),
    SetPosition(Duration),
    SetVolume(f32),
}

#[cfg(target_os = "macos")]
pub struct MediaControlsEngine {
    controls: souvlaki::MediaControls,
    metadata_path: Option<String>,
}

#[cfg(target_os = "macos")]
impl MediaControlsEngine {
    pub fn new() -> Result<(Self, Receiver<MediaCommand>)> {
        use souvlaki::{MediaControlEvent, PlatformConfig, SeekDirection};

        let mut controls = souvlaki::MediaControls::new(PlatformConfig {
            dbus_name: "dopamine",
            display_name: "Dopamine",
            hwnd: None,
        })?;
        let (sender, receiver) = mpsc::channel();
        controls.attach(move |event| {
            let command = match event {
                MediaControlEvent::Play => Some(MediaCommand::Play),
                MediaControlEvent::Pause => Some(MediaCommand::Pause),
                MediaControlEvent::Toggle => Some(MediaCommand::Toggle),
                MediaControlEvent::Next => Some(MediaCommand::Next),
                MediaControlEvent::Previous => Some(MediaCommand::Previous),
                MediaControlEvent::Stop => Some(MediaCommand::Stop),
                MediaControlEvent::Seek(direction) => Some(MediaCommand::SeekBy(match direction {
                    SeekDirection::Forward => 10,
                    SeekDirection::Backward => -10,
                })),
                MediaControlEvent::SeekBy(direction, duration) => {
                    let seconds = duration.as_secs().min(i64::MAX as u64) as i64;
                    Some(MediaCommand::SeekBy(match direction {
                        SeekDirection::Forward => seconds,
                        SeekDirection::Backward => -seconds,
                    }))
                }
                MediaControlEvent::SetPosition(position) => {
                    Some(MediaCommand::SetPosition(position.0))
                }
                MediaControlEvent::SetVolume(volume) => {
                    Some(MediaCommand::SetVolume(volume as f32))
                }
                _ => None,
            };
            if let Some(command) = command {
                let _ = sender.send(command);
            }
        })?;
        Ok((
            Self {
                controls,
                metadata_path: None,
            },
            receiver,
        ))
    }

    pub fn update(
        &mut self,
        track: Option<&Track>,
        paused: bool,
        position: Duration,
    ) -> Result<()> {
        use souvlaki::{MediaMetadata, MediaPlayback, MediaPosition};

        let path = track.map(|track| track.path.as_str());
        if path != self.metadata_path.as_deref() {
            if let Some(track) = track {
                self.controls.set_metadata(MediaMetadata {
                    title: Some(&track.title),
                    album: Some(&track.album),
                    artist: Some(&track.artist),
                    duration: Some(Duration::from_secs(track.duration_secs.max(0) as u64)),
                    ..Default::default()
                })?;
            }
            self.metadata_path = path.map(str::to_owned);
        }

        let progress = Some(MediaPosition(position));
        let playback = if track.is_none() {
            MediaPlayback::Stopped
        } else if paused {
            MediaPlayback::Paused { progress }
        } else {
            MediaPlayback::Playing { progress }
        };
        self.controls.set_playback(playback)?;
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
pub struct MediaControlsEngine;

#[cfg(not(target_os = "macos"))]
impl MediaControlsEngine {
    pub fn new() -> Result<(Self, Receiver<MediaCommand>)> {
        let (_, receiver) = mpsc::channel();
        Ok((Self, receiver))
    }

    pub fn update(
        &mut self,
        _track: Option<&Track>,
        _paused: bool,
        _position: Duration,
    ) -> Result<()> {
        Ok(())
    }
}
