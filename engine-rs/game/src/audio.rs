//! Plays a song and tells where in it the listener is. The song position drives everything else,
//! so it comes from the sound card's own sample count, not from the frame timer.
use std::path::Path;
use std::time::Instant;

pub struct Song {
    started: Option<Instant>,
    #[cfg(feature = "audio")]
    output: Option<output::Output>,
}

impl Song {
    /// Loads the tracks (instrumental, vocals) to play together. Without a usable sound device the song runs silently.
    pub fn load(tracks: &[&Path]) -> Song {
        #[cfg(feature = "audio")]
        {
            let output = output::Output::open(tracks).map_err(|e| eprintln!("No sound: {e}")).ok();
            Song { started: None, output }
        }
        #[cfg(not(feature = "audio"))]
        {
            let _ = tracks;
            Song { started: None }
        }
    }

    pub fn start(&mut self) {
        self.started = Some(Instant::now());
        #[cfg(feature = "audio")]
        if let Some(output) = &self.output {
            output.play();
        }
    }

    /// Milliseconds of the song heard so far.
    pub fn position_ms(&self) -> f64 {
        #[cfg(feature = "audio")]
        if let Some(heard) = self.output.as_ref().and_then(|o| o.heard_ms()) {
            return heard;
        }
        self.started.map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0)
    }

    /// Length of the audio, when there is any.
    pub fn length_ms(&self) -> Option<f64> {
        #[cfg(feature = "audio")]
        return self.output.as_ref().map(|o| o.length_ms);
        #[cfg(not(feature = "audio"))]
        None
    }
}

#[cfg(feature = "audio")]
mod output {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    struct Shared {
        playing: AtomicBool,
        /// A song frame and the moment it reaches (or reached) the speakers.
        anchor: Mutex<Option<(f64, Instant)>>,
    }

    pub struct Output {
        _stream: cpal::Stream,
        shared: Arc<Shared>,
        rate: f64,
        pub length_ms: f64,
    }

    /// Decodes an Ogg Vorbis file to interleaved stereo.
    fn decode(path: &Path) -> Result<(Vec<f32>, u32), String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut reader = lewton::inside_ogg::OggStreamReader::new(file).map_err(|e| format!("{}: {e}", path.display()))?;
        let channels = reader.ident_hdr.audio_channels as usize;
        let mut samples = Vec::new();
        while let Some(packet) = reader.read_dec_packet_itl().map_err(|e| format!("{}: {e}", path.display()))? {
            for frame in packet.chunks(channels) {
                let right = if channels > 1 { frame[1] } else { frame[0] };
                samples.extend([frame[0] as f32 / 32768.0, right as f32 / 32768.0]);
            }
        }
        Ok((samples, reader.ident_hdr.audio_sample_rate))
    }

    impl Output {
        pub fn open(tracks: &[&Path]) -> Result<Output, String> {
            let (mut samples, rate) = decode(tracks.first().ok_or("no track")?)?;
            for track in &tracks[1..] {
                let (more, more_rate) = decode(track)?;
                if more_rate != rate {
                    // shortcut: tracks of one song share a sample rate in every song seen so far; resample here if one does not.
                    eprintln!("{} has another sample rate than the instrumental, left out", track.display());
                    continue;
                }
                samples.resize(samples.len().max(more.len()), 0.0);
                for (out, add) in samples.iter_mut().zip(&more) {
                    *out += add;
                }
            }

            let device = cpal::default_host().default_output_device().ok_or("no output device")?;
            let config = device.default_output_config().map_err(|e| e.to_string())?;
            if config.sample_format() != cpal::SampleFormat::F32 {
                // shortcut: only float output (what WASAPI shared mode and PipeWire give). Convert here for devices that want integers.
                return Err(format!("output device wants {:?} samples", config.sample_format()));
            }
            let channels = config.channels() as usize;
            let step = rate as f64 / config.sample_rate().0 as f64;
            let frames = samples.len() / 2;
            let shared = Arc::new(Shared { playing: AtomicBool::new(false), anchor: Mutex::new(None) });
            let for_callback = shared.clone();
            let mut cursor = 0.0f64;
            let stream = device.build_output_stream(
                &config.into(),
                move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                    if !for_callback.playing.load(Ordering::Relaxed) {
                        data.fill(0.0);
                        return;
                    }
                    let time = info.timestamp();
                    let until_heard = time.playback.duration_since(&time.callback).unwrap_or(Duration::ZERO);
                    *for_callback.anchor.lock().unwrap() = Some((cursor, Instant::now() + until_heard));
                    for frame in data.chunks_mut(channels) {
                        let (index, blend) = (cursor as usize, cursor.fract() as f32);
                        for (channel, out) in frame.iter_mut().enumerate() {
                            *out = if channel < 2 && index + 1 < frames {
                                samples[index * 2 + channel] * (1.0 - blend) + samples[index * 2 + 2 + channel] * blend
                            } else {
                                0.0
                            };
                        }
                        cursor += step;
                    }
                },
                |error| eprintln!("Sound output: {error}"),
                None,
            ).map_err(|e| e.to_string())?;
            stream.play().map_err(|e| e.to_string())?;
            Ok(Output { _stream: stream, shared, rate: rate as f64, length_ms: frames as f64 / rate as f64 * 1000.0 })
        }

        pub fn play(&self) {
            self.shared.playing.store(true, Ordering::Relaxed);
        }

        /// None until the first buffer of the song is on its way out.
        pub fn heard_ms(&self) -> Option<f64> {
            let (frame, at) = (*self.shared.anchor.lock().unwrap())?;
            let now = Instant::now();
            let since = if now >= at { (now - at).as_secs_f64() } else { -(at - now).as_secs_f64() };
            Some(((frame / self.rate + since) * 1000.0).min(self.length_ms))
        }
    }
}
