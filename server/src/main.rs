use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha1::{Digest, Sha1};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::protocol::Role;
use tungstenite::{Error, Message, WebSocket};

use benchy_common as net;

const STEP: f32 = 0.05;
const TURN: f32 = 3.2;
const ACCEL: f32 = 260.0;
const MAXV: f32 = 340.0;
const DRAG: f32 = 0.05;
const BULLET_V: f32 = 620.0;
const BULLET_TTL: f32 = 1.1;
const FIRE_CD: f32 = 0.28;
const SHIP_R: f32 = 14.0;
const RESPAWN: f32 = 1.5;
const INVULN: f32 = 2.0;
const RADIUS: [f32; 4] = [0.0, 22.0, 38.0, 60.0];
const SCORE: [u32; 4] = [0, 60, 40, 20];
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const TAU: f32 = std::f32::consts::TAU;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn f(&mut self) -> f32 {
        (self.next() % 100000) as f32 / 100000.0
    }
    fn r(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    fn shape(&mut self) -> [f32; 8] {
        let mut s = [0.0f32; 8];
        for v in &mut s {
            *v = 0.72 + 0.56 * self.f();
        }
        s
    }
}

#[derive(Clone, Copy, Default)]
struct Input {
    rot: i8,
    thrust: bool,
    fire: bool,
}

struct Player {
    id: u32,
    name: String,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    angle: f32,
    input: Input,
    alive: bool,
    score: u32,
    deaths: u32,
    cooldown: f32,
    respawn: f32,
    invuln: f32,
    tx: Sender<String>,
}

struct Asteroid {
    id: u32,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    angle: f32,
    spin: f32,
    size: u8,
    shape: [f32; 8],
    dead: bool,
}

struct Bullet {
    id: u32,
    owner: u32,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    ttl: f32,
    dead: bool,
}

struct World {
    players: Vec<Player>,
    asteroids: Vec<Asteroid>,
    bullets: Vec<Bullet>,
    events: Vec<net::Event>,
    tick: u64,
    next_id: u32,
    rng: Rng,
}

impl World {
    fn new() -> World {
        let mut w = World {
            players: Vec::new(),
            asteroids: Vec::new(),
            bullets: Vec::new(),
            events: Vec::new(),
            tick: 0,
            next_id: 1,
            rng: Rng(0x9E3779B97F4A7C15),
        };
        for _ in 0..10 {
            let x = w.rng.r(120.0, net::ARENA_W - 120.0);
            let y = w.rng.r(120.0, net::ARENA_H - 120.0);
            let sp = w.rng.r(40.0, 90.0);
            let a = w.rng.r(0.0, TAU);
            let ang = w.rng.r(0.0, TAU);
            let spin = w.rng.r(-1.2, 1.2);
            let shape = w.rng.shape();
            w.asteroids.push(Asteroid {
                id: w.next_id,
                x,
                y,
                vx: a.cos() * sp,
                vy: a.sin() * sp,
                angle: ang,
                spin,
                size: 3,
                shape,
                dead: false,
            });
            w.next_id += 1;
        }
        w
    }
}

fn bounce(x: &mut f32, y: &mut f32, vx: &mut f32, vy: &mut f32, r: f32) {
    if *x < r {
        *x = r;
        *vx = -*vx;
    } else if *x > net::ARENA_W - r {
        *x = net::ARENA_W - r;
        *vx = -*vx;
    }
    if *y < r {
        *y = r;
        *vy = -*vy;
    } else if *y > net::ARENA_H - r {
        *y = net::ARENA_H - r;
        *vy = -*vy;
    }
}

