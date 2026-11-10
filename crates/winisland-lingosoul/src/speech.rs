//! Offline speech front end: 16 kHz resampling, wake word spotting, silero VAD
//! end-pointing and SenseVoice recognition of the captured utterance.

use std::path::Path;
use std::time::{Duration, Instant};

use sherpa_onnx::{
    KeywordSpotter, KeywordSpotterConfig, LinearResampler, OfflineRecognizer,
    OfflineRecognizerConfig, OnlineStream, VadModelConfig, VoiceActivityDetector,
};

pub const SAMPLE_RATE: i32 = 16_000;
pub const KWS_DIR: &str = "sherpa-onnx-kws-zipformer-zh-en-3M-2025-12-20";
pub const ASR_DIR: &str = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09";
pub const VAD_MODEL: &str = "silero_vad.onnx";

/// Waiting this long for the first word after the wake word cancels the request.
const START_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_UTTERANCE: Duration = Duration::from_secs(15);

fn path(dir: &Path, name: &str) -> Option<String> {
    Some(dir.join(name).to_string_lossy().into_owned())
}

fn spotter(models: &Path, keywords: &str) -> Result<KeywordSpotter, String> {
    let dir = models.join(KWS_DIR);
    let mut config = KeywordSpotterConfig::default();
    let m = &mut config.model_config;
    m.transducer.encoder = path(&dir, "encoder-epoch-13-avg-2-chunk-16-left-64.int8.onnx");
    m.transducer.decoder = path(&dir, "decoder-epoch-13-avg-2-chunk-16-left-64.onnx");
    m.transducer.joiner = path(&dir, "joiner-epoch-13-avg-2-chunk-16-left-64.int8.onnx");
    m.tokens = path(&dir, "tokens.txt");
    m.num_threads = 1;
    m.provider = Some("cpu".into());
    config.keywords_buf = Some(keywords.to_owned());
    config.keywords_threshold = 0.2;
    config.keywords_score = 1.5;
    KeywordSpotter::create(&config).ok_or_else(|| "Cannot load the wake-word model".into())
}

fn recognizer(models: &Path) -> Result<OfflineRecognizer, String> {
    let dir = models.join(ASR_DIR);
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.sense_voice.model = path(&dir, "model.int8.onnx");
    config.model_config.sense_voice.language = Some("auto".into());
    config.model_config.sense_voice.use_itn = true;
    config.model_config.tokens = path(&dir, "tokens.txt");
    config.model_config.num_threads = 2;
    config.model_config.provider = Some("cpu".into());
    OfflineRecognizer::create(&config)
        .ok_or_else(|| "Cannot load the speech-recognition model".into())
}

fn vad(models: &Path) -> Result<VoiceActivityDetector, String> {
    let mut config = VadModelConfig::default();
    config.silero_vad.model = path(models, VAD_MODEL);
    config.silero_vad.threshold = 0.5;
    config.silero_vad.min_silence_duration = 0.6;
    config.silero_vad.min_speech_duration = 0.2;
    config.silero_vad.max_speech_duration = MAX_UTTERANCE.as_secs_f32();
    config.silero_vad.window_size = 512;
    config.sample_rate = SAMPLE_RATE;
    config.num_threads = 1;
    config.provider = Some("cpu".into());
    VoiceActivityDetector::create(&config, 30.0)
        .ok_or_else(|| "Cannot load the voice-activity model".into())
}

pub enum Event {
    Wake,
    /// Listening is over; `None` means nobody spoke.
    Utterance(Option<Vec<f32>>),
}

enum Phase {
    Idle,
    Listening {
        since: Instant,
        heard: bool,
        wait: Duration,
    },
}

pub struct Ear {
    resampler: Option<LinearResampler>,
    kws: Option<(KeywordSpotter, OnlineStream)>,
    vad: VoiceActivityDetector,
    asr: OfflineRecognizer,
    phase: Phase,
    speech: Vec<f32>,
}

