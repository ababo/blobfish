use crate::server::{middleware::Auth, Server};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::{header::CONTENT_TYPE, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use futures::{stream::SplitSink, StreamExt, TryStreamExt};
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

const VORBIS_CONTENT_TYPE: &str = "audio/ogg; codecs=vorbis";

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

    ws.on_upgrade(move |s| ws_callback(server, s))
}

async fn ws_callback(server: Arc<Server>, socket: WebSocket) {
    let (sender, receiver) = socket.split();

    let data_reader = Box::pin(receiver.into_stream().filter_map(|msg| async {
        match msg {
            Ok(Message::Binary(data)) => Some(Ok(data)),
            Ok(_) => None,
            Err(err) => Some(Err(IoError::new(IoErrorKind::Other, err))),
        }
    }))
    .into_async_read();
    let mut packet_reader = PacketReader::new_compat(data_reader);

    let mut id_header = Vec::new();
    let mut decoder = None;

    let mut context = Context::new(sender);

    let mut packet_index = 0;
    while let Some(Ok(mut packet)) = packet_reader.next().await {
        match packet_index {
            0 => id_header = packet.data,
            1 => (), // skip comment header
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
                let packet =
                    SymphoniaPacket::new_from_boxed_slice(0, 0, 0, packet.data.into_boxed_slice());
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
                process_audio_buffer(server.as_ref(), &mut context, buf_f32.into_owned()).await;
            }
        }
        packet_index += 1;
    }
}

struct Context {
    _sender: SplitSink<WebSocket, Message>,
    merged: Vec<f32>,
    resampled: Vec<f32>,
    resampler: Option<FastFixedIn<f32>>,
}

impl Context {
    fn new(_sender: SplitSink<WebSocket, Message>) -> Self {
        Self {
            _sender,
            merged: Vec::new(),
            resampled: Vec::new(),
            resampler: None,
        }
    }
}

async fn process_audio_buffer(_server: &Server, context: &mut Context, buf: AudioBuffer<f32>) {
    normalize_audio_buffer(context, buf);
}

fn normalize_audio_buffer(context: &mut Context, buf: AudioBuffer<f32>) {
    let offset = context.merged.len();

    // Merge the channels into one (mono).
    context.merged.resize(offset + buf.frames(), 0.0);
    context.merged[offset..].fill(0.0);
    for i in 0..buf.spec().channels.count() {
        context.merged[offset..]
            .iter_mut()
            .zip(buf.chan(i).iter())
            .for_each(|(m, s)| *m += (*s - *m) / (i + 1) as f32);
    }

    // Resample into SAMPLE_RATE.
    if buf.spec().rate != SAMPLE_RATE {
        const CHUNK_SIZE: usize = 1024;
        if context.resampler.is_none() {
            context.resampler = Some(
                FastFixedIn::<f32>::new(
                    SAMPLE_RATE as f64 / buf.spec().rate as f64,
                    1.0,
                    PolynomialDegree::Linear,
                    CHUNK_SIZE,
                    1,
                )
                .unwrap(),
            );
        }

        if context.merged.len() < CHUNK_SIZE {
            context.resampled.clear();
            return;
        }

        const OUTPUT_MARGIN: usize = 10;
        let ratio = SAMPLE_RATE as f32 / buf.spec().rate as f32;
        context.resampled.resize(
            (context.merged.len() as f32 * ratio) as usize + OUTPUT_MARGIN,
            0.0,
        );

        let (in_samples, out_samples) = context
            .resampler
            .as_mut()
            .unwrap()
            .process_into_buffer(
                &[context.merged.as_slice()],
                &mut [context.resampled.as_mut_slice()],
                None,
            )
            .unwrap();

        context.merged.drain(..in_samples);
        context.resampled.truncate(out_samples);
    } else {
        context.resampled.clear();
        swap(&mut context.merged, &mut context.resampled);
    }
}