fn step(w: &mut World) {
    w.tick += 1;
    w.events.clear();

    let bullet_count = w.bullets.len();
    let mut shots: Vec<Bullet> = Vec::new();

    for p in &mut w.players {
        if !p.alive {
            p.respawn -= STEP;
            if p.respawn <= 0.0 {
                p.alive = true;
                p.x = w.rng.r(120.0, net::ARENA_W - 120.0);
                p.y = w.rng.r(120.0, net::ARENA_H - 120.0);
                p.vx = 0.0;
                p.vy = 0.0;
                p.angle = w.rng.r(0.0, TAU);
                p.invuln = INVULN;
            }
            continue;
        }

        p.angle += p.input.rot as f32 * TURN * STEP;
        if p.input.thrust {
            p.vx += p.angle.cos() * ACCEL * STEP;
            p.vy += p.angle.sin() * ACCEL * STEP;
        }
        p.vx *= 1.0 - DRAG;
        p.vy *= 1.0 - DRAG;
        let sp = (p.vx * p.vx + p.vy * p.vy).sqrt();
        if sp > MAXV {
            let k = MAXV / sp;
            p.vx *= k;
            p.vy *= k;
        }
        p.x += p.vx * STEP;
        p.y += p.vy * STEP;
        bounce(&mut p.x, &mut p.y, &mut p.vx, &mut p.vy, SHIP_R);

        p.cooldown -= STEP;
        p.invuln -= STEP;

        if p.input.fire && p.cooldown <= 0.0 && bullet_count < 60 {
            p.cooldown = FIRE_CD;
            shots.push(Bullet {
                id: w.next_id,
                owner: p.id,
                x: p.x + p.angle.cos() * (SHIP_R + 6.0),
                y: p.y + p.angle.sin() * (SHIP_R + 6.0),
                vx: p.angle.cos() * BULLET_V,
                vy: p.angle.sin() * BULLET_V,
                ttl: BULLET_TTL,
                dead: false,
            });
            w.next_id += 1;
        }
    }
    w.bullets.extend(shots);

    for b in &mut w.bullets {
        if b.dead {
            continue;
        }
        b.ttl -= STEP;
        if b.ttl <= 0.0 {
            b.dead = true;
            continue;
        }
        b.x += b.vx * STEP;
        b.y += b.vy * STEP;
        if b.x < 0.0 || b.x > net::ARENA_W || b.y < 0.0 || b.y > net::ARENA_H {
            b.dead = true;
        }
    }

    let mut hits: Vec<(usize, usize)> = Vec::new();
    for (i, b) in w.bullets.iter().enumerate() {
        if b.dead {
            continue;
        }
        for (j, a) in w.asteroids.iter().enumerate() {
            if a.dead {
                continue;
            }
            let r = RADIUS[a.size as usize];
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            if dx * dx + dy * dy < r * r {
                hits.push((i, j));
                break;
            }
        }
    }

    let mut new_rocks: Vec<Asteroid> = Vec::new();
    for (i, j) in hits {
        let owner = w.bullets[i].owner;
        let (x, y, size) = (w.asteroids[j].x, w.asteroids[j].y, w.asteroids[j].size);
        w.bullets[i].dead = true;
        w.asteroids[j].dead = true;
        w.events.push(net::Event { kind: 1, x, y, size });
        if let Some(p) = w.players.iter_mut().find(|p| p.id == owner) {
            p.score += SCORE[size as usize];
        }
        if size > 1 {
            for _ in 0..2 {
                let a = w.rng.r(0.0, TAU);
                let sp = 60.0 + 70.0 * w.rng.f();
                new_rocks.push(Asteroid {
                    id: w.next_id,
                    x,
                    y,
                    vx: a.cos() * sp,
                    vy: a.sin() * sp,
                    angle: w.rng.r(0.0, TAU),
                    spin: w.rng.r(-1.4, 1.4),
                    size: size - 1,
                    shape: w.rng.shape(),
                    dead: false,
                });
                w.next_id += 1;
            }
        }
    }
    w.asteroids.extend(new_rocks);
    w.asteroids.retain(|a| !a.dead);
    w.bullets.retain(|b| !b.dead);

    for a in &mut w.asteroids {
        a.x += a.vx * STEP;
        a.y += a.vy * STEP;
        a.angle += a.spin * STEP;
        bounce(&mut a.x, &mut a.y, &mut a.vx, &mut a.vy, RADIUS[a.size as usize]);
    }

    let mut dead_players: Vec<usize> = Vec::new();
    for (i, p) in w.players.iter().enumerate() {
        if !p.alive || p.invuln > 0.0 {
            continue;
        }
        for a in &w.asteroids {
            let r = RADIUS[a.size as usize] + SHIP_R - 3.0;
            let dx = p.x - a.x;
            let dy = p.y - a.y;
            if dx * dx + dy * dy < r * r {
                dead_players.push(i);
                break;
            }
        }
    }
    for i in dead_players {
        let p = &mut w.players[i];
        p.alive = false;
        p.deaths += 1;
        p.respawn = RESPAWN;
        p.vx = 0.0;
        p.vy = 0.0;
        w.events.push(net::Event {
            kind: 2,
            x: p.x,
            y: p.y,
            size: 0,
        });
    }
}

