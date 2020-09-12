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
    runs: Vec<Run>,
}

impl fmt::Display for RecordCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut table = tabular::Table::new("{:<}  {:<}  {:<}  {:<}  {:<}");
        table.add_row(row!("Place", "Run ID", "Player ID", "Run Video", "Time"));
        for (i, run) in self.runs.iter().enumerate() {
            let time: String = {
                if let iso8601::Duration::YMDHMS {
                    year: _,
                    month: _,
                    day: _,
                    hour,
                    minute,
                    second,
                    millisecond,
                } = run.time
                {
                    format!("{:02}:{:02}:{:02}:{:02}", hour, minute, second, millisecond)
                } else {
                    "No time provided".to_string()
                }
            };

            let player = &run.player_refs.first().unwrap().id;

            table.add_row(row!(i, &run.id, player, &run.video, time));
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

#[derive(Debug)]
struct Run {
    id: String,
    weblink: String,
    video: String,
    time: iso8601::Duration,
    submitted: String,
    player_refs: Vec<PlayerRef>,
}

#[derive(Debug, Deserialize)]
struct PlayerRef {
    rel: String,
    id: String,
    uri: String,
}

// Figured out how to deserialize & flatten deeply nested JSON from Stack Overflow Answer: https://stackoverflow.com/a/48978402
impl<'de> Deserialize<'de> for Run {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RunObj {
            run: _Run,
        }

        #[derive(Deserialize)]
        struct _Run {
            id: String,
            weblink: String,
            videos: Vids,
            times: Times,
            submitted: String,
            players: Vec<PlayerRef>,
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

        let help = RunObj::deserialize(deserializer)?;

        let videos = help
            .run
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
            match iso8601::duration(&help.run.times.realtime) {
                Ok(date) => date,
                Err(err) => {
                    panic!("Error parsing duration {:#?}", err);
                }
            }
        };

        Ok(Run {
            id: help.run.id,
            weblink: help.run.weblink,
            video,
            time,
            submitted: help.run.submitted,
            player_refs: help.run.players,
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

#[derive(Debug, Deserialize)]
struct Player {
    id: String,
    names: Names,
}

#[derive(Debug, Deserialize)]
struct Names {
    international: Value,
    japanese: Value,
}

async fn get_player(uri: &str) -> Result<Player, Box<dyn std::error::Error>> {
    let player_resp = reqwest::get(uri)
        .await?
        .json::<HashMap<String, Value>>()
        .await?;

    let player: Player = serde_json::from_str(&player_resp["data"].to_string())?;
    Ok(player)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let game_title = "super mario odyssey";

    let player = get_player("https://speedrun.com/api/v1/users/0jm34we8").await?;

    dbg!(player);

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

    println!("Runs for {}\n", game_title);
    for category in records {
        let cat_name = &categories.get(&category.category).unwrap().name;
        println!("Category: {}", cat_name);
        println!("{}", category);
    }

    Ok(())
}

#[test]
fn should_encode_str() {
    assert_eq!("Hello%20World".to_string(), encode("Hello World"));
}
