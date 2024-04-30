use crate::server::{middleware::Auth, Server};
use axum::{
    extract::{
        ws::{Message as AxumWsMessage, WebSocket as AxumWebSocket},
        State, WebSocketUpgrade,
    },
    http::{header::CONTENT_TYPE, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt, TryStreamExt,
};
use log::debug;
use ogg::reading::async_api::PacketReader;
use rubato::{FastFixedIn, PolynomialDegree, Resampler};
use std::{
    io::{Error as IoError, ErrorKind as IoErrorKind},
    mem::swap,
    sync::Arc,
};
use symphonia::{
    core::{
        audio::{AudioBuffer, AudioBufferRef, Signal},
        codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_VORBIS},
        formats::Packet as SymphoniaPacket,
    },
    default::codecs::VorbisDecoder,
};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest, Message as TungsteniteMessage, Result as TungsteniteResult,
    },
    MaybeTlsStream, WebSocketStream,
};

type TungsteniteWebsocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const VORBIS_CONTENT_TYPE: &str = "audio/ogg; codecs=vorbis";

/// Ring buffer capacity for keeping last audio segment.
const RING_BUFFER_CAPACITY_MSECS: u32 = 30_000;

/// Economical sample rate that is enough for speech recognition.
const SAMPLE_RATE: u32 = 16000;

/// Handle transcribe requests.
pub async fn handle_transcribe(
    State(server): State<Arc<Server>>,
    _auth: Auth,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    debug!("received transcribe request");

    if headers.get(CONTENT_TYPE) != Some(&HeaderValue::from_static(VORBIS_CONTENT_TYPE)) {
        debug!("rejected to transcribe due to unsupported content type");
        return (StatusCode::BAD_REQUEST, "unsupported content type").into_response();
    }

    let mut url = server.config.infsrv_url.clone();
    url.query_pairs_mut()
        .append_pair("nc", "1")
        .append_pair("sr", &SAMPLE_RATE.to_string())
        .append_pair("st", "i16");
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .append(CONTENT_TYPE, "audio/lpcm".try_into().unwrap());
    let infsrv_ws = match connect_async(request).await {
        Ok((wss, _)) => wss,
        Err(err) => {
            debug!("failed to connect to infsrv: {err:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to connect to inference server",
            )
                .into_response();
        }
    };

    ws.on_upgrade(move |client_ws| async { ws_callback(server, client_ws, infsrv_ws).await })
}

async fn ws_callback(
    server: Arc<Server>,
    client_ws: AxumWebSocket,
    infsrv_ws: TungsteniteWebsocket,
) {
    let (client_sender, client_receiver) = client_ws.split();
    let (infsrv_sender, infsrv_receiver) = infsrv_ws.split();
    let ring_buffer = Arc::new(RingBuffer::with_capacity(
        SAMPLE_RATE,
        RING_BUFFER_CAPACITY_MSECS,
    ));

    let cloned_server = server.clone();
    let cloned_ring_buffer = ring_buffer.clone();
    let segment_handle = tokio::spawn(async move {
        process_segments(
            cloned_server,
            client_sender,
            infsrv_receiver,
            cloned_ring_buffer,
        )
        .await;
    });

    let mut processor = AudioStreamProcessor::new();
    processor
        .process(server, client_receiver, infsrv_sender, ring_buffer.clone())
        .await;

    let _ = segment_handle.await;
    debug!("disconnected transcribe");
}

async fn process_segments(
    _server: Arc<Server>,
    mut _client_sender: SplitSink<AxumWebSocket, AxumWsMessage>,
    mut infsrv_receiver: SplitStream<TungsteniteWebsocket>,
    _ring_buffer: Arc<RingBuffer>,
) {
    while let Some(Ok(TungsteniteMessage::Text(json))) = infsrv_receiver.next().await {
        debug!("segment {}", json.trim());
    }
}

struct AudioStreamProcessor {
    resampler: Option<FastFixedIn<f32>>,
    merged: Vec<f32>,
    resampled: Vec<f32>,
}

impl AudioStreamProcessor {
    pub fn new() -> Self {
        Self {
            resampler: None,
            merged: Vec::new(),
            resampled: Vec::new(),
        }
    }

