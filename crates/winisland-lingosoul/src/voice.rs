use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, Sample, SampleFormat, SizedSample};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::{speech, wake};

const RETRY_DELAY: Duration = Duration::from_secs(1);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) enum Control {
    Listen(Duration, u64),
    Stop(u64),
}

pub(crate) enum Event {
    Ready,
    Reconnecting(String),
    Wake,
    Level(f32),
    Transcribing,
    Text(String),
    Silence,
    Failed { error: String, model: bool },
    Warning(String),
}

pub(crate) struct Message {
    pub generation: Option<u64>,
    pub event: Event,
}

struct Events {
    sender: UnboundedSender<Message>,
    generation: Arc<AtomicU64>,
}

impl Events {
    fn send(&self, event: Event) {
        let _ = self.sender.send(Message {
            generation: None,
            event,
        });
    }

    fn send_at(&self, generation: u64, event: Event) {
        if self.generation.load(Ordering::Acquire) == generation {
            let _ = self.sender.send(Message {
                generation: Some(generation),
                event,
            });
        }
    }
}

pub(crate) struct Voice {
    control: mpsc::Sender<Control>,
    stop: CancellationToken,
    worker: Option<std::thread::JoinHandle<()>>,
    generation: Arc<AtomicU64>,
}

impl Voice {
    pub fn start(
        models: PathBuf,
        words: Vec<String>,
        events: UnboundedSender<Message>,
    ) -> Result<Self, String> {
        let (control, receiver) = mpsc::channel();
        let stop = CancellationToken::new();
        let cancelled = stop.clone();
        let generation = Arc::new(AtomicU64::new(0));
        let events = Events {
            sender: events,
            generation: Arc::clone(&generation),
        };
        let worker = std::thread::Builder::new()
            .name("lingosoul-voice".into())
            .spawn(move || {
                if let Err((error, model)) = run(models, words, &events, receiver, &cancelled)
                    && !cancelled.is_cancelled()
                {
                    log::error!("LingoSoul voice worker stopped: {error}");
                    events.send(Event::Failed { error, model });
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            control,
            stop,
            worker: Some(worker),
            generation,
        })
    }

    pub fn listen(&self, duration: Duration) {
        let generation = self
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let _ = self.control.send(Control::Listen(duration, generation));
    }
    pub fn stop_listening(&self) {
        let generation = self
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let _ = self.control.send(Control::Stop(generation));
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub async fn shutdown(mut self) {
        self.stop.cancel();
        if let Some(worker) = self.worker.take() {
            match tokio::task::spawn_blocking(move || worker.join()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => log::warn!("LingoSoul voice worker panicked"),
                Err(error) => log::warn!("Cannot join the LingoSoul voice worker: {error}"),
            }
        }
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

fn run(
    models: PathBuf,
    words: Vec<String>,
    events: &Events,
    control: mpsc::Receiver<Control>,
    cancel: &CancellationToken,
) -> Result<(), (String, bool)> {
    let (keywords, warnings) = wake::keywords(&words, &models.join(speech::KWS_DIR));
    let warning = warnings.join("; ");
    let mut ear = None;
    let mut delay = RETRY_DELAY;
    while !cancel.is_cancelled() {
        let started = Instant::now();
        let error = match capture(
            &models, &keywords, &warning, &mut ear, events, &control, cancel,
        ) {
            Ok(()) => return Ok(()),
            Err(_) if cancel.is_cancelled() => return Ok(()),
            Err(CaptureError::Model(error)) => return Err((error, true)),
            Err(CaptureError::Audio(error)) => error,
        };
        if !matches!(
            error.kind(),
            ErrorKind::StreamInvalidated
                | ErrorKind::DeviceNotAvailable
                | ErrorKind::DeviceBusy
                | ErrorKind::HostUnavailable
                | ErrorKind::ResourceExhausted
        ) {
            return Err((error.to_string(), false));
        }
        if started.elapsed() >= MAX_RETRY_DELAY {
            delay = RETRY_DELAY;
        }
        events.generation.fetch_add(1, Ordering::AcqRel);
        events.send(Event::Reconnecting(error.to_string()));
        log::warn!(
            "LingoSoul microphone interrupted; rebuilding in {}s: {error}",
            delay.as_secs()
        );
        let retry_at = Instant::now() + delay;
        while !cancel.is_cancelled() {
            let remaining = retry_at.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            std::thread::sleep(remaining.min(POLL_INTERVAL));
        }
        delay = (delay * 2).min(MAX_RETRY_DELAY);
    }
    Ok(())
}

enum CaptureError {
    Audio(cpal::Error),
    Model(String),
}

impl From<cpal::Error> for CaptureError {
    fn from(error: cpal::Error) -> Self {
        Self::Audio(error)
    }
}

fn capture(
    models: &Path,
    keywords: &str,
    warning: &str,
    cached: &mut Option<(i32, speech::Ear)>,
    events: &Events,
    control: &mpsc::Receiver<Control>,
    cancel: &CancellationToken,
) -> Result<(), CaptureError> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or_else(|| cpal::Error::new(ErrorKind::DeviceNotAvailable))?;
    let config = device.default_input_config()?;
    let rate = i32::try_from(config.sample_rate()).map_err(|error| {
        cpal::Error::with_message(ErrorKind::UnsupportedConfig, error.to_string())
    })?;
    let (_, ear) = match cached {
        Some(loaded) if loaded.0 == rate => loaded,
        cached => {
            *cached = None;
            cached.insert((
                rate,
                speech::Ear::new(models, keywords, rate).map_err(CaptureError::Model)?,
            ))
        }
    };
    ear.stop();
    if cancel.is_cancelled() {
        return Ok(());
    }
    let (samples, receiver) = mpsc::sync_channel(128);
    let (errors, failures) = mpsc::sync_channel(1);
    let stream = match config.sample_format() {
        SampleFormat::F32 => input::<f32>(&device, &config.into(), samples, errors),
        SampleFormat::I16 => input::<i16>(&device, &config.into(), samples, errors),
        SampleFormat::U16 => input::<u16>(&device, &config.into(), samples, errors),
        SampleFormat::I32 => input::<i32>(&device, &config.into(), samples, errors),
        SampleFormat::U32 => input::<u32>(&device, &config.into(), samples, errors),
        SampleFormat::F64 => input::<f64>(&device, &config.into(), samples, errors),
        _ => Err(cpal::Error::with_message(
            ErrorKind::UnsupportedConfig,
            "Unsupported microphone sample format",
        )),
    }?;
    stream.play()?;
    let mut ready = false;
    let mut last_level = Instant::now();
    let mut generation = events.generation.load(Ordering::Acquire);
    while !cancel.is_cancelled() {
        if let Ok(error) = failures.try_recv() {
            return Err(error.into());
        }
        while let Ok(command) = control.try_recv() {
            while receiver.try_recv().is_ok() {}
            match command {
                Control::Listen(duration, next)
                    if next == events.generation.load(Ordering::Acquire) =>
                {
                    generation = next;
                    ear.follow_up(duration);
                }
                Control::Stop(next) if next == events.generation.load(Ordering::Acquire) => {
                    generation = next;
                    ear.stop();
                }
                Control::Listen(_, _) | Control::Stop(_) => {}
            }
        }
        let chunk = match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(chunk) => chunk,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(failures
                    .try_recv()
                    .unwrap_or_else(|_| {
                        cpal::Error::with_message(
                            ErrorKind::StreamInvalidated,
                            "Microphone stream stopped",
                        )
                    })
                    .into());
            }
        };
        if let Ok(error) = failures.try_recv() {
            return Err(error.into());
        }
        if chunk.is_empty() {
            continue;
        }
        if !ready {
            ready = true;
            events.send(Event::Ready);
            if !warning.is_empty() {
                events.send(Event::Warning(warning.to_owned()));
            }
            log::info!("LingoSoul microphone capture is ready at {rate} Hz");
        }
        if events.generation.load(Ordering::Acquire) != generation {
            continue;
        }
        let event = ear.feed(&chunk);
        if ear.is_listening() && last_level.elapsed() >= Duration::from_millis(30) {
            last_level = Instant::now();
            let rms = (chunk.iter().map(|sample| sample * sample).sum::<f32>()
                / chunk.len().max(1) as f32)
                .sqrt();
            let level = ((20.0 * rms.max(1e-6).log10() + 52.0) / 36.0).clamp(0.0, 1.0);
            events.send_at(generation, Event::Level(level));
        }
        match event {
            Some(speech::Event::Wake) => {
                events.send_at(generation, Event::Wake);
            }
            Some(speech::Event::Utterance(Some(samples))) => {
                events.send_at(generation, Event::Transcribing);
                let text = ear.transcribe(&samples);
                if cancel.is_cancelled() {
                    break;
                }
                events.send_at(generation, Event::Text(text));
                while receiver.try_recv().is_ok() {}
            }
            Some(speech::Event::Utterance(None)) => {
                events.send_at(generation, Event::Silence);
            }
            None => {}
        }
    }
    drop(stream);
    Ok(())
}

fn input<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: mpsc::SyncSender<Vec<f32>>,
    errors: mpsc::SyncSender<cpal::Error>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: Sample + SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels);
    if channels == 0 {
        return Err(cpal::Error::with_message(
            ErrorKind::UnsupportedConfig,
            "The microphone has no input channels",
        ));
    }
    let mut xrun_reported = false;
    device.build_input_stream(
        *config,
        move |data: &[T], _| {
            let mono = data
                .chunks_exact(channels)
                .map(|frame| {
                    frame
                        .iter()
                        .map(|sample| sample.to_sample::<f32>())
                        .sum::<f32>()
                        / channels as f32
                })
                .collect();
            let _ = samples.try_send(mono);
        },
        move |error| match error.kind() {
            ErrorKind::Xrun => {
                if !xrun_reported {
                    log::warn!(
                        "LingoSoul microphone buffer underrun or overrun; continuing capture"
                    );
                    xrun_reported = true;
                }
            }
            ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => {
                log::warn!("LingoSoul microphone notice; continuing capture: {error}");
            }
            _ => {
                let _ = errors.try_send(error);
            }
        },
        None,
    )
}
