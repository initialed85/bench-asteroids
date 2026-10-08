use serde::{Deserialize, Serialize};

pub const ARENA_W: f32 = 1600.0;
pub const ARENA_H: f32 = 900.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum ClientMsg {
    Join { name: String },
    Input { rot: i8, thrust: bool, fire: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum ServerMsg {
    Welcome { id: u32, name: String },
    State {
        tick: u64,
        me: u32,
        players: Vec<Player>,
        asteroids: Vec<Asteroid>,
        bullets: Vec<Bullet>,
        events: Vec<Event>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: u32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub angle: f32,
    pub thrust: bool,
    pub alive: bool,
    pub score: u32,
    pub deaths: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asteroid {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub angle: f32,
    pub size: u8,
    pub shape: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bullet {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub size: u8,
}
