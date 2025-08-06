use std::io::Cursor;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::{Arc, Mutex};
use std::thread::spawn;
use std::time::Duration;

use ciborium::{from_reader, into_writer};

use uuid::Uuid;

use tungstenite::{accept, Bytes, Error, Message};

use crossbeam_channel::{bounded, Receiver, Sender};

use timed::timed;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
enum Color {
    Red,
    Blue,
    Green,
    Yellow,
}
#[derive(Serialize, Deserialize, Debug)]
struct Player {
    name: String,
    color: Color,
    id: Uuid,
}

#[derive(Serialize, Deserialize, Debug)]
enum WSEventType {
    NewPlayer,
    PlayersInLobby,
    GetPlayersInLobby,
}

#[derive(Serialize, Deserialize, Debug)]
struct WSEvent {
    event_type: WSEventType,
}

static CLIENTS: Mutex<Vec<Player>> = Mutex::new(vec![]);
static CONNECTIONS: AtomicU8 = AtomicU8::new(0);

fn find_next_color() -> Result<Color, ()> {
    let colors = [Color::Red, Color::Blue, Color::Green, Color::Yellow];
    let players = CLIENTS.lock().expect("couldnt lock clients");
    let color = colors
        .into_iter()
        .find(|color| !players.iter().any(|player| player.color.eq(color)));
    println!("free color {:?}", color);
    color.ok_or(())
}

// .find(|&player| player.color.eq(color))
// .is_none()
fn handle_new_player(id: Uuid, cursor: Cursor<&Bytes>) -> Result<Option<Vec<u8>>, ()> {
    println!("handling new player");
    let color = find_next_color()?;
    #[derive(Serialize, Deserialize, Debug)]
    struct NewPlayerEvent {
        name: String,
    }
    let new_player_event: NewPlayerEvent = from_reader(cursor).expect("new player event broken");
    let mut players = CLIENTS.lock().expect("cant get CLIENTS");

    let new_player = Player {
        id,
        name: new_player_event.name,
        color,
    };
    players.push(new_player);
    println!("new player event {:?}", players);
    drop(players);
    Ok(Some(get_players_in_lobby()))
}

fn get_players_in_lobby() -> Vec<u8> {
    #[derive(Serialize)]
    struct PlayersInLobbyEvent {
        event_type: WSEventType,
        players: Vec<PlayerInLobby>,
    }
    #[derive(Serialize)]
    struct PlayerInLobby {
        name: String,
        color: Color,
    }
    let mut players_in_lobby = vec![];
    CLIENTS.lock().unwrap().iter().for_each(|player| {
        players_in_lobby.push(PlayerInLobby {
            name: player.name.clone(),
            color: player.color.clone(),
        })
    });
    let players_in_lobby_event = PlayersInLobbyEvent {
        players: players_in_lobby,
        event_type: WSEventType::PlayersInLobby,
    };
    let mut serialized = Vec::new();
    into_writer(&players_in_lobby_event, &mut serialized)
        .expect("couldnt serialize players in lobby");
    serialized
}

fn handle_binary(id: Uuid, bin: Bytes) -> Result<Option<Vec<u8>>, ()> {
    let type_cursor = Cursor::new(&bin);
    let event_cursor = Cursor::new(&bin);
    let event: WSEvent = from_reader(type_cursor).expect("unknown event");
    println!("event type received: {:?}", event);

    match event.event_type {
        WSEventType::NewPlayer => handle_new_player(id, event_cursor),
        WSEventType::GetPlayersInLobby => Ok(Some(get_players_in_lobby())),
        _ => Ok(None),
    }
}

fn handle_client(tcp_stream: TcpStream, tx: Sender<Vec<u8>>, rx: Receiver<Vec<u8>>) {
    CONNECTIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let id = Uuid::new_v4();

    println!("stream {}", tcp_stream.peer_addr().unwrap());
    tcp_stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let web_socket = Arc::new(Mutex::new(
        accept(tcp_stream).expect("couldnt accept client"),
    ));
    let channel_websocket = web_socket.clone();
    let close_thread: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    let ws_close = close_thread.clone();
    let channel_close = close_thread.clone();

    let channel_thread = spawn(move || loop {
        std::thread::sleep(Duration::from_millis(50));
        if channel_close.load(std::sync::atomic::Ordering::Acquire) {
            println!("true!!!");
            break;
        }
        if let Ok(data) = rx.try_recv() {
            println!("received channel data! {:?}", data);
            channel_websocket
                .lock()
                .unwrap()
                .send(data.into())
                .unwrap_or_else(|_| panic!("couldnt broadcast to websocket {}", id));
            channel_websocket.lock().unwrap().flush().unwrap();
        }
    });
    let ws_thread = spawn(move || loop {
        std::thread::sleep(Duration::from_millis(50));
        let ws_read = web_socket.lock().unwrap().read();
        match ws_read {
            Ok(Message::Binary(bin)) => {
                let response_option = handle_binary(id, bin).expect("couldnt handle binary");
                if let Some(response) = response_option {
                    println!("got a response, sending it to channel now!");
                    for _ in 0..CONNECTIONS.load(std::sync::atomic::Ordering::Acquire) {
                        tx.send(response.clone())
                            .expect("couldnt send response to crossbeam_channel");
                    }
                }
            }
            Ok(Message::Close(_frame)) => {
                {
                    let mut players = CLIENTS.lock().expect("couldnt get clients");
                    let position = players.iter().position(|player| player.id.eq(&id));
                    if let Some(pos_found) = position {
                        let _ = players.remove(pos_found);
                    };
                }
                ws_close.store(true, std::sync::atomic::Ordering::Release);

                let response = get_players_in_lobby();
                for _ in 0..CONNECTIONS.load(std::sync::atomic::Ordering::Acquire) {
                    tx.send(response.clone())
                        .expect("couldnt send response to crossbeam_channel");
                }
                break;
            }
            _ => {}
        }
    });

    channel_thread.join().expect("couldnt join channel thread");
    ws_thread.join().expect("couldnt join ws_thread");
    CONNECTIONS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    println!("ws_thread joined! {}", id);
    println!("channel_thread joined! {}", id);
}

fn main() {
    let server = TcpListener::bind("0.0.0.0:6942").expect("port probably already in use");
    let (tx, rx) = bounded::<Vec<u8>>(16);
    for stream in server.incoming() {
        let thread_tx = tx.clone();
        let thread_rx = rx.clone();
        spawn(move || match stream {
            Ok(stream) => {
                handle_client(stream, thread_tx, thread_rx);
            }
            Err(e) => println!("error {}", e),
        });
    }
    println!("Hello, world!");
}
