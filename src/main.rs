use dialoguer::{theme::ColorfulTheme, Select};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use structopt::StructOpt;
use urlencoding::encode;

#[macro_use(row)]
extern crate tabular;

macro_rules! deserialize_with_path {
    ($expression:expr) => {
        serde_path_to_error::deserialize(&mut serde_json::Deserializer::from_str($expression))
    };
}

#[derive(Debug, Deserialize)]
struct GameResult {
    abbreviation: String,
    names: Names,
    released: u16,
    links: Vec<Link>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Link {
    rel: String,
    uri: String,
}

async fn get_game_result(game_title: &str) -> Result<Vec<GameResult>, Box<dyn std::error::Error>> {
    let resp = reqwest::get(&format!(
        "https://speedrun.com/api/v1/games?name={}",
        encode(game_title)
    ))
    .await?
    .json::<HashMap<String, Value>>()
    .await?;

    Ok(deserialize_with_path!(&resp["data"].to_string())?)
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

            let player: String = if run.player_refs.first().unwrap().rel == "user" {
                run.player_refs
                    .first()
                    .expect("No player ref")
                    .id
                    .as_ref()
                    .unwrap()
                    .to_string()
            } else {
                run.player_refs
                    .first()
                    .expect("No player ref")
                    .name
                    .as_ref()
                    .unwrap()
                    .to_string()
            };

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

    Ok(deserialize_with_path!(&records_resp["data"].to_string())?)
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
    id: Option<String>,
    name: Option<String>,
    uri: String,
}

// Figured out how to deserialize & flatten deeply nested JSON from Stack Overflow Answer: https://stackoverflow.com/a/48978402
impl<'de> Deserialize<'de> for Run {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        struct RunObj {
            run: _Run,
        }

        #[derive(Debug, Deserialize)]
        struct _Run {
            id: String,
            weblink: String,
            videos: Vids,
            times: Times,
            submitted: String,
            players: Vec<PlayerRef>,
        }

        #[derive(Debug, Deserialize)]
        struct Vids(Value);

        #[derive(Debug, Deserialize)]
        struct Times {
            realtime: String,
        }

        let help = RunObj::deserialize(deserializer)?;

        let videos: Vec<String> = if !help.run.videos.0.is_null() {
            match help.run.videos.0.get("links").unwrap() {
                Value::Array(arr) => arr
                    .iter()
                    .map(|x| {
                        x.get("uri")
                            .expect("failed to find uri")
                            .as_str()
                            .expect("Failed to convert to string")
                            .to_string()
                    })
                    .collect::<Vec<String>>(),
                _ => vec![],
            }
        } else {
            vec![]
        };

        let video = if videos.len() == 2 {
            videos[1].to_owned()
        } else if videos.len() == 1 {
            videos[0].to_owned()
        } else {
            String::new()
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

    let categories: Vec<CategoryObj> =
        deserialize_with_path!(&categories_resp["data"].to_string())?;

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

    let player: Player = deserialize_with_path!(&player_resp["data"].to_string())?;
    Ok(player)
}

#[derive(StructOpt, Debug)]
#[structopt(name = "speedrun-rust-cli", about = "CLI for exploring speedrun.com")]
enum Opts {
    Game {
        /// Game name
        #[structopt(short, long)]
        name: String,
    },
    Player {
        /// Player ID
        #[structopt(short, long)]
        id: String,

        #[structopt(short, long)]
        debug: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Opts::from_args() {
        Opts::Game { name } => {
            let games = get_game_result(&name).await?;

            if games.is_empty() {
                panic!("No games came back with the search of {}", name)
            }

            let names = &games
                .iter()
                .map(|game| game.names.international.clone())
                .collect::<Vec<Value>>();

            let game_name = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select a game:")
                .default(0)
                .items(&names)
                .interact()?;

            println!("You selected: {:?}", &games[game_name]);

            let selected_game = &games[game_name];

            let records_endpoint_uri: String = selected_game
                .links
                .iter()
                .find(|link| link.rel == "records")
                .unwrap()
                .uri
                .clone();
            let category_endpoint_uri: String = selected_game
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

            println!("Runs for {}\n", selected_game.names.international);
            for category in records {
                let cat_name = &categories.get(&category.category).unwrap().name;
                println!("Category: {}", cat_name);
                println!("{}", category);
            }
        }
        Opts::Player { id, debug } => {
            // example player: 0jm34we8
            let player = get_player(&format!("https://speedrun.com/api/v1/users/{}", id)).await?;
            if debug {
                dbg!(player);
            }
        }
    }

    Ok(())
}

#[test]
fn should_encode_str() {
    assert_eq!("Hello%20World".to_string(), encode("Hello World"));
}
