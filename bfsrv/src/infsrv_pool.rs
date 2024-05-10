use axum::http::header::CONTENT_TYPE;
use futures::{SinkExt, StreamExt};
use log::debug;
use reqwest::{
    multipart::{Form, Part},
    Client,
};
use serde::Deserialize;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};

use url::Url;
use uuid::Uuid;

/// Request capabilities header name.
const CAPABILITIES_HEADER: &str = "X-Blobfish-Capabilities";

/// Economical sample rate that is enough for speech recognition.
pub const SAMPLE_RATE: u32 = 16000;

/// InfsrvPool error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("internal")]
    Internal,
    #[error("reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("serde_json: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("tungstanite: {0}")]
    Tungstanite(#[from] tokio_tungstenite::tungstenite::Error),
}

/// InfsrvPool result.
pub type Result<T> = std::result::Result<T, Error>;

/// An item returned from speech segmentation stream.
#[derive(Deserialize)]
pub struct SegmentItem {
    pub begin: u32, // In milliseconds.
    pub end: u32,   // In milliseconds.
}

/// An item returned from speech transcription.
#[derive(Deserialize)]
pub struct TranscribeItem {
    pub text: String,
}

/// Pool of infsrv instances.
pub struct InfsrvPool {}

impl InfsrvPool {
    /// Create a new InfsrvPool instance.
    pub fn new() -> Self {
        Self {}
    }

    /// Initiate a speech segmentation session.
    /// Returns a sender for raw PCM data (i16 le-encoded samples, 16kHz mono)
    /// and a receiver to receive time intervals (in milliseconds).
    pub async fn segment(
        &self,
        _user: Uuid,
    ) -> Result<(Sender<Vec<u8>>, Receiver<Result<SegmentItem>>)> {
        // TODO: Allocate URL and capabilities dynamically instead of hardcoding.
        let mut url = Url::parse("ws://127.0.0.1:8001/segment").unwrap();
        let capabilities = ["segment-cpu"];

        url.query_pairs_mut()
            .append_pair("nc", "1")
            .append_pair("sr", &SAMPLE_RATE.to_string())
            .append_pair("st", "i16");

        let mut request = url.into_client_request().unwrap();
        let headers = request.headers_mut();
        headers.append(
            CAPABILITIES_HEADER,
            capabilities.join(",").try_into().unwrap(),
        );
        headers.append(CONTENT_TYPE, "audio/lpcm".try_into().unwrap());

        let (ws, _) = connect_async(request).await?;
        let (mut ws_sender, mut ws_receiver) = ws.split();

        let (out_sender, mut out_receiver) = channel(1);
        tokio::spawn(async move {
            while let Some(pcm) = out_receiver.recv().await {
                if let Err(err) = ws_sender.send(Message::binary(pcm)).await {
                    debug!("failed to send to websocket: {err:#}");
                    break;
                }
            }
            ws_sender.close().await
        });

        use Error::*;
        let (in_sender, in_receiver) = channel(1);
        tokio::spawn(async move {
            while let Some(result) = ws_receiver.next().await {
                match result {
                    Ok(Message::Text(json)) => {
                        let Ok(item) = serde_json::from_str::<'_, SegmentItem>(&json) else {
                            debug!("failed to parse infsrv segment json '{json}'");
                            let _ = in_sender.send(Err(Internal)).await;
                            break;
                        };
                        if in_sender.send(Ok(item)).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(reason)) => {
                        debug!("received close msg (reason = {reason:?}) from infsrv ws");
                        break;
                    }
                    Ok(msg) => {
                        debug!("ignoring infsrv ws {msg:?}");
                        continue;
                    }
                    Err(err) => {
                        debug!("failed to receive from infsrv ws: {err:#}");
                        let _ = in_sender.send(Err(Tungstanite(err))).await;
                        break;
                    }
                }
            }
        });

        Ok((out_sender, in_receiver))
    }

    /// Transcribe a given audio blob.
    pub async fn transcribe(
        &self,
        _user: Uuid,
        _tariff: &str,
        file: Vec<u8>,
        language: Option<String>,
        prompt: Option<String>,
    ) -> Result<TranscribeItem> {
        // TODO: Allocate URL and capabilities dynamically instead of hardcoding.
        let url = Url::parse("http://127.0.0.1:8001/transcribe").unwrap();
        let capabilities = ["transcribe-small-cpu"];

        let mut form = Form::new()
            .part("file", Part::bytes(file).file_name("a.wav"))
            .text("temperature", "0");

        if let Some(language) = language {
            form = form.text("language", language);
        }

        if let Some(prompt) = prompt {
            form = form.text("prompt", prompt);
        }

        let response = Client::default()
            .post(url)
            .header(CAPABILITIES_HEADER, capabilities.join(","))
            .multipart(form)
            .send()
            .await?;

        let text = response.text().await?;
        let item = serde_json::from_str(&text)?;
        Ok(item)
    }
}
