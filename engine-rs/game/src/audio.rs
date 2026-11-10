//! Plays a song and tells where in it the listener is. The song position drives everything else,
//! so it comes from the sound card's own sample count, not from the frame timer.
use std::path::Path;
use std::time::Instant;

pub struct Song {
    /// Silent clock: ms played before the current run, and when that run began.
    played: f64,
    started: Option<Instant>,
    #[cfg(feature = "audio")]
    output: Option<output::Output>,
}

impl Song {
    /// Loads the tracks (instrumental, vocals) to play together, each with its own volume. Without a usable sound device the song runs silently.
    pub fn load(tracks: &[&Path]) -> Song {
        #[cfg(feature = "audio")]
        {
            let output = output::Output::open(tracks).map_err(|e| eprintln!("No sound: {e}")).ok();
            Song { played: 0.0, started: None, output }
        }
        #[cfg(not(feature = "audio"))]
        {
            let _ = tracks;
            Song { played: 0.0, started: None }
        }
    }

    /// Starts or resumes.
    pub fn start(&mut self) {
        self.started = Some(Instant::now());
        #[cfg(feature = "audio")]
        if let Some(output) = &self.output {
            output.play();
        }
    }

    pub fn pause(&mut self) {
        self.played = self.position_ms();
        self.started = None;
        #[cfg(feature = "audio")]
        if let Some(output) = &self.output {
            output.pause(self.played);
        }
    }

    /// Volume of one of the loaded tracks, 0 to 1.
    pub fn set_volume(&self, track: usize, volume: f32) {
        #[cfg(feature = "audio")]
        if let Some(volumes) = self.output.as_ref().and_then(|o| o.volumes.get(track)) {
            volumes.store(volume.to_bits(), std::sync::atomic::Ordering::Relaxed);
        }
        #[cfg(not(feature = "audio"))]
        let _ = (track, volume);
    }

    /// Milliseconds of the song heard so far.
    pub fn position_ms(&self) -> f64 {
        #[cfg(feature = "audio")]
        if let Some(heard) = self.output.as_ref().and_then(|o| o.heard_ms()) {
            return heard;
        }
        self.played + self.started.map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0)
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
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    struct Shared {
        playing: AtomicBool,
        /// A song frame and the moment it reaches (or reached) the speakers. None while not playing.
        anchor: Mutex<Option<(f64, Instant)>>,
        /// Position to report while paused, in ms.
        frozen: Mutex<Option<f64>>,
    }

    pub struct Output {
        _stream: cpal::Stream,
        shared: Arc<Shared>,
        /// One per track, an f32 as bits.
        pub volumes: Arc<Vec<AtomicU32>>,
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
            let (first, rate) = decode(tracks.first().ok_or("no track")?)?;
            let mut loaded = vec![first];
            for track in &tracks[1..] {
                let (samples, track_rate) = decode(track)?;
                if track_rate != rate {
                    // shortcut: tracks of one song share a sample rate in every song seen so far; resample here if one does not.
                    eprintln!("{} has another sample rate than the instrumental, left silent", track.display());
                    loaded.push(Vec::new());
                } else {
                    loaded.push(samples);
                }
            }
            let volumes: Arc<Vec<AtomicU32>> = Arc::new(loaded.iter().map(|_| AtomicU32::new(1.0f32.to_bits())).collect());
            let for_mix = volumes.clone();

            let device = cpal::default_host().default_output_device().ok_or("no output device")?;
            let config = device.default_output_config().map_err(|e| e.to_string())?;
            if config.sample_format() != cpal::SampleFormat::F32 {
                // shortcut: only float output (what WASAPI shared mode and PipeWire give). Convert here for devices that want integers.
                return Err(format!("output device wants {:?} samples", config.sample_format()));
            }
            let channels = config.channels() as usize;
            let step = rate as f64 / config.sample_rate().0 as f64;
            let frames = loaded[0].len() / 2;
            let shared = Arc::new(Shared { playing: AtomicBool::new(false), anchor: Mutex::new(None), frozen: Mutex::new(None) });
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
                    {
                        // checked again under the lock: a pause that got in between must not be undone
                        let mut anchor = for_callback.anchor.lock().unwrap();
                        if for_callback.playing.load(Ordering::Relaxed) {
                            *anchor = Some((cursor, Instant::now() + until_heard));
                        }
                    }
                    for frame in data.chunks_mut(channels) {
                        let (index, blend) = (cursor as usize, cursor.fract() as f32);
                        for (channel, out) in frame.iter_mut().enumerate() {
                            *out = 0.0;
                            if channel < 2 {
                                for (samples, volume) in loaded.iter().zip(for_mix.iter()) {
                                    if let (Some(a), Some(b)) = (samples.get(index * 2 + channel), samples.get(index * 2 + 2 + channel)) {
                                        *out += (a * (1.0 - blend) + b * blend) * f32::from_bits(volume.load(Ordering::Relaxed));
                                    }
                                }
                            }
                        }
                        cursor += step;
                    }
                },
                |error| eprintln!("Sound output: {error}"),
                None,
            ).map_err(|e| e.to_string())?;
            stream.play().map_err(|e| e.to_string())?;
            Ok(Output { _stream: stream, shared, volumes, rate: rate as f64, length_ms: frames as f64 / rate as f64 * 1000.0 })
        }

        pub fn play(&self) {
            self.shared.playing.store(true, Ordering::Relaxed);
        }

        /// Stops the sound; `heard_ms` is what the position stays at until it plays again.
        pub fn pause(&self, heard_ms: f64) {
            self.shared.playing.store(false, Ordering::Relaxed);
            *self.shared.frozen.lock().unwrap() = Some(heard_ms);
            *self.shared.anchor.lock().unwrap() = None;
        }

        /// None until the first buffer of the song is on its way out.
        pub fn heard_ms(&self) -> Option<f64> {
            let Some((frame, at)) = *self.shared.anchor.lock().unwrap() else { return *self.shared.frozen.lock().unwrap() };
            let now = Instant::now();
            let since = if now >= at { (now - at).as_secs_f64() } else { -(at - now).as_secs_f64() };
            Some(((frame / self.rate + since) * 1000.0).min(self.length_ms))
        }
    }
}
