use std::env;
use chatroom;

#[tokio::main]
async fn main() {
    let name = env::args().nth(1).unwrap();
    let addr = env::args().nth(2).unwrap();
    chatroom::host_room(name, addr, "").await.unwrap();
}
