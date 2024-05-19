use crate::{
    infsrv_pool::{
        Result as InfsrvResult, SegmentItem, MAX_SEGMENT_DURATION, SAMPLE_RATE, TERMINATOR_HEADER,
    },
    server::{middleware::Auth, Error, Result, Server},
};
use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{header::CONTENT_TYPE, HeaderMap, HeaderValue},
    response::IntoResponse,
};
use futures::{
    channel::mpsc::channel,
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt, TryStreamExt,
};
use hound::{SampleFormat, WavSpec, WavWriter};
use log::{debug, error, info};
use ogg::reading::async_api::PacketReader;
use rubato::{FastFixedIn, PolynomialDegree, Resampler};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{Cursor, Error as IoError, ErrorKind as IoErrorKind},
    mem::swap,
    sync::{Arc, Mutex},
};
use symphonia::{
    core::{
        audio::{AudioBuffer, AudioBufferRef, Signal},
        codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_VORBIS},
        formats::Packet as SymphoniaPacket,
    },
    default::codecs::VorbisDecoder,
};
use tokio::{
    io::AsyncRead,
    sync::mpsc::{unbounded_channel, Receiver, Sender, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

const VORBIS_CONTENT_TYPE: &str = "audio/ogg; codecs=vorbis";

/// Ring buffer time capacity for keeping last audio segment.
const RING_BUFFER_CAPACITY: usize = (MAX_SEGMENT_DURATION * SAMPLE_RATE) as usize;

/// Transcribe request query.
#[derive(Deserialize)]
pub struct TranscribeQuery {
    pub tariff: String,
    pub lang: Option<String>,
}

/// Transcribe request output item.
#[derive(Deserialize, Serialize)]
pub struct TranscribeItem {
    pub begin: f32,
    pub end: f32,
    pub text: String,
}

/// Handle transcribe requests.
pub async fn handle_transcribe(
    State(server): State<Arc<Server>>,
    auth: Auth,
    Query(query): Query<TranscribeQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse> {
    info!("received transcribe request");

    if headers.get(CONTENT_TYPE) != Some(&HeaderValue::from_static(VORBIS_CONTENT_TYPE)) {
        return Err(Error::BadRequest("unsupported content type".to_owned()));
    }

    let terminator = headers.get(TERMINATOR_HEADER).map(|v| {
        debug!("stream terminator: {}", v.to_str().unwrap_or("?"));
        v.as_bytes().to_vec()
    });

    let (infsrv_sender, infsrv_receiver) = server
        .infsrv_pool
        .segment(auth.user, &query.tariff, terminator.as_deref())
        .await?;

    Ok(ws.on_upgrade(move |client_ws| async {
        ws_callback(
            server,
            auth,
            query,
            infsrv_sender,
            infsrv_receiver,
            client_ws,
            terminator,
        )
        .await
    }))
}

async fn ws_callback(
    server: Arc<Server>,
    auth: Auth,
    query: TranscribeQuery,
    infsrv_sender: Sender<Vec<u8>>,
    infsrv_receiver: Receiver<InfsrvResult<SegmentItem>>,
    client_ws: WebSocket,
    terminator: Option<Vec<u8>>,
) {
    let (client_sender, client_receiver) = client_ws.split();

    let ring_buffer = Arc::new(Mutex::new(RingBuffer::with_capacity(
        SAMPLE_RATE,
        RING_BUFFER_CAPACITY,
    )));

    let (limit_sender, limit_receiver) = unbounded_channel::<f32>();

    let cloned_server = server.clone();
    let cloned_ring_buffer = ring_buffer.clone();
    let segment_handle = tokio::spawn(async move {
        process_segments(
            cloned_server,
            auth,
            query,
            client_sender,
            infsrv_receiver,
            cloned_ring_buffer,
            limit_sender,
        )
        .await;
    });

    let mut processor = AudioStreamProcessor::new();
    processor
        .process(
            server,
            infsrv_sender,
            client_receiver,
            terminator,
            ring_buffer.clone(),
            limit_receiver,
        )
        .await;

    let _ = segment_handle.await;
    info!("disconnected transcribe");
}

async fn process_segments(
    server: Arc<Server>,
    auth: Auth,
    query: TranscribeQuery,
    mut client_sender: SplitSink<WebSocket, Message>,
    mut infsrv_receiver: Receiver<InfsrvResult<SegmentItem>>,
    ring_buffer: Arc<Mutex<RingBuffer>>,
    limit_sender: UnboundedSender<f32>,
) {
    let mut item = None;
    while let Some(Ok(segment_item)) = infsrv_receiver.recv().await {
        use SegmentItem::*;
        let (begin, end) = match segment_item {
            Speech { begin, end } => (begin, end),
            Void { begin, end } => {
                debug!("received void segment {}s-{}s", begin, end);
                if limit_sender.send(end).is_err() {
                    debug!("failed to send time consumed for void segment");
                    break;
                }
                continue;
            }
        };

        debug!("received speech segment {}s-{}s", begin, end);

        let wav_blob = ring_buffer
            .lock()
            .unwrap()
            .extract_time_interval_wav(begin, end);

        if limit_sender.send(end).is_err() {
            debug!("failed to send time consumed for speech segment");
            break;
        }

        let result = server
            .infsrv_pool
            .transcribe(
                auth.user,
                query.tariff.as_str(),
                wav_blob,
                query.lang.as_ref().cloned(),
                item.take().map(|s: TranscribeItem| s.text),
            )
            .await;

        let transcribe_item = match result {
            Ok(item) => item,
            Err(err) => {
                error!("failed to transcribe segment: {err:#}");
                break;
            }
        };

        item = Some(TranscribeItem {
            begin,
            end,
            text: transcribe_item.text,
        });
        let json = serde_json::to_string(&item).unwrap();
        if let Err(err) = client_sender.send(Message::Text(json + "\n")).await {
            debug!("failed to send to client ws: {err:#}");
            break;
        }
    }
    let _ = client_sender.close().await;
    debug!("finished processing infsrv segments");
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
        infsrv_sender: Sender<Vec<u8>>,
        client_receiver: SplitStream<WebSocket>,
        terminator: Option<Vec<u8>>,
        ring_buffer: Arc<Mutex<RingBuffer>>,
        mut limit_receiver: UnboundedReceiver<f32>,
    ) {
        let (mut packet_reader, join_handle) =
            Self::create_packet_reader(client_receiver, terminator.clone());

        let mut id_header = Vec::new();
        let mut decoder = None;
        let mut frames_consumed = 0;

        let mut packet_index = 0;
        loop {
            let mut packet = tokio::select! {
                 _ = infsrv_sender.closed() => {
                        debug!("closed infsrv pcm sender");
                        break;
                }
                result = packet_reader.next() => {
                    match result {
                        Some(Ok(packet)) => packet,
                        Some(Err(err)) => {
                            debug!("failed to read ogg packet: {err:#}");
                            break;
                        }
                        None => {
                            debug!("no more ogg packets");
                            break;
                        }
                    }
                }
            };

            let last = packet.last_in_stream();
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
                    if !self
                        .process_audio_buffer(
                            &infsrv_sender,
                            &ring_buffer,
                            &mut limit_receiver,
                            &mut frames_consumed,
                            buf_f32.as_ref(),
                            terminator.as_deref().filter(|_| last),
                        )
                        .await
                    {
                        break;
                    }
                }
            }
            packet_index += 1;
        }
        debug!("finished processing client audio stream");

        let mut client_receiver = join_handle.await.unwrap();
        while let Some(Ok(msg)) = client_receiver.next().await {
            debug!("ignoring client ws post-audio msg {msg:?}");
        }
        debug!("finished to read post-audio client ws");
    }

    fn create_packet_reader(
        mut client_receiver: SplitStream<WebSocket>,
        terminator: Option<Vec<u8>>,
    ) -> (
        PacketReader<impl AsyncRead>,
        JoinHandle<SplitStream<WebSocket>>,
    ) {
        let (mut sender, receiver) = channel(32);
        let join_handle = tokio::spawn(async move {
            while let Some(result) = client_receiver.next().await {
                match result {
                    Ok(Message::Binary(mut data)) => {
                        let mut last = false;
                        if let Some(delim) = terminator.as_deref() {
                            if data.ends_with(delim) {
                                data.truncate(data.len() - delim.len());
                                debug!("detected client audio stream terminator");
                                last = true;
                            }
                        }
                        if sender.send(Ok(data)).await.is_err() {
                            debug!("failed to send data to packet reader");
                            break;
                        }
                        if last {
                            break;
                        }
                    }
                    Ok(Message::Close(maybe_reason)) => {
                        if let Some(CloseFrame { code, reason }) = maybe_reason {
                            debug!("received close msg (code {code}, reason='{reason}') from client ws");
                        } else {
                            debug!("received close msg from client ws");
                        }
                        break;
                    }
                    Ok(msg) => {
                        debug!("ignoring client ws msg {msg:?}");
                    }
                    Err(err) => {
                        debug!("failed to read client ws: {err:#}");
                        let io_err = IoError::new(IoErrorKind::Other, err);
                        if sender.send(Err(io_err)).await.is_err() {
                            debug!("failed to send error to packet reader");
                            break;
                        }
                    }
                }
            }
            debug!("finished to feed ogg packet reader");
            client_receiver
        });
        (
            PacketReader::new_compat(receiver.into_async_read()),
            join_handle,
        )
    }

    async fn process_audio_buffer(
        &mut self,
        infsrv_sender: &Sender<Vec<u8>>,
        ring_buffer: &Mutex<RingBuffer>,
        limit_receiver: &mut UnboundedReceiver<f32>,
        frames_consumed: &mut usize,
        audio_buffer: &AudioBuffer<f32>,
        terminator: Option<&[u8]>,
    ) -> bool {
        self.merge_channels(audio_buffer);
        self.resample(audio_buffer.spec().rate as f32);

        let mut offset = 0;
        while offset < self.resampled.len() {
            let pushed = ring_buffer.lock().unwrap().pushed;
            let chunk_len = (RING_BUFFER_CAPACITY - (pushed - *frames_consumed))
                .min(self.resampled.len() - offset);

            if chunk_len == 0 {
                // Wait until more frames have been consumed before pushing.
                let Some(time_consumed) = limit_receiver.recv().await else {
                    debug!("failed to read from limit receiver");
                    return false;
                };
                *frames_consumed = (time_consumed * SAMPLE_RATE) as usize;
                continue;
            }

            let mut pcm = Vec::with_capacity(2 * self.resampled.len());
            {
                let mut guard = ring_buffer.lock().unwrap();
                for f32_sample in &self.resampled[offset..offset + chunk_len] {
                    let i16_sample = (*f32_sample * i16::MAX as f32) as i16;
                    pcm.extend_from_slice(&i16_sample.to_le_bytes());
                    guard.push(i16_sample);
                }
            }
            if let Err(err) = infsrv_sender.send(pcm).await {
                debug!("failed to send pcm to infsrv ws: {err:#}");
                return false;
            }
            offset += chunk_len;
        }

        if let Some(delim) = terminator {
            if let Err(err) = infsrv_sender.send(delim.to_owned()).await {
                debug!("failed to send terminator to infsrv ws: {err:#}");
                return false;
            }
        }

        true
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

    fn resample(&mut self, sample_rate: f32) {
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
            let ratio = SAMPLE_RATE / sample_rate;
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

struct RingBuffer {
    sample_rate: f32,
    deque: VecDeque<i16>,
    pushed: usize,
}

impl RingBuffer {
    fn with_capacity(sample_rate: f32, capacity: usize) -> Self {
        Self {
            sample_rate,
            deque: VecDeque::with_capacity(capacity),
            pushed: 0,
        }
    }

    #[inline]
    fn push(&mut self, sample: i16) {
        if self.deque.len() == self.deque.capacity() {
            self.deque.pop_front();
        }
        self.deque.push_back(sample);
        self.pushed += 1;
    }

    fn extract_time_interval_wav(&self, begin: f32, end: f32) -> Vec<u8> {
        let frame_offset = self.pushed - self.deque.len();
        let get_index = |time| {
            (((time * self.sample_rate) as usize).max(frame_offset) - frame_offset)
                .min(self.deque.len() - 1)
        };

        const WAV_HEADER_SIZE: usize = 44;
        let (begin_index, end_index) = (get_index(begin), get_index(end));
        let capacity = WAV_HEADER_SIZE + (end_index - begin_index) * 2;
        let mut data = Vec::with_capacity(capacity);

        let spec = WavSpec {
            channels: 1,
            sample_rate: self.sample_rate as u32,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::new(Cursor::new(&mut data), spec).unwrap();

        for i in begin_index..end_index {
            writer.write_sample(self.deque[i]).unwrap();
        }

        writer.finalize().unwrap();
        assert_eq!(data.len(), capacity);
        data
    }
}
