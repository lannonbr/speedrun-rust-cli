use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use urlencoding::encode;

#[macro_use(row)]
extern crate tabular;

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

async fn get_game_result(game_title: &str) -> Result<GameResult, Box<dyn std::error::Error>> {
    let resp = reqwest::get(&format!(
        "https://speedrun.com/api/v1/games?name={}",
        encode(game_title)
    ))
    .await?
    .json::<HashMap<String, Value>>()
    .await?;

    Ok(serde_json::from_str(&resp["data"][0].to_string())?)
}

#[derive(Debug, Deserialize)]
struct RecordCategory {
    game: String,
    weblink: String,
    category: String,
    runs: Vec<RunObj>,
}

impl fmt::Display for RecordCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut table = tabular::Table::new("{:<}  {:<}  {:<}");
        for run in &self.runs {
            let time: String = {
                if let iso8601::Duration::YMDHMS {
                    year: _,
                    month: _,
                    day: _,
                    hour,
                    minute,
                    second,
                    millisecond,
                } = run.run.time
                {
                    format!("{:02}:{:02}:{:02}:{:02}", hour, minute, second, millisecond)
                } else {
                    "No time provided".to_string()
                }
            };

            table.add_row(row!(&run.place, &run.run.video, time));
        }
        write!(f, "{}", table)
    }
}

async fn get_game_records(uri: &str) -> Result<Vec<RecordCategory>, Box<dyn std::error::Error>> {
    let records_resp = reqwest::get(uri)
        .await?
        .json::<HashMap<String, Value>>()
        .await?;

    Ok(serde_json::from_str(&records_resp["data"].to_string())?)
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
    time: iso8601::Duration,
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

        let videos = help
            .videos
            .links
            .iter()
            .map(|link| link.uri.clone())
            .collect::<Vec<String>>();

        let video = if videos.len() == 2 {
            videos[1].to_owned()
        } else {
            videos[0].to_owned()
        };

        let time = {
            match iso8601::duration(&help.times.realtime) {
                Ok(date) => date,
                Err(err) => {
                    panic!("Error parsing duration {:#?}", err);
                }
            }
        };

        Ok(Run {
            weblink: help.weblink,
            video,
            time,
            submitted: help.submitted,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CategoryObj {
    id: String,
    name: String,
    r#type: String,
}

async fn get_categories(
    uri: &str,
) -> Result<HashMap<String, CategoryObj>, Box<dyn std::error::Error>> {
    let categories_resp = reqwest::get(uri)
        .await?
        .json::<HashMap<String, Value>>()
        .await?;

    let categories: Vec<CategoryObj> = serde_json::from_str(&categories_resp["data"].to_string())?;

    let mut hash = HashMap::new();

    for cat in categories {
        hash.insert(
            cat.id.clone(),
            CategoryObj {
                id: cat.id.clone(),
                name: cat.name,
                r#type: cat.r#type,
            },
        );
    }

    Ok(hash)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let game_title = "super mario odyssey";

    let game = get_game_result(game_title).await?;

    let records_endpoint_uri: String = game
        .links
        .iter()
        .find(|link| link.rel == "records")
        .unwrap()
        .uri
        .clone();

    let category_endpoint_uri: String = game
        .links
        .iter()
        .find(|link| link.rel == "categories")
        .unwrap()
        .uri
        .clone();

    let categories = get_categories(&category_endpoint_uri).await?;

    let mut records: Vec<RecordCategory> = get_game_records(&records_endpoint_uri).await?;
    records.retain(|record| {
        let cat = &categories.get(&record.category).unwrap();
        cat.r#type == "per-game"
    });

    for category in &records {
        println!("{}", category);
    }

    Ok(())
}

#[test]
fn should_encode_str() {
    assert_eq!("Hello%20World".to_string(), encode("Hello World"));
}