impl Ear {
    pub fn new(models: &Path, keywords: &str, input_rate: i32) -> Result<Self, String> {
        if input_rate <= 0 {
            return Err("Invalid microphone sample rate".into());
        }
        let kws = if keywords.trim().is_empty() {
            None
        } else {
            let kws = spotter(models, keywords)?;
            let stream = kws.create_stream();
            Some((kws, stream))
        };
        let resampler = if input_rate == SAMPLE_RATE {
            None
        } else {
            Some(
                LinearResampler::create(input_rate, SAMPLE_RATE)
                    .ok_or("Cannot initialize the audio resampler")?,
            )
        };
        Ok(Self {
            resampler,
            kws,
            vad: vad(models)?,
            asr: recognizer(models)?,
            phase: Phase::Idle,
            speech: Vec::new(),
        })
    }

    pub fn is_listening(&self) -> bool {
        matches!(self.phase, Phase::Listening { .. })
    }

    /// Keeps listening without a wake word, for a follow-up question; gives up
    /// after `wait` of silence.
    pub fn follow_up(&mut self, wait: Duration) {
        self.vad.reset();
        self.speech.clear();
        self.phase = Phase::Listening {
            since: Instant::now(),
            heard: false,
            wait,
        };
    }

    /// Stops listening and waits for the wake word again.
    pub fn stop(&mut self) {
        self.vad.reset();
        self.speech.clear();
        self.phase = Phase::Idle;
        if let Some((kws, stream)) = &self.kws {
            kws.reset(stream);
        }
    }

    /// Feeds microphone samples at the input rate; returns at most one event.
    pub fn feed(&mut self, samples: &[f32]) -> Option<Event> {
        let resampled;
        let samples = match &self.resampler {
            Some(r) => {
                resampled = r.resample(samples, false);
                &resampled[..]
            }
            None => samples,
        };
        match self.phase {
            Phase::Idle => {
                let (kws, stream) = self.kws.as_ref()?;
                stream.accept_waveform(SAMPLE_RATE, samples);
                while kws.is_ready(stream) {
                    kws.decode(stream);
                    if let Some(r) = kws.get_result(stream)
                        && !r.keyword.is_empty()
                    {
                        kws.reset(stream);
                        self.vad.reset();
                        self.speech.clear();
                        self.phase = Phase::Listening {
                            since: Instant::now(),
                            heard: false,
                            wait: START_TIMEOUT,
                        };
                        return Some(Event::Wake);
                    }
                }
                None
            }
            Phase::Listening { since, heard, wait } => {
                self.vad.accept_waveform(samples);
                let heard = heard || self.vad.detected();
                while let Some(segment) = self.vad.front() {
                    self.speech.extend_from_slice(segment.samples());
                    self.vad.pop();
                }
                let speaking = self.vad.detected();
                let done = (heard && !speaking && !self.speech.is_empty())
                    || (!heard && since.elapsed() > wait)
                    || since.elapsed() > wait + MAX_UTTERANCE;
                if !done {
                    self.phase = Phase::Listening { since, heard, wait };
                    return None;
                }
                self.vad.flush();
                while let Some(segment) = self.vad.front() {
                    self.speech.extend_from_slice(segment.samples());
                    self.vad.pop();
                }
                self.phase = Phase::Idle;
                if let Some((kws, stream)) = &self.kws {
                    kws.reset(stream);
                }
                let speech = std::mem::take(&mut self.speech);
                Some(Event::Utterance((!speech.is_empty()).then_some(speech)))
            }
        }
    }

    pub fn transcribe(&self, samples: &[f32]) -> String {
        let stream = self.asr.create_stream();
        stream.accept_waveform(SAMPLE_RATE, samples);
        self.asr.decode(&stream);
        stream
            .get_result()
            .map(|r| r.text.trim().to_owned())
            .unwrap_or_default()
    }
}
