use crate::models::Track;
use crate::config::LastFmConfig;
use serde::Deserialize;
use reqwest::Client;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LrcLibResponse {
    pub synced_lyrics: Option<String>,
    pub plain_lyrics: Option<String>,
}

pub async fn fetch_online_lyrics(track: &Track) -> Option<String> {
    let client = Client::new();
    
    // 1. Try exact match first
    let url = format!(
        "https://lrclib.net/api/get?artist_name={}&track_name={}&album_name={}&duration={}",
        urlencoding::encode(&track.artist),
        urlencoding::encode(&track.title),
        urlencoding::encode(&track.album),
        track.duration_secs
    );

    if let Ok(resp) = client.get(&url).send().await {
        if resp.status().is_success() {
            if let Ok(lyrics_data) = resp.json::<LrcLibResponse>().await {
                return lyrics_data.synced_lyrics.or(lyrics_data.plain_lyrics);
            }
        }
    }

    // 2. Fallback: Try search API if exact match fails
    // This handles cases where duration or album name slightly differs
    let search_url = format!(
        "https://lrclib.net/api/search?artist_name={}&track_name={}",
        urlencoding::encode(&track.artist),
        urlencoding::encode(&track.title)
    );

    if let Ok(resp) = client.get(search_url).send().await {
        if let Ok(results) = resp.json::<Vec<LrcLibResponse>>().await {
            if let Some(first) = results.into_iter().next() {
                return first.synced_lyrics.or(first.plain_lyrics);
            }
        }
    }

    None
}

pub async fn scrobble_to_lastfm(config: &LastFmConfig, track: &Track) -> anyhow::Result<()> {
    if !config.enabled || config.api_key.is_empty() || config.session_key.is_empty() {
        return Ok(());
    }

    let client = Client::new();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .to_string();

    let mut params = BTreeMap::new();
    params.insert("method".to_string(), "track.scrobble".to_string());
    params.insert("artist".to_string(), track.artist.clone());
    params.insert("track".to_string(), track.title.clone());
    params.insert("timestamp".to_string(), timestamp);
    params.insert("api_key".to_string(), config.api_key.clone());
    params.insert("sk".to_string(), config.session_key.clone());

    let sig_params: BTreeMap<&str, &str> = params.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let signature = generate_lastfm_signature(&sig_params, &config.api_secret);
    
    params.insert("api_sig".to_string(), signature);
    params.insert("format".to_string(), "json".to_string());

    let _ = client.post("https://ws.audioscrobbler.com/2.0/")
        .form(&params)
        .send()
        .await?;

    Ok(())
}

fn generate_lastfm_signature(params: &BTreeMap<&str, &str>, secret: &str) -> String {
    let mut s = String::new();
    for (k, v) in params {
        s.push_str(k);
        s.push_str(v);
    }
    s.push_str(secret);
    format!("{:x}", md5::compute(s))
}
