use bevy::prelude::*;
use bevy::window::WindowResolution;
use benchy_common as net;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use web_sys::WebSocket;

thread_local! {
    static INBOX: RefCell<Vec<net::ServerMsg>> = RefCell::new(Vec::new());
    static OUTBOX: RefCell<Vec<String>> = RefCell::new(Vec::new());
    static SOCK: RefCell<Option<WebSocket>> = RefCell::new(None);
    static CONNECTED: Cell<bool> = Cell::new(false);
}

const BORDER: u32 = 0x2a3947;
const ROCK: u32 = 0x93a1b5;
const SHIP: u32 = 0xf2f5fa;
const ME: u32 = 0x5ad1c8;
const FIRE: u32 = 0xffb454;
const HIT: u32 = 0xff5c7a;
const RADIUS: [f32; 4] = [0.0, 22.0, 38.0, 60.0];

#[derive(Resource, Default)]
struct View {
    me: u32,
    name: String,
    tick: u64,
    last_rx: f64,
    players: Vec<net::Player>,
    asteroids: Vec<net::Asteroid>,
    bullets: Vec<net::Bullet>,
    fx: Vec<(f32, f32, u8, f64)>,
    smooth: HashMap<u32, Vec2>,
    last_rot: i8,
    last_thrust: bool,
    fire_at: f64,
}

fn pt(x: f32, y: f32) -> Vec3 {
    Vec3::new(x, -y, 0.0)
}

// circle_2d takes an Isometry2d, so it needs the same y-flip as pt()
fn iso(x: f32, y: f32) -> Isometry2d {
    Isometry2d::from_translation(Vec2::new(x, -y))
}

fn col(c: u32, a: f32) -> Color {
    let aa = (a.clamp(0.0, 1.0) * 255.0) as u32;
    Color::srgba_u32((c << 8) | aa)
}

fn log(s: &str) {
    web_sys::console::log_1(&JsValue::from_str(s));
}

#[wasm_bindgen]
pub fn start(name: &str) {
    console_error_panic_hook::set_once();

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "benchy · asteroids".into(),
            resolution: WindowResolution::new(1280, 720),
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    }));
    app.insert_resource(View {
        name: name.to_string(),
        ..View::default()
    });
    app.add_systems(Startup, (setup, connect));
    app.add_systems(Update, (pump, capture, render, hud).chain());
    app.run();
}

fn setup(mut store: ResMut<GizmoConfigStore>, mut commands: Commands) {
    if let Some((cfg, _)) = store.get_config_mut::<DefaultGizmoConfigGroup>() {
        cfg.line.width = 2.0;
    }
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(col(0x0b1218, 1.0)),
            ..default()
        },
    ));
}

