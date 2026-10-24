// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Spoken requests for the Chat panel.
//!
//! Talk starts the microphone; Done stops it and sends what was heard to a
//! supervised `whisper-server`, which runs the speech model on this
//! machine. The words land in the chat's text field, not in the
//! conversation, so a misheard one is fixed before it reaches the model.
//! Nothing listens until Talk is pressed, and the recording is dropped
//! once it is transcribed.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use runebender::workflows::whisper_server::{self, WhisperServer};

use crate::application::editor::tools::chat::ChatJob;
use crate::application::workspace::Workspace;

/// The longest one recording may run before it stops on its own.
const MAX_RECORDING: Duration = Duration::from_secs(90);

/// Words the speech model should expect, so it spells them as a type
/// designer would. The open font's family name is added to it.
const VOCABULARY: &str = "Runebender. Glyph, glyphs, outline, contour, stem, bowl, counter, \
    serif, terminal, overshoot, sidebearing, sidebearings, advance width, kerning, kern pair, \
    baseline, x-height, cap height, ascender, descender, master, interpolation, units. \
    Eszett, ampersand, lowercase, uppercase, figures, zero, oh.";

/// A microphone capture in progress, owned by its own thread.
#[derive(Debug, Default)]
pub(crate) struct Capture {
    samples: Arc<Mutex<Vec<f32>>>,
    stop: Arc<AtomicBool>,
    /// The device's rate and channel count, set once the stream is open.
    format: Arc<Mutex<Option<(u32, u16)>>>,
    error: Arc<Mutex<Option<String>>>,
    /// Where the thread reports that samples are flowing.
    events: Arc<Mutex<Vec<serde_json::Value>>>,
    /// Samples have arrived: the device takes a moment to start, and the
    /// panel says so until then.
    pub(crate) started: bool,
}

impl Capture {
    /// A capture with samples flowing and no thread behind it, for
    /// headless renders of the listening state.
    pub(crate) fn sample() -> Self {
        Self {
            started: true,
            ..Self::default()
        }
    }

    fn thread_view(&self) -> Self {
        Self {
            samples: self.samples.clone(),
            stop: self.stop.clone(),
            format: self.format.clone(),
            error: self.error.clone(),
            events: self.events.clone(),
            started: false,
        }
    }
}

/// Everything the Chat panel keeps for voice input.
#[derive(Debug, Default)]
pub(crate) struct VoiceState {
    /// The microphone is open.
    pub(crate) capture: Option<Capture>,
    /// The thread behind the current phase: the capture while the
    /// microphone is open, then the transcription, whose `heard` event
    /// carries the text.
    pub(crate) job: Option<ChatJob>,
    /// The `whisper-server` child, kept across requests so the model loads once.
    server: Arc<Mutex<Option<WhisperServer>>>,
}

impl VoiceState {
    /// Whether the microphone is open or a transcription is in flight.
    pub(crate) fn active(&self) -> bool {
        self.capture.is_some() || self.job.is_some()
    }
}

impl Drop for VoiceState {
    fn drop(&mut self) {
        if let Some(capture) = &self.capture {
            capture.stop.store(true, Ordering::Release);
        }
    }
}

/// Whether a transcription is only the model's note about non-speech,
/// such as `(audience mumbling)` or `[BLANK_AUDIO]`.
fn is_placeholder(text: &str) -> bool {
    let text = text.trim();
    (text.starts_with('(') && text.ends_with(')')) || (text.starts_with('[') && text.ends_with(']'))
}

#[cfg(test)]
mod tests {
    use super::is_placeholder;

    #[test]
    fn silence_notes_are_placeholders_and_speech_is_not() {
        assert!(is_placeholder("(audience mumbling)"));
        assert!(is_placeholder(" [BLANK_AUDIO] "));
        assert!(!is_placeholder("Make the zero (the figure) wider."));
        assert!(!is_placeholder(""));
    }
}

/// The browser host has no microphone access yet.
#[cfg(target_arch = "wasm32")]
fn record(_capture: &Capture) -> Result<(), String> {
    Err("the browser cannot record audio yet".into())
}