fn broadcast(w: &mut World) {
    let players: Vec<net::Player> = w
        .players
        .iter()
        .map(|p| net::Player {
            id: p.id,
            name: p.name.clone(),
            x: p.x,
            y: p.y,
            angle: p.angle,
            thrust: p.input.thrust,
            alive: p.alive,
            score: p.score,
            deaths: p.deaths,
        })
        .collect();
    let asteroids: Vec<net::Asteroid> = w
        .asteroids
        .iter()
        .map(|a| net::Asteroid {
            id: a.id,
            x: a.x,
            y: a.y,
            angle: a.angle,
            size: a.size,
            shape: a.shape.to_vec(),
        })
        .collect();
    let bullets: Vec<net::Bullet> = w
        .bullets
        .iter()
        .map(|b| net::Bullet { id: b.id, x: b.x, y: b.y })
        .collect();
    let events: Vec<net::Event> = w.events.clone();

    for p in &w.players {
        let msg = net::ServerMsg::State {
            tick: w.tick,
            me: p.id,
            players: players.clone(),
            asteroids: asteroids.clone(),
            bullets: bullets.clone(),
            events: events.clone(),
        };
        if let Ok(s) = serde_json::to_string(&msg) {
            let _ = p.tx.send(s);
        }
    }
}

fn read_headers(stream: &mut std::net::TcpStream) -> (String, Option<String>, bool) {
    let mut buf: Vec<u8> = Vec::new();
    let mut b = [0u8; 1];
    while buf.len() < 8192 {
        match stream.read(&mut b) {
            Ok(0) => break,
            Ok(_) => {
                buf.push(b[0]);
                if buf.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    let mut path = String::new();
    let mut key = None;
    let mut gzip = false;
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("sec-websocket-key") {
                key = Some(v.trim().to_string());
            }
            if k.eq_ignore_ascii_case("accept-encoding") && v.contains("gzip") {
                gzip = true;
            }
        } else {
            path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
        }
    }
    (path, key, gzip)
}

fn query_name(path: &str) -> String {
    if let Some(q) = path.split('?').nth(1) {
        for pair in q.split('&') {
            if let Some(v) = pair.strip_prefix("name=") {
                let v = v.trim();
                if !v.is_empty() {
                    return v.to_string();
                }
            }
        }
    }
    "player".to_string()
}

fn http_err(stream: &mut std::net::TcpStream, code: u16, msg: &str) {
    let body = msg.as_bytes();
    write!(
        stream,
        "HTTP/1.1 {code} {msg}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .ok();
    stream.write_all(body).ok();
}

fn serve(stream: &mut std::net::TcpStream, path: &str, web_dir: &str, gzip: bool) {
    let rel = path.split('?').next().unwrap_or("/");
    let rel = rel.trim_start_matches('/');
    let rel = if rel.is_empty() || rel == "/" { "index.html" } else { rel };
    if rel.contains("..") {
        http_err(stream, 400, "bad path");
        return;
    }
    let full = Path::new(web_dir).join(rel);
    let gz_path = Path::new(web_dir).join(format!("{rel}.gz"));
    let (body, encoded) = if gzip && std::fs::exists(&gz_path).unwrap_or(false) {
        match std::fs::read(&gz_path) {
            Ok(g) => (g, true),
            Err(_) => (match std::fs::read(&full) { Ok(b) => b, Err(_) => { http_err(stream, 404, "not found"); return; } }, false),
        }
    } else {
        match std::fs::read(&full) {
            Ok(b) => (b, false),
            Err(_) => {
                http_err(stream, 404, "not found");
                return;
            }
        }
    };
    let ctype = match full.extension().and_then(|e| e.to_str()).unwrap_or("bin") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => "application/octet-stream",
    };
    let encoding = if encoded { "Content-Encoding: gzip\r\n" } else { "" };
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n{encoding}Cache-Control: no-cache\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .ok();
    stream.write_all(&body).ok();
}

fn handle(mut stream: std::net::TcpStream, world: Arc<Mutex<World>>, web_dir: String) {
    let peer = stream.peer_addr().ok();
    let (path, key, gzip) = read_headers(&mut stream);

    if path.starts_with("/ws") {
        let Some(key) = key else {
            http_err(&mut stream, 400, "missing websocket key");
            return;
        };
        let mut d = Sha1::new();
        d.update(key.as_bytes());
        d.update(GUID.as_bytes());
        let accept = STANDARD.encode(d.finalize());
        if write!(
            stream,
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            accept
        )
        .is_err()
        {
            return;
        }

        let name = query_name(&path);
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let id = {
            let mut w = world.lock().unwrap();
            let id = w.next_id;
            w.next_id += 1;
            let px = w.rng.r(150.0, net::ARENA_W - 150.0);
            let py = w.rng.r(150.0, net::ARENA_H - 150.0);
            w.players.push(Player {
                id,
                name: name.clone(),
                x: px,
                y: py,
                vx: 0.0,
                vy: 0.0,
                angle: 0.0,
                input: Input::default(),
                alive: true,
                score: 0,
                deaths: 0,
                cooldown: 0.0,
                respawn: 0.0,
                invuln: INVULN,
                tx,
            });
            id
        };
        println!("[ws] {peer:?} joined as #{id} ({name})");

        let welcome = serde_json::to_string(&net::ServerMsg::Welcome { id, name }).unwrap();

        stream.set_nonblocking(true).ok();
        let mut ws = WebSocket::from_raw_socket(stream, Role::Server, None);
        let _ = ws.send(Message::Text(welcome));

        'conn: loop {
            loop {
                match ws.read() {
                    Ok(msg) => {
                        if let Message::Text(t) = &msg {
                            if let Ok(cm) = serde_json::from_str::<net::ClientMsg>(t) {
                                let mut w = world.lock().unwrap();
                                if let Some(p) = w.players.iter_mut().find(|p| p.id == id) {
                                    match cm {
                                        net::ClientMsg::Join { name } => p.name = name,
                                        net::ClientMsg::Input { rot, thrust, fire } => {
                                            p.input = Input { rot, thrust, fire }
                                        }
                                    }
                                }
                            }
                        } else if let Message::Ping(p) = &msg {
                            let _ = ws.send(Message::Pong(p.clone()));
                        }
                    }
                    Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        println!("[ws] {peer:?} closed ({e})");
                        break 'conn;
                    }
                }
            }
            while let Ok(s) = rx.try_recv() {
                if ws.send(Message::Text(s)).is_err() {
                    break 'conn;
                }
            }
            if ws.flush().is_err() {
                break 'conn;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        {
            let mut w = world.lock().unwrap();
            w.players.retain(|p| p.id != id);
        }
        println!("[ws] {peer:?} removed");
    } else {
        serve(&mut stream, &path, &web_dir, gzip);
    }
}

fn main() {
    let web_dir = std::env::var("WEB_DIR").unwrap_or_else(|_| "web".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(11111);

    let world = Arc::new(Mutex::new(World::new()));

    {
        let w = world.clone();
        thread::spawn(move || {
            let target = Duration::from_millis((STEP * 1000.0) as u64);
            loop {
                let start = Instant::now();
                {
                    let mut g = w.lock().unwrap();
                    step(&mut g);
                    broadcast(&mut g);
                }
                let spent = start.elapsed();
                if spent < target {
                    thread::sleep(target - spent);
                }
            }
        });
    }

    let listener = TcpListener::bind(("0.0.0.0", port)).expect("bind");
    println!("benchy serving {web_dir} on http://0.0.0.0:{port} (ws at /ws)");

    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let w = world.clone();
                let d = web_dir.clone();
                thread::spawn(move || handle(stream, w, d));
            }
            Err(e) => println!("[tcp] {e}"),
        }
    }
}
