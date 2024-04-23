use crate::server::{middleware::Auth, Server};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use log::info;
use std::sync::Arc;
use tokio::sync::mpsc::channel;

const CHANNEL_BUFFER_SIZE: usize = 1;

pub async fn transcribe_handler(
    State(server): State<Arc<Server>>,
    Auth {}: Auth,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |s| ws_jsonrpc_callback(server, s))
}

async fn ws_jsonrpc_callback(_server: Arc<Server>, socket: WebSocket) {
    let (mut socket_sender, mut socket_receiver) = socket.split();

    let (channel_sender, mut channel_receiver) = channel::<String>(CHANNEL_BUFFER_SIZE);
    tokio::spawn(async move {
        while let Some(response) = channel_receiver.recv().await {
            socket_sender.send(Message::Text(response)).await.unwrap();
        }
    });

    while let Some(Ok(msg)) = socket_receiver.next().await {
        let Message::Text(msg) = msg else {
            continue;
        };
        if let Err(err) = channel_sender.send(msg).await {
            info!("failed to send: {err:#}")
        }
    }
}
