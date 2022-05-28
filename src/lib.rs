use std::collections::HashMap;
use std::io::Error as IoError;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    let (tx, rx) = unbounded();
    peer_map.lock().unwrap().insert(addr, tx);
    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {

        let event: serde_json::Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
        if event["event"] == "message" {

            let peers = peer_map.lock().unwrap();
            for (addr, ws_sink) in peers.iter() {
                println!("Sending to {}", addr);
                ws_sink.unbounded_send(msg.clone()).unwrap();
            }

        } else if event["event"] == "join" {

            let mut join_msg = HashMap::new();
            join_msg.insert("event", "message");
            join_msg.insert("sender", "Server");
            let join_string = format!("User <{}> joined", event["username"].as_str().unwrap());
            join_msg.insert("message", &join_string);
            let join_message = serde_json::to_string(&join_msg).unwrap();
            let peers = peer_map.lock().unwrap();
            for (addr, ws_sink) in peers.iter() {
                println!("Sending to {}", addr);
                ws_sink.unbounded_send(Message::from(join_message.clone())).unwrap();
            }

        }


        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.lock().unwrap().remove(&addr);
}

pub async fn host_room(name: String, addr: String) -> Result<(), IoError> {
    let name_b64 = base64::encode_config(name, base64::URL_SAFE);
    let url_b64 = base64::encode_config("ws://".to_string()+&addr, base64::URL_SAFE);
    let client = reqwest::Client::new();
    client.post(format!("http://127.0.0.1:8000/add_server/{}/{}", name_b64, url_b64))
        .send()
        .await
        .unwrap();

    let state = PeerMap::new(Mutex::new(HashMap::new()));

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(state.clone(), stream, addr));
    }

    Ok(())
}
