use crate::models::Track;
use rand::seq::SliceRandom;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepeatMode {
    #[default]
    None,
    One,
    All,
}

#[derive(Clone, Debug, Default)]
pub struct PlaybackQueue {
    pub items: Vec<Track>,
    pub current_index: usize,
    pub shuffle: bool,
    pub repeat_mode: RepeatMode,
    order: Vec<usize>,
    order_position: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Removal {
    pub removed_current: bool,
    pub next_index: Option<usize>,
}

impl PlaybackQueue {
    pub fn restore(
        &mut self,
        items: Vec<Track>,
        current_index: usize,
        shuffle: bool,
        repeat_mode: RepeatMode,
    ) {
        self.items = items;
        self.current_index = clamp_index(current_index, self.items.len()).unwrap_or(0);
        self.shuffle = shuffle;
        self.repeat_mode = repeat_mode;
        self.rebuild_order();
    }

    pub fn replace(&mut self, items: Vec<Track>, current_index: usize) {
        self.items = items;
        self.current_index = clamp_index(current_index, self.items.len()).unwrap_or(0);
        self.rebuild_order();
    }

    pub fn replace_for_playback(&mut self, items: Vec<Track>, shuffled: bool) -> Option<&Track> {
        self.items = items;
        self.current_index = 0;
        self.shuffle = shuffled;
        self.rebuild_order();
        self.order_position = 0;
        self.current_index = self.order.first().copied().unwrap_or(0);
        self.current()
    }

    pub fn select(&mut self, index: usize) -> Option<&Track> {
        if index >= self.items.len() {
            return None;
        }
        self.current_index = index;
        self.order_position = self
            .order
            .iter()
            .position(|&item| item == index)
            .unwrap_or(0);
        self.items.get(index)
    }

    pub fn current(&self) -> Option<&Track> {
        self.items.get(self.current_index)
    }

    pub fn move_up(&mut self, index: usize) -> Option<usize> {
        if index == 0 || index >= self.items.len() {
            return None;
        }
        self.items.swap(index, index - 1);
        self.adjust_current_for_swap(index, index - 1);
        self.rebuild_order();
        Some(index - 1)
    }

    pub fn move_down(&mut self, index: usize) -> Option<usize> {
        if index >= self.items.len().saturating_sub(1) {
            return None;
        }
        self.items.swap(index, index + 1);
        self.adjust_current_for_swap(index, index + 1);
        self.rebuild_order();
        Some(index + 1)
    }

    pub fn remove(&mut self, index: usize) -> Option<Removal> {
        if index >= self.items.len() {
            return None;
        }
        let removed_current = index == self.current_index;
        self.items.remove(index);
        self.current_index =
            queue_index_after_removal(self.current_index, index, self.items.len()).unwrap_or(0);
        self.rebuild_order();
        Some(Removal {
            removed_current,
            next_index: clamp_index(self.current_index, self.items.len()),
        })
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        self.rebuild_order();
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat_mode = match self.repeat_mode {
            RepeatMode::None => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::None,
        };
    }

    pub fn advance(&mut self) -> Option<usize> {
        if self.items.is_empty() {
            return None;
        }
        if self.shuffle {
            if self.order.is_empty() {
                self.rebuild_order();
            }
            if self.order_position + 1 >= self.order.len() {
                if self.repeat_mode != RepeatMode::All {
                    return None;
                }
                self.rebuild_order();
                self.order_position = 0;
            } else {
                self.order_position += 1;
            }
            self.current_index = self.order[self.order_position];
        } else if self.current_index + 1 >= self.items.len() {
            if self.repeat_mode != RepeatMode::All {
                return None;
            }
            self.current_index = 0;
        } else {
            self.current_index += 1;
        }
        Some(self.current_index)
    }

    pub fn retreat(&mut self) -> Option<usize> {
        if self.items.is_empty() {
            return None;
        }
        if self.shuffle {
            if self.order_position == 0 {
                if self.repeat_mode != RepeatMode::All {
                    return None;
                }
                self.order_position = self.order.len().saturating_sub(1);
            } else {
                self.order_position -= 1;
            }
            self.current_index = self.order[self.order_position];
        } else if self.current_index == 0 {
            if self.repeat_mode != RepeatMode::All {
                return None;
            }
            self.current_index = self.items.len() - 1;
        } else {
            self.current_index -= 1;
        }
        Some(self.current_index)
    }

    pub fn next_index(&self) -> Option<usize> {
        if self.items.is_empty() || self.repeat_mode == RepeatMode::One {
            return None;
        }
        if self.shuffle {
            if self.order_position + 1 < self.order.len() {
                Some(self.order[self.order_position + 1])
            } else if self.repeat_mode == RepeatMode::All {
                self.order.first().copied()
            } else {
                None
            }
        } else if self.current_index + 1 < self.items.len() {
            Some(self.current_index + 1)
        } else if self.repeat_mode == RepeatMode::All {
            Some(0)
        } else {
            None
        }
    }

    fn rebuild_order(&mut self) {
        self.order = (0..self.items.len()).collect();
        if self.shuffle {
            self.order.shuffle(&mut rand::rng());
        }
        self.order_position = self
            .order
            .iter()
            .position(|&item| item == self.current_index)
            .unwrap_or(0);
    }