/// Open the default input device and collect its samples until `stop` is
/// set or [`MAX_RECORDING`] passes. Runs on its own thread, which owns the
/// stream.
#[cfg(not(target_arch = "wasm32"))]
fn record(capture: &Capture) -> Result<(), String> {
    use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("no microphone found")?;
    let supported = device
        .default_input_config()
        .map_err(|error| format!("microphone: {error}"))?;
    let config = supported.config();
    *capture.format.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((config.sample_rate, config.channels));
    let samples = capture.samples.clone();
    let failed = capture.error.clone();
    let on_error = move |error: cpal::Error| {
        *failed.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("microphone: {error}"));
    };
    fn collect<T>(samples: Arc<Mutex<Vec<f32>>>) -> impl FnMut(&[T], &cpal::InputCallbackInfo)
    where
        T: cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        move |data: &[T], _| {
            samples.lock().unwrap_or_else(|e| e.into_inner()).extend(
                data.iter()
                    .map(|&s| <f32 as cpal::FromSample<T>>::from_sample_(s)),
            );
        }
    }
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            device.build_input_stream(config, collect::<f32>(samples.clone()), on_error, None)
        }
        cpal::SampleFormat::I16 => {
            device.build_input_stream(config, collect::<i16>(samples.clone()), on_error, None)
        }
        cpal::SampleFormat::U16 => {
            device.build_input_stream(config, collect::<u16>(samples.clone()), on_error, None)
        }
        cpal::SampleFormat::I32 => {
            device.build_input_stream(config, collect::<i32>(samples.clone()), on_error, None)
        }
        other => return Err(format!("microphone sample format {other} is not supported")),
    }
    .map_err(|error| format!("microphone: {error}"))?;
    stream
        .play()
        .map_err(|error| format!("microphone: {error}"))?;
    let started = std::time::Instant::now();
    let mut flowing = false;
    while !capture.stop.load(Ordering::Acquire) && started.elapsed() < MAX_RECORDING {
        if capture
            .error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
        {
            break;
        }
        if !flowing
            && !capture
                .samples
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        {
            flowing = true;
            capture
                .events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(serde_json::json!({"event": "started"}));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(stream);
    Ok(())
}

impl Workspace {
    /// Whether a speech model and server are on this machine.
    pub(crate) fn voice_available(&self) -> bool {
        whisper_server::find_binary().is_some() && self.voice_model().is_some()
    }

    fn voice_model(&self) -> Option<PathBuf> {
        let roots = runebender::workflows::graph_run::model_roots(Self::models_dir().as_deref());
        whisper_server::find_model(&roots)
    }

    /// Talk: open the microphone.
    pub(crate) fn voice_start(&mut self) {
        if self.voice.active() {
            return;
        }
        if whisper_server::find_binary().is_none() {
            self.note = "Install whisper.cpp (brew install whisper.cpp) or set \
                         RUNEBENDER_WHISPER_SERVER to use voice input"
                .into();
            return;
        }
        if self.voice_model().is_none() {
            self.note = "Put a whisper.cpp model in a models folder named whisper-… as \
                         model.bin to use voice input"
                .into();
            return;
        }
        let job = ChatJob::default();
        let capture = Capture {
            events: job.events.clone(),
            ..Capture::default()
        };
        let thread_view = capture.thread_view();
        // The job outlives the microphone: it is the same one the
        // transcription reports through, so the panel's poll of it runs
        // from Talk to the words arriving. The thread ending on its own
        // (the time limit, a fault) is an event, not the end of the job.
        std::thread::spawn(move || {
            let result = record(&thread_view);
            thread_view.stop.store(true, Ordering::Release);
            let event = match result {
                Ok(()) => serde_json::json!({"event": "ended"}),
                Err(error) => serde_json::json!({"event": "ended", "error": error}),
            };
            thread_view
                .events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(event);
        });
        self.voice.capture = Some(capture);
        self.voice.job = Some(job);
    }

    /// Done: close the microphone and transcribe what it heard.
    pub(crate) fn voice_stop(&mut self) {
        let Some(capture) = self.voice.capture.take() else {
            return;
        };
        let Some(job) = self.voice.job.clone() else {
            return;
        };
        let fail = |app: &mut Self, note: String| {
            app.voice.job = None;
            app.note = note;
        };
        capture.stop.store(true, Ordering::Release);
        // The capture thread may still be appending its last buffer.
        std::thread::sleep(Duration::from_millis(80));
        if let Some(error) = capture
            .error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            fail(self, error);
            return;
        }
        let samples =
            std::mem::take(&mut *capture.samples.lock().unwrap_or_else(|e| e.into_inner()));
        let Some((rate, channels)) = *capture.format.lock().unwrap_or_else(|e| e.into_inner())
        else {
            fail(self, "The microphone did not open".into());
            return;
        };
        let samples = whisper_server::to_mono_16k(&samples, channels, rate);
        if samples.len() < whisper_server::SAMPLE_RATE as usize / 2 {
            fail(self, "Nothing heard".into());
            return;
        }
        let (Some(binary), Some(model)) = (whisper_server::find_binary(), self.voice_model())
        else {
            fail(self, "The speech model is no longer available".into());
            return;
        };
        let family = self
            .font
            .font_info()
            .names
            .family_name
            .clone()
            .unwrap_or_default();
        let prompt = format!("{family}. {VOCABULARY}");
        let server = self.voice.server.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let mut slot = server.lock().unwrap_or_else(|e| e.into_inner());
                let reuse = slot
                    .as_mut()
                    .is_some_and(|current| current.model() == model && current.is_running());
                if !reuse {
                    *slot = None;
                    *slot = Some(WhisperServer::start(&binary, &model)?);
                }
                let text = slot
                    .as_ref()
                    .expect("server just checked or started")
                    .transcribe(&samples, &prompt)?;
                job.events
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(serde_json::json!({"event": "heard", "text": text}));
                Ok(())
            })();
            *job.finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        });
    }

    /// Follow the microphone, then put a finished transcription into the
    /// chat's text field.
    pub(crate) fn voice_pump(&mut self) {
        let Some(job) = self.voice.job.clone() else {
            return;
        };
        let events = std::mem::take(&mut *job.events.lock().unwrap_or_else(|e| e.into_inner()));
        for event in events {
            match event.get("event").and_then(serde_json::Value::as_str) {
                Some("started") => {
                    if let Some(capture) = &mut self.voice.capture {
                        capture.started = true;
                    }
                }
                Some("ended") if self.voice.capture.is_some() => {
                    // The microphone thread ended on its own: the time
                    // limit, or a fault.
                    match event.get("error").and_then(serde_json::Value::as_str) {
                        None => self.voice_stop(),
                        Some(error) => {
                            self.voice.capture = None;
                            self.voice.job = None;
                            self.note = error.to_owned();
                        }
                    }
                }
                Some("heard") => {
                    let text = event
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    // Silence comes back as a bracketed note such as
                    // "(audience mumbling)" or "[BLANK_AUDIO]".
                    if text.is_empty() || is_placeholder(text) {
                        self.note = "Nothing heard".into();
                        continue;
                    }
                    eprintln!("voice heard: {text}");
                    if !self.chat.prompt.is_empty() && !self.chat.prompt.ends_with(' ') {
                        self.chat.prompt.push(' ');
                    }
                    self.chat.prompt.push_str(text);
                }
                _ => {}
            }
        }
        let finished = job
            .finished
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(result) = finished {
            self.voice.job = None;
            if let Err(error) = result {
                self.note = error;
            }
        }
    }
}

