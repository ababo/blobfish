use crate::{data::capability::TaskType, ledger::Ledger, util::fmt::TruncateDebug};
use axum::http::{header::CONTENT_TYPE, StatusCode};
use futures::{SinkExt, StreamExt};
use log::{debug, error};
use reqwest::{
    multipart::{Form, Part},
    Client,
};
use serde::Deserialize;
use std::time::Duration;
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    time::interval,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};
use url::Url;
use uuid::Uuid;

/// Request capabilities header name.
const CAPABILITIES_HEADER: &str = "X-Blobfish-Capabilities";

/// Stream terminator header name.
pub const TERMINATOR_HEADER: &str = "X-Blobfish-Terminator";

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
    #[error("ledger: {0}")]
    Ledger(#[from] crate::ledger::Error),
    #[error("reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("serde_json: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("tungstanite: {0}")]
    Tungstanite(#[from] tokio_tungstenite::tungstenite::Error),
}

impl Error {
    /// HTTP status code.
    pub fn status(&self) -> StatusCode {
        use Error::*;
        match self {
            Internal | Reqwest(_) | SerdeJson(_) | Tungstanite(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Ledger(err) => err.status(),
        }
    }

    /// Kind code.
    pub fn code(&self) -> &str {
        use Error::*;
        match self {
            Internal => "internal",
            Ledger(err) => err.code(),
            Reqwest(_) => "reqwest",
            SerdeJson(_) => "serde_json",
            Tungstanite(_) => "tungstanite",
        }
    }
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
pub struct InfsrvPool {
    ledger: Ledger,
}

impl InfsrvPool {
    /// Create a new InfsrvPool instance.
    pub fn new(ledger: Ledger) -> Self {
        Self { ledger }
    }

    /// Initiate a speech segmentation session.
    /// Returns a sender for raw PCM data (i16 le-encoded samples, 16kHz mono)
    /// and a receiver to receive time intervals (in milliseconds).
    pub async fn segment(
        &self,
        user: Uuid,
        tariff: &str,
        terminator: Option<&[u8]>,
    ) -> Result<(Sender<Vec<u8>>, Receiver<Result<SegmentItem>>)> {
        let allocation = self
            .ledger
            .allocate(user, tariff, TaskType::Segment)
            .await?;

        let mut url = Url::parse("ws://127.0.0.1:9322/segment").unwrap();
        url.set_ip_host(allocation.ip_address()).unwrap();
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
            allocation.capabilities().join(",").try_into().unwrap(),
        );
        headers.append(CONTENT_TYPE, "audio/lpcm".try_into().unwrap());
        if let Some(delim) = terminator {
            headers.append(TERMINATOR_HEADER, delim.try_into().unwrap());
        }

        let (ws, _) = connect_async(request).await?;
        let (mut ws_sender, mut ws_receiver) = ws.split();

        let (sender, infsrv_receiver) = channel(32);
        let (infsrv_sender, mut receiver) = channel(32);

        tokio::spawn(async move {
            let mut closed_interval = interval(Duration::from_secs(5));
            closed_interval.tick().await;
            loop {
                tokio::select! {
                    maybe_pcm = receiver.recv() => {
                        let Some(pcm) = maybe_pcm else {
                            break;
                        };
                        if let Err(err) = ws_sender.send(Message::binary(pcm)).await {
                            debug!("failed to send pcm to infsrv ws: {err:#}");
                            break;
                        }
                    },
                    _ = closed_interval.tick() => {
                        match allocation.check_invalidated().await {
                            Ok(true) => {
                                debug!("detected allocation closed");
                                break;
                            }
                            Err(err) => {
                                error!("failed to check if allocation closed: {:#}", err);
                                break;
                            }
                            _ => {},
                        }
                    }
                }
            }

            let _ = ws_sender.close().await;
            debug!("finished sending pcm to infsrv ws");
        });

        use Error::*;
        tokio::spawn(async move {
            while let Some(result) = ws_receiver.next().await {
                match result {
                    Ok(Message::Text(json)) => {
                        let Ok(item) = serde_json::from_str::<'_, SegmentItem>(&json) else {
                            debug!("failed to parse infsrv segment json '{json}'");
                            let _ = sender.send(Err(Internal)).await;
                            break;
                        };
                        if sender.send(Ok(item)).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(maybe_reason)) => {
                        if let Some(reason) = maybe_reason {
                            debug!("received close msg (reason = {reason}) from infsrv ws");
                        } else {
                            debug!("received close msg from infsrv ws");
                        }
                        break;
                    }
                    Ok(msg) => {
                        debug!("ignoring infsrv ws msg {:?}", TruncateDebug::new(&msg));
                        continue;
                    }
                    Err(err) => {
                        debug!("failed to receive from infsrv ws: {err:#}");
                        let _ = sender.send(Err(Tungstanite(err))).await;
                        break;
                    }
                }
            }
            debug!("finished receiving segments from infsrv ws");
        });

        Ok((infsrv_sender, infsrv_receiver))
    }

    /// Transcribe a given wav-blob.
    pub async fn transcribe(
        &self,
        user: Uuid,
        tariff: &str,
        wav_blob: Vec<u8>,
        language: Option<String>,
        prompt: Option<String>,
    ) -> Result<TranscribeItem> {
        let allocation = self
            .ledger
            .allocate(user, tariff, TaskType::Transcribe)
            .await?;

        let mut form = Form::new().part("file", Part::bytes(wav_blob).file_name("file.wav"));

        if let Some(language) = language {
            form = form.text("language", language);
        }

        if let Some(prompt) = prompt {
            form = form.text("prompt", prompt);
        }

        let mut url = Url::parse("http://127.0.0.1:9322/transcribe").unwrap();
        url.set_ip_host(allocation.ip_address()).unwrap();
        let response = Client::default()
            .post(url)
            .header(CAPABILITIES_HEADER, allocation.capabilities().join(","))
            .multipart(form)
            .send()
            .await?;

        let text = response.text().await?;
        let item = serde_json::from_str(&text)?;
        Ok(item)
    }
}
