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

/// Max speech segment duration.
pub const MAX_SEGMENT_DURATION: f32 = 30.0;

/// Economical sample rate that is enough for speech recognition.
pub const SAMPLE_RATE: f32 = 16000.0;

const WINDOW_DURATION: f32 = 5.0;

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
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SegmentItem {
    Speech { begin: f32, end: f32 },
    Void { begin: f32, end: f32 },
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
            .append_pair("msd", &(MAX_SEGMENT_DURATION - WINDOW_DURATION).to_string())
            .append_pair("nc", "1")
            .append_pair("sr", &SAMPLE_RATE.to_string())
            .append_pair("st", "i16")
            .append_pair("wd", &WINDOW_DURATION.to_string());

        let mut request = url.into_client_request().unwrap();
        let headers = request.headers_mut();
        headers.append(
            CAPABILITIES_HEADER,
            capabilities.join(",").try_into().unwrap(),
        );
        headers.append(CONTENT_TYPE, "audio/lpcm".try_into().unwrap());

        let (ws, _) = connect_async(request).await?;
        let (mut ws_sender, mut ws_receiver) = ws.split();

        let (in_sender, in_receiver) = channel(1);
        let (out_sender, mut out_receiver) = channel(1);

        // Some WS clients (e.g. websocat) ignore close-messages, so we must
        // forcefully disconnect by closing ws_sender when in_receiver is destroyed.
        let in_sender_cloned = in_sender.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = in_sender_cloned.closed() => {
                        break;
                    }
                    maybe_pcm = out_receiver.recv() => {
                        let Some(pcm) = maybe_pcm else {
                            break;
                        };
                        if let Err(err) = ws_sender.send(Message::binary(pcm)).await {
                            debug!("failed to send to websocket: {err:#}");
                            break;
                        }
                    }
                }
            }
            ws_sender.close().await
        });

        use Error::*;
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

    /// Transcribe a given wav-blob.
    pub async fn transcribe(
        &self,
        _user: Uuid,
        _tariff: &str,
        wav_blob: Vec<u8>,
        language: Option<String>,
        prompt: Option<String>,
    ) -> Result<TranscribeItem> {
        // TODO: Allocate URL and capabilities dynamically instead of hardcoding.
        let url = Url::parse("http://127.0.0.1:8001/transcribe").unwrap();
        let capabilities = ["transcribe-small-cpu"];

        let mut form = Form::new()
            .part("file", Part::bytes(wav_blob).file_name("file.wav"))
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
