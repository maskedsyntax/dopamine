use mpris_player::{Metadata, MprisPlayer, PlaybackStatus};
use std::sync::Arc;
use std::time::Duration;
use std::sync::mpsc::Sender;
use crate::Message;
use crate::models::Track;

pub struct MprisEngine {
    player: Arc<MprisPlayer>,
}

impl MprisEngine {
    pub fn new(tx: Sender<Message>) -> Self {
        let player = MprisPlayer::new(
            "dopamine".to_string(),
            "Dopamine".to_string(),
            "".to_string()
        );
        
        player.set_can_control(true);
        player.set_can_play(true);
        player.set_can_pause(true);
        player.set_can_go_next(true);
        player.set_can_go_previous(true);

        // Callbacks
        let tx_pp = tx.clone();
        player.connect_play_pause(move || { let _ = tx_pp.send(Message::MprisPlayPause); });
        
        let tx_next = tx.clone();
        player.connect_next(move || { let _ = tx_next.send(Message::MprisNext); });
        
        let tx_prev = tx.clone();
        player.connect_previous(move || { let _ = tx_prev.send(Message::MprisPrevious); });
        
        Self { player }
    }

    pub fn update(&self, is_paused: bool, current_track: &Option<Track>, position: Duration) {
        let status = if is_paused {
            PlaybackStatus::Paused
        } else if current_track.is_some() {
            PlaybackStatus::Playing
        } else {
            PlaybackStatus::Stopped
        };
        self.player.set_playback_status(status);

        if let Some(track) = current_track {
            let mut m = Metadata::new();
            m.title = Some(track.title.clone());
            m.artist = Some(vec![track.artist.clone()]);
            m.album = Some(track.album.clone());
            m.length = Some(track.duration_secs * 1_000_000); // Microseconds
            self.player.set_metadata(m);
            
            let pos_us = position.as_micros() as i64;
            self.player.set_position(pos_us);
        }
    }
}