#[cfg(test)]
mod real_microphone {
    //! Runs only with `RUNEBENDER_REAL_MICROPHONE=1`: records two seconds
    //! from the default input and checks that samples arrived.
    use super::*;

    #[test]
    #[ignore = "opens the microphone; set RUNEBENDER_REAL_MICROPHONE=1"]
    fn two_seconds_of_microphone_arrive_as_samples() {
        if std::env::var_os("RUNEBENDER_REAL_MICROPHONE").is_none() {
            return;
        }
        let capture = Capture::default();
        let stop = capture.stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            stop.store(true, Ordering::Release);
        });
        record(&capture).unwrap();
        let format = capture.format.lock().unwrap().unwrap();
        let samples = capture.samples.lock().unwrap();
        let mono = whisper_server::to_mono_16k(&samples, format.1, format.0);
        eprintln!(
            "{} Hz, {} channels, {} samples, {:.1} s, peak {:.3}",
            format.0,
            format.1,
            samples.len(),
            mono.len() as f64 / f64::from(whisper_server::SAMPLE_RATE),
            mono.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
        );
        assert!(!mono.is_empty(), "samples arrived");
        assert!(capture.error.lock().unwrap().is_none());
        assert!(
            capture.events.lock().unwrap()[0]["event"] == "started",
            "the thread reported that samples were flowing"
        );
    }
}

#[cfg(test)]
mod real_voice {
    //! Runs only with `RUNEBENDER_REAL_VOICE=<designspace>`, a microphone,
    //! `whisper-server` and a speech model: Talk, five seconds, Done, and
    //! the pump until the words arrive. This is the whole path the panel
    //! drives, including the hand-over from the microphone thread to the
    //! transcription.
    use super::*;

    #[test]
    #[ignore = "opens the microphone and runs whisper-server; set RUNEBENDER_REAL_VOICE"]
    fn talk_then_done_puts_words_in_the_field() {
        let Some(font) = std::env::var_os("RUNEBENDER_REAL_VOICE").map(PathBuf::from) else {
            return;
        };
        let mut app = Workspace::open(&font).unwrap();
        assert!(
            app.voice_available(),
            "whisper-server and a model are needed"
        );
        app.voice_start();
        assert!(
            app.voice.capture.is_some() && app.voice.job.is_some(),
            "{}",
            app.note
        );
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            app.voice_pump();
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            app.voice.capture.as_ref().is_some_and(|c| c.started),
            "samples flow within five seconds"
        );
        app.voice_stop();
        assert!(
            app.voice.capture.is_none() && app.voice.job.is_some(),
            "{}",
            app.note
        );
        let started = std::time::Instant::now();
        while app.voice.job.is_some() && started.elapsed() < Duration::from_secs(60) {
            app.voice_pump();
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            app.voice.job.is_none(),
            "the transcription ends; note: {}",
            app.note
        );
        eprintln!("field: {:?}; note: {:?}", app.chat.prompt, app.note);
    }
}