    pub async fn process(
        &mut self,
        _server: Arc<Server>,
        client_receiver: SplitStream<AxumWebSocket>,
        mut infsrv_sender: SplitSink<TungsteniteWebsocket, TungsteniteMessage>,
        ring_buffer: Arc<RingBuffer>,
    ) {
        let data_reader = Box::pin(client_receiver.into_stream().filter_map(|msg| async {
            match msg {
                Ok(AxumWsMessage::Binary(data)) => Some(Ok(data)),
                Ok(_) => None,
                Err(err) => Some(Err(IoError::new(IoErrorKind::Other, err))),
            }
        }))
        .into_async_read();
        let mut packet_reader = PacketReader::new_compat(data_reader);

        let mut id_header = Vec::new();
        let mut decoder = None;

        let mut packet_index = 0;
        while let Some(Ok(mut packet)) = packet_reader.next().await {
            match packet_index {
                0 => id_header = packet.data,
                1 => (), // Skip comment header.
                2 => {
                    let mut codec_params = CodecParameters::new();
                    codec_params.for_codec(CODEC_TYPE_VORBIS);
                    id_header.append(&mut packet.data);
                    swap(&mut id_header, &mut packet.data);
                    codec_params.with_extra_data(packet.data.into_boxed_slice());

                    let decoder_opts = DecoderOptions::default();

                    decoder = match VorbisDecoder::try_new(&codec_params, &decoder_opts) {
                        Ok(decoder) => Some(decoder),
                        Err(err) => {
                            debug!("failed to create vorbis decoder: {err:#}");
                            return;
                        }
                    };
                }
                _ => {
                    let packet = SymphoniaPacket::new_from_boxed_slice(
                        0,
                        0,
                        0,
                        packet.data.into_boxed_slice(),
                    );
                    let buf = match decoder.as_mut().unwrap().decode(&packet) {
                        Ok(buf) => buf,
                        Err(err) => {
                            debug!("failed to decode packet: {err:#}");
                            return;
                        }
                    };

                    let AudioBufferRef::F32(buf_f32) = buf else {
                        debug!("unsupported type of decoded samples");
                        return;
                    };
                    if let Err(err) = self
                        .process_audio_buffer(&mut infsrv_sender, &ring_buffer, buf_f32.as_ref())
                        .await
                    {
                        debug!("failed to process audio buffer: {err:#}");
                        return;
                    }
                }
            }
            packet_index += 1;
        }

        let _ = infsrv_sender.close().await;
    }

    async fn process_audio_buffer(
        &mut self,
        infsrv_sender: &mut SplitSink<TungsteniteWebsocket, TungsteniteMessage>,
        ring_buffer: &RingBuffer,
        audio_buffer: &AudioBuffer<f32>,
    ) -> TungsteniteResult<()> {
        self.merge_channels(audio_buffer);
        self.resample(audio_buffer.spec().rate);

        let mut data = Vec::with_capacity(2 * self.resampled.len());

        for f32 in &self.resampled {
            let i16 = (*f32 * i16::MAX as f32) as i16;
            data.extend_from_slice(&i16.to_le_bytes());
            ring_buffer.push(i16);
        }

        infsrv_sender.send(TungsteniteMessage::binary(data)).await
    }

    fn merge_channels(&mut self, audio_buffer: &AudioBuffer<f32>) {
        let offset = self.merged.len();

        self.merged.resize(offset + audio_buffer.frames(), 0.0);
        self.merged[offset..].fill(0.0);

        for i in 0..audio_buffer.spec().channels.count() {
            self.merged[offset..]
                .iter_mut()
                .zip(audio_buffer.chan(i).iter())
                .for_each(|(m, s)| *m += (*s - *m) / (i + 1) as f32);
        }
    }

    fn resample(&mut self, sample_rate: u32) {
        if sample_rate != SAMPLE_RATE {
            const CHUNK_SIZE: usize = 1024;
            if self.resampler.is_none() {
                self.resampler = Some(
                    FastFixedIn::<f32>::new(
                        SAMPLE_RATE as f64 / sample_rate as f64,
                        1.0,
                        PolynomialDegree::Linear,
                        CHUNK_SIZE,
                        1,
                    )
                    .unwrap(),
                );
            }

            const OUTPUT_MARGIN: usize = 10;
            let ratio = SAMPLE_RATE as f32 / sample_rate as f32;
            self.resampled.resize(
                (self.merged.len() as f32 * ratio) as usize + OUTPUT_MARGIN,
                0.0,
            );

            let mut merged_offset = 0;
            let mut resampled_offset = 0;

            while self.merged.len() - merged_offset >= CHUNK_SIZE {
                let (in_samples, out_samples) = self
                    .resampler
                    .as_mut()
                    .unwrap()
                    .process_into_buffer(
                        &[&self.merged[merged_offset..]],
                        &mut [&mut self.resampled[resampled_offset..]],
                        None,
                    )
                    .unwrap();

                merged_offset += in_samples;
                resampled_offset += out_samples;
            }

            self.merged.drain(..merged_offset);
            self.resampled.truncate(resampled_offset);
        } else {
            self.resampled.clear();
            swap(&mut self.merged, &mut self.resampled);
        }
    }
}

struct RingBuffer {}

impl RingBuffer {
    fn with_capacity(_sample_rate: u32, _capacity_msecs: u32) -> Self {
        Self {}
    }

    fn push(&self, _sample: i16) {}
}
