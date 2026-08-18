use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: i32,
    pub favorite: bool,
    pub play_count: i32,
    pub last_played: Option<i64>,
    pub duration_secs: i64,
    pub lyrics: Option<String>,
    pub lyrics_offset_ms: i64,
}

pub fn artist_credits(value: &str) -> Vec<&str> {
    let mut seen = HashSet::new();
    value
        .split([',', ';', '，', '；'])
        .map(str::trim)
        .filter(|credit| !credit.is_empty())
        .filter(|credit| seen.insert(credit.to_lowercase()))
        .collect()
}

pub fn artist_credit_matches(value: &str, artist: &str) -> bool {
    let artist = artist.trim().to_lowercase();
    artist_credits(value)
        .into_iter()
        .any(|credit| credit.to_lowercase() == artist)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artist_credits_split_collaborations_and_remove_duplicates() {
        assert_eq!(
            artist_credits("Travis Scott, Don Toliver; TRAVIS SCOTT"),
            ["Travis Scott", "Don Toliver"]
        );
        assert_eq!(
            artist_credits("JUBIN NAUTIYAL， ANTARA MITRA"),
            ["JUBIN NAUTIYAL", "ANTARA MITRA"]
        );
    }

    #[test]
    fn ampersands_remain_part_of_an_artist_name() {
        assert_eq!(
            artist_credits("Selena Gomez & The Scene"),
            ["Selena Gomez & The Scene"]
        );
    }
}
