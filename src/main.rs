use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use urlencoding::encode;

#[derive(Debug, Serialize, Deserialize)]
struct GameResult {
    abbreviation: String,
    released: u16,
    links: Vec<Link>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Link {
    rel: String,
    uri: String,
}

#[derive(Debug, Deserialize)]
struct RecordCategory {
    game: String,
    category: String,
    runs: Vec<RunObj>,
}

#[derive(Debug, Deserialize)]
struct RunObj {
    place: u8,
    run: Run,
}

#[derive(Debug)]
struct Run {
    weblink: String,
    video: String,
    time: String,
    submitted: String,
}

// Figured out how to deserialize & flatten deeply nested JSON from Stack Overflow Answer: https://stackoverflow.com/a/48978402
impl<'de> Deserialize<'de> for Run {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct _Run {
            weblink: String,
            videos: Vids,
            times: Times,
            submitted: String,
        }

        #[derive(Deserialize)]
        struct Vids {
            links: Vec<_Links>,
        }

        #[derive(Deserialize)]
        struct _Links {
            uri: String,
        }

        #[derive(Deserialize)]
        struct Times {
            realtime: String,
        }

        let help = _Run::deserialize(deserializer)?;

        Ok(Run {
            weblink: help.weblink,
            video: help
                .videos
                .links
                .iter()
                .map(|link| link.uri.clone())
                .collect::<Vec<String>>()
                .first()
                .cloned()
                .unwrap(),
            time: help.times.realtime,
            submitted: help.submitted,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let game_title = "super mario odyssey";

    let resp = reqwest::get(&format!(
        "https://speedrun.com/api/v1/games?name={}",
        encode(game_title)
    ))
    .await?
    .json::<HashMap<String, Value>>()
    .await?;

    let game: GameResult = serde_json::from_str(&resp["data"][0].to_string())?;

    println!("Game: {:?}", game);
    println!(
        "Records URL: {}",
        &game
            .links
            .clone()
            .into_iter()
            .find(|link| link.rel == "records")
            .unwrap()
            .uri
    );

    let records_resp = reqwest::get(
        &game
            .links
            .into_iter()
            .find(|link| link.rel == "records")
            .unwrap()
            .uri,
    )
    .await?
    .json::<HashMap<String, Value>>()
    .await?;

    let records: Vec<RecordCategory> = serde_json::from_str(&records_resp["data"].to_string())?;

    dbg!(records);

    Ok(())
}

#[test]
fn should_encode_str() {
    assert_eq!("Hello%20World".to_string(), encode("Hello World"));
}
