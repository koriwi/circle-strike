use std::io::Cursor;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::{mpsc, Arc};
use std::thread::spawn;
use std::time::Duration;
use tracing_mutex::stdsync::{Mutex, RwLock};

use ciborium::{from_reader, into_writer};

use tungstenite::error::ProtocolError;
use uuid::Uuid;

use tungstenite::{accept, Bytes, Error, Message};

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

fn handle_client(
    tcp_stream: TcpStream,
    connections: Arc<RwLock<Vec<Connection<Vec<u8>>>>>,
    receiver: mpsc::Receiver<Vec<u8>>,
) {
    let id = Uuid::new_v4();

    println!("stream {}", tcp_stream.peer_addr().unwrap());
    tcp_stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let web_socket = Arc::new(Mutex::new(
        accept(tcp_stream).expect("couldnt accept client"),
    ));
    let channel_websocket = web_socket.clone();

    let ws_thread = spawn(move || loop {
        std::thread::sleep(Duration::from_millis(50));
        let ws_read = web_socket.lock().unwrap().read();
        match ws_read {
            Ok(Message::Binary(bin)) => {
                let response_option = handle_binary(id, bin).expect("couldnt handle binary");
                if let Some(response) = response_option {
                    let connections = connections.read().unwrap();
                    connections.iter().for_each(|con| {
                        con.sender
                            .send(response.clone())
                            .expect("couldnt send response to channel")
                    });
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

                let response = get_players_in_lobby();
                let connections = connections.read().unwrap();
                connections.iter().for_each(|con| {
                    con.sender
                        .send(response.clone())
                        .expect("couldnt send response to channel")
                });
                break;
            }
            _ => {}
        }
    });

    let channel_thread = spawn(move || loop {
        std::thread::sleep(Duration::from_millis(50));
        if let Ok(data) = receiver.recv_timeout(Duration::from_millis(20)) {
            let mut cwl = channel_websocket.lock().unwrap();
            match cwl.send(data.into()) {
                Err(Error::ConnectionClosed) => break,
                Err(Error::Protocol(ProtocolError::SendAfterClosing)) => break,
                Err(e) => panic!("socket broken {} {:?}", id, e),
                _ => cwl.flush().unwrap(),
            }
        }
    });
    channel_thread.join().expect("couldnt join channel thread");
    ws_thread.join().expect("couldnt join ws_thread");
}

#[derive(Debug)]
struct Connection<T> {
    address: SocketAddr,
    sender: mpsc::SyncSender<T>,
}

fn main() {
    let server = TcpListener::bind("0.0.0.0:6942").expect("port probably already in use");
    let senders: Arc<RwLock<Vec<Connection<Vec<u8>>>>> = Arc::new(RwLock::new(Vec::new()));
    for stream in server.incoming() {
        let address = stream.as_ref().unwrap().peer_addr().unwrap();
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(16);
        let stream_senders = senders.clone();
        stream_senders
            .write()
            .unwrap()
            .push(Connection { address, sender });
        spawn(move || {
            let remove_senders = stream_senders.clone();
            match stream {
                Ok(stream) => {
                    handle_client(stream, stream_senders, receiver);
                }
                Err(e) => println!("error {}", e),
            };

            let pos = remove_senders
                .read()
                .unwrap()
                .iter()
                .position(|con| con.address.eq(&address))
                .unwrap();
            remove_senders.write().unwrap().swap_remove(pos);
        });
    }
}