    fn adjust_current_for_swap(&mut self, first: usize, second: usize) {
        if self.current_index == first {
            self.current_index = second;
        } else if self.current_index == second {
            self.current_index = first;
        }
    }
}

fn clamp_index(index: usize, len: usize) -> Option<usize> {
    (len > 0).then(|| index.min(len - 1))
}

fn queue_index_after_removal(
    current_index: usize,
    removed_index: usize,
    remaining_len: usize,
) -> Option<usize> {
    if remaining_len == 0 {
        None
    } else if removed_index < current_index {
        Some(current_index - 1)
    } else if removed_index == current_index {
        Some(removed_index.min(remaining_len - 1))
    } else {
        Some(current_index.min(remaining_len - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: &str) -> Track {
        Track {
            path: path.to_string(),
            title: path.to_string(),
            artist: String::new(),
            album: String::new(),
            genre: String::new(),
            year: 0,
            favorite: false,
            play_count: 0,
            last_played: None,
            duration_secs: 0,
            lyrics: None,
            lyrics_offset_ms: 0,
        }
    }

    fn tracks() -> Vec<Track> {
        vec![track("a"), track("b"), track("c"), track("d")]
    }

    #[test]
    fn restore_clamps_an_invalid_current_index() {
        let mut queue = PlaybackQueue::default();
        queue.restore(tracks(), 99, false, RepeatMode::None);
        assert_eq!(queue.current_index, 3);
        assert_eq!(queue.current().map(|track| track.path.as_str()), Some("d"));
    }

    #[test]
    fn replace_for_playback_starts_at_the_first_unshuffled_track() {
        let mut queue = PlaybackQueue::default();
        queue.replace(tracks(), 3);

        let current = queue.replace_for_playback(tracks(), false);

        assert_eq!(current.map(|track| track.path.as_str()), Some("a"));
        assert_eq!(queue.current_index, 0);
        assert!(!queue.shuffle);
        assert_eq!(queue.advance(), Some(1));
    }

    #[test]
    fn replace_for_playback_visits_every_shuffled_track_once_without_repeat_all() {
        let mut queue = PlaybackQueue {
            repeat_mode: RepeatMode::None,
            ..Default::default()
        };
        queue.replace_for_playback(tracks(), true);

        let mut visited = vec![queue.current_index];
        while let Some(index) = queue.advance() {
            visited.push(index);
        }

        visited.sort_unstable();
        assert_eq!(visited, vec![0, 1, 2, 3]);
        assert!(queue.shuffle);
    }

    #[test]
    fn replace_for_playback_accepts_empty_items() {
        let mut queue = PlaybackQueue::default();
        queue.replace(tracks(), 2);

        assert_eq!(queue.replace_for_playback(Vec::new(), true), None);
        assert_eq!(queue.current(), None);
        assert_eq!(queue.advance(), None);
        assert_eq!(queue.retreat(), None);
        assert_eq!(queue.next_index(), None);
    }

    #[test]
    fn replace_for_playback_preserves_repeat_mode() {
        let mut queue = PlaybackQueue {
            repeat_mode: RepeatMode::One,
            ..Default::default()
        };

        queue.replace_for_playback(tracks(), false);

        assert_eq!(queue.repeat_mode, RepeatMode::One);
    }

    #[test]
    fn advances_and_wraps_only_when_repeating_all() {
        let mut queue = PlaybackQueue::default();
        queue.replace(tracks(), 3);
        assert_eq!(queue.advance(), None);
        assert_eq!(queue.current_index, 3);

        queue.repeat_mode = RepeatMode::All;
        assert_eq!(queue.advance(), Some(0));
        assert_eq!(queue.retreat(), Some(3));
    }

    #[test]
    fn removing_current_selects_the_item_that_shifted_into_its_place() {
        let mut queue = PlaybackQueue::default();
        queue.replace(tracks(), 1);
        let removal = queue.remove(1).expect("removal");
        assert!(removal.removed_current);
        assert_eq!(removal.next_index, Some(1));
        assert_eq!(queue.current().map(|track| track.path.as_str()), Some("c"));

        let removal = queue.remove(2).expect("last removal");
        assert!(!removal.removed_current);
        assert_eq!(queue.current().map(|track| track.path.as_str()), Some("c"));
    }

    #[test]
    fn moving_items_keeps_current_track_identity() {
        let mut queue = PlaybackQueue::default();
        queue.replace(tracks(), 1);
        assert_eq!(queue.move_down(1), Some(2));
        assert_eq!(queue.current_index, 2);
        assert_eq!(queue.current().map(|track| track.path.as_str()), Some("b"));
    }

    #[test]
    fn next_index_predicts_without_advancing() {
        let mut queue = PlaybackQueue::default();
        queue.replace(tracks(), 1);
        assert_eq!(queue.next_index(), Some(2));
        assert_eq!(queue.current_index, 1);
        queue.repeat_mode = RepeatMode::One;
        assert_eq!(queue.next_index(), None);
    }
}