fn connect(view: Res<View>) {
    let Some(win) = web_sys::window() else { return };
    let loc = win.location();
    let scheme = loc.protocol().unwrap_or_default();
    let host = loc.host().unwrap_or_default();
    let url = format!("{}://{}{}", if scheme == "https:" { "wss" } else { "ws" }, host, "/ws");

    let ws = match WebSocket::new(&url) {
        Ok(w) => w,
        Err(e) => {
            log(&format!("websocket failed: {e:?}"));
            return;
        }
    };

    let join = serde_json::to_string(&net::ClientMsg::Join {
        name: view.name.clone(),
    })
    .unwrap();

    let j = join.clone();
    let onopen = Closure::wrap(Box::new(move || {
        CONNECTED.with(|c| c.set(true));
        SOCK.with(|s| {
            if let Some(w) = s.borrow().as_ref() {
                let _ = w.send_with_str(&j);
            }
        });
    }) as Box<dyn FnMut()>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        let Some(s) = e.data().as_string() else { return };
        if let Ok(m) = serde_json::from_str::<net::ServerMsg>(&s) {
            INBOX.with(|q| q.borrow_mut().push(m));
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    let onclose = Closure::wrap(Box::new(move || {
        CONNECTED.with(|c| c.set(false));
    }) as Box<dyn FnMut()>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();

    SOCK.with(|s| *s.borrow_mut() = Some(ws));
}

fn pump(mut view: ResMut<View>, time: Res<Time>) {
    let msgs = INBOX.with(|q| std::mem::take(&mut *q.borrow_mut()));
    for m in msgs {
        match m {
            net::ServerMsg::Welcome { id, name } => {
                view.me = id;
                view.name = name;
                view.last_rx = time.elapsed_secs_f64();
            }
            net::ServerMsg::State {
                tick,
                me,
                players,
                asteroids,
                bullets,
                events,
            } => {
                view.tick = tick;
                view.me = me;
                view.players = players;
                view.asteroids = asteroids;
                view.bullets = bullets;
                let now = time.elapsed_secs_f64();
                for e in events {
                    view.fx.push((e.x, e.y, e.size, now));
                }
                if view.fx.len() > 60 {
                    let n = view.fx.len() - 60;
                    view.fx.drain(..n);
                }
                view.last_rx = now;
            }
        }
    }

    let out = OUTBOX.with(|q| std::mem::take(&mut *q.borrow_mut()));
    if !out.is_empty() {
        SOCK.with(|s| {
            if let Some(w) = s.borrow().as_ref() {
                for m in out {
                    let _ = w.send_with_str(&m);
                }
            }
        });
    }
}

fn capture(mut view: ResMut<View>, input: Res<ButtonInput<KeyCode>>, time: Res<Time>) {
    let left = input.pressed(KeyCode::KeyA) || input.pressed(KeyCode::ArrowLeft);
    let right = input.pressed(KeyCode::KeyD) || input.pressed(KeyCode::ArrowRight);
    let thrust = input.pressed(KeyCode::KeyW) || input.pressed(KeyCode::ArrowUp);
    // one event per press; holding repeats at the fire cooldown rate
    let now = time.elapsed_secs_f64();
    let fire = input.pressed(KeyCode::Space) && now >= view.fire_at;
    if fire {
        view.fire_at = now + 0.28;
    }

    let rot = right as i8 - left as i8;
    if rot != view.last_rot || thrust != view.last_thrust || fire {
        view.last_rot = rot;
        view.last_thrust = thrust;
        let s = serde_json::to_string(&net::ClientMsg::Input {
            rot,
            thrust,
            fire,
        })
        .unwrap();
        OUTBOX.with(|q| q.borrow_mut().push(s));
    }
}

fn render(
    mut gizmos: Gizmos,
    mut view: ResMut<View>,
    time: Res<Time>,
    mut cams: Query<&mut Transform, With<Camera2d>>,
) {
    let now = time.elapsed_secs_f64();
    let dt = time.delta_secs_f64().max(0.0001) as f32;
    let k = 1.0 - (-dt * 9.0).exp();

    let targets: Vec<(u32, Vec2)> = view
        .players
        .iter()
        .map(|p| (p.id, Vec2::new(p.x, p.y)))
        .chain(view.asteroids.iter().map(|a| (a.id, Vec2::new(a.x, a.y))))
        .chain(view.bullets.iter().map(|b| (b.id, Vec2::new(b.x, b.y))))
        .collect();

    for (id, t) in targets {
        let prev = view.smooth.get(&id).copied();
        let next = match prev {
            Some(p) if (t - p).length() < 220.0 => p + (t - p) * k,
            _ => t,
        };
        view.smooth.insert(id, next);
    }

    if let Some(&p) = view.smooth.get(&view.me) {
        for mut tf in &mut cams {
            tf.translation = tf.translation.lerp(pt(p.x, p.y), 0.12);
        }
    }

    let corners = [
        pt(0.0, 0.0),
        pt(net::ARENA_W, 0.0),
        pt(net::ARENA_W, net::ARENA_H),
        pt(0.0, net::ARENA_H),
    ];
    for i in 0..4 {
        gizmos.line(corners[i], corners[(i + 1) % 4], col(BORDER, 0.8));
    }

    for a in &view.asteroids {
        let Some(&c) = view.smooth.get(&a.id) else { continue };
        let r = RADIUS[a.size as usize];
        let n = a.shape.len();
        for i in 0..n {
            let a1 = a.angle + (i as f32) * std::f32::consts::TAU / (n as f32);
            let a2 = a.angle + ((i + 1) as f32) * std::f32::consts::TAU / (n as f32);
            gizmos.line(
                pt(c.x + a1.cos() * r * a.shape[i], c.y + a1.sin() * r * a.shape[i]),
                pt(
                    c.x + a2.cos() * r * a.shape[(i + 1) % n],
                    c.y + a2.sin() * r * a.shape[(i + 1) % n],
                ),
                col(ROCK, 0.9),
            );
        }
    }

    for b in &view.bullets {
        let Some(&c) = view.smooth.get(&b.id) else { continue };
        gizmos.circle_2d(iso(c.x, c.y), 3.0, col(FIRE, 1.0));
    }

    for p in &view.players {
        let Some(&c) = view.smooth.get(&p.id) else { continue };
        if !p.alive {
            continue;
        }
        let is_me = p.id == view.me;
        let color = if is_me { col(ME, 1.0) } else { col(SHIP, 0.75) };

        let nose = pt(c.x + p.angle.cos() * 16.0, c.y + p.angle.sin() * 16.0);
        let left = pt(c.x + (p.angle + 2.4).cos() * 12.0, c.y + (p.angle + 2.4).sin() * 12.0);
        let right = pt(c.x + (p.angle - 2.4).cos() * 12.0, c.y + (p.angle - 2.4).sin() * 12.0);
        gizmos.line(nose, left, color);
        gizmos.line(left, right, color);
        gizmos.line(right, nose, color);

        if p.thrust {
            let back = pt(c.x - p.angle.cos() * 22.0, c.y - p.angle.sin() * 22.0);
            gizmos.line(pt(c.x, c.y), back, col(FIRE, 0.7));
        }

        if !is_me {
            gizmos.circle_2d(iso(c.x, c.y), 20.0, col(SHIP, 0.25));
        }
    }

    for (x, y, size, born) in &view.fx {
        let age = (now - born) as f32;
        if age > 0.45 {
            continue;
        }
        let base = if *size == 0 { 18.0 } else { RADIUS[*size as usize] };
        let r = base * (0.5 + age * 1.6);
        gizmos.circle_2d(
            iso(*x, *y),
            r,
            col(HIT, 1.0 - age / 0.45),
        );
    }
}

fn hud(view: Res<View>, time: Res<Time>) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let Some(el) = doc.get_element_by_id("hud") else { return };

    let me = view.players.iter().find(|p| p.id == view.me);
    let line1 = match me {
        Some(p) => format!(
            "{}  ·  score {}  ·  deaths {}  ·  {}",
            p.name,
            p.score,
            p.deaths,
            if p.alive { "alive" } else { "respawning" }
        ),
        None => format!("{}  ·  waiting for state…", view.name),
    };
    let connected = CONNECTED.with(|c| c.get());
    let age = (time.elapsed_secs_f64() - view.last_rx) * 1000.0;
    let line2 = format!(
        "{}  ·  tick {}  ·  {} players  ·  state {} ms ago",
        if connected { "connected" } else { "disconnected" },
        view.tick,
        view.players.len(),
        age as i64
    );
    el.set_text_content(Some(&format!("{line1}\n{line2}")));
}
