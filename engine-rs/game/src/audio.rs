//! Plays a song and tells where in it the listener is. The song position drives everything else,
//! so it comes from the sound card's own sample count, not from the frame timer.
use std::path::Path;
use std::time::Instant;

pub struct Song {
    /// Silent clock: ms played before the current run, and when that run began.
    played: f64,
    started: Option<Instant>,
    /// How fast it plays: 2 is twice as fast (and an octave up).
    speed: f64,
    #[cfg(feature = "audio")]
    output: Option<output::Output>,
}

impl Song {
    /// Loads the tracks (instrumental, vocals) to play together, each with its own volume. Without a usable sound device the song runs silently.
    pub fn load(tracks: &[&Path]) -> Song {
        Song::load_at(tracks, 1.0)
    }

    /// The same, played `speed` times as fast.
    pub fn load_at(tracks: &[&Path], speed: f64) -> Song {
        #[cfg(feature = "audio")]
        {
            let output = output::Output::open(tracks, speed).map_err(|e| eprintln!("No sound: {e}")).ok();
            Song { played: 0.0, started: None, speed, output }
        }
        #[cfg(not(feature = "audio"))]
        {
            let _ = tracks;
            Song { played: 0.0, started: None, speed }
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
        self.played + self.started.map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0 * self.speed)
    }

    /// Plays a short sound (or, looped, a piece of music) on top of the song, whether the song runs or not.
    pub fn play_sound(&self, path: &Path, volume: f32, looped: bool) {
        #[cfg(feature = "audio")]
        if let Some(output) = &self.output {
            output.play_sound(path, volume, looped);
        }
        #[cfg(not(feature = "audio"))]
        let _ = (path, volume, looped);
    }

    pub fn stop_sounds(&self) {
        #[cfg(feature = "audio")]
        if let Some(output) = &self.output {
            output.sounds.lock().unwrap().clear();
        }
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

    /// A sound playing on top of the song.
    pub struct Voice {
        samples: Arc<Vec<f32>>,
        /// Source frames per output frame.
        step: f64,
        cursor: f64,
        volume: f32,
        looped: bool,
    }

    pub struct Output {
        _stream: cpal::Stream,
        shared: Arc<Shared>,
        pub sounds: Arc<Mutex<Vec<Voice>>>,
        device_rate: f64,
        /// One per track, an f32 as bits.
        pub volumes: Arc<Vec<AtomicU32>>,
        rate: f64,
        speed: f64,
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
        pub fn open(tracks: &[&Path], speed: f64) -> Result<Output, String> {
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
            let step = rate as f64 / config.sample_rate().0 as f64 * speed;
            let frames = loaded[0].len() / 2;
            let shared = Arc::new(Shared { playing: AtomicBool::new(false), anchor: Mutex::new(None), frozen: Mutex::new(None) });
            let for_callback = shared.clone();
            let sounds: Arc<Mutex<Vec<Voice>>> = Arc::new(Mutex::new(Vec::new()));
            let for_sounds = sounds.clone();
            let device_rate = config.sample_rate().0 as f64;
            let mut cursor = 0.0f64;
            let stream = device.build_output_stream(
                &config.into(),
                move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                    data.fill(0.0);
                    if for_callback.playing.load(Ordering::Relaxed) {
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
                            for (channel, out) in frame.iter_mut().take(2).enumerate() {
                                for (samples, volume) in loaded.iter().zip(for_mix.iter()) {
                                    if let (Some(a), Some(b)) = (samples.get(index * 2 + channel), samples.get(index * 2 + 2 + channel)) {
                                        *out += (a * (1.0 - blend) + b * blend) * f32::from_bits(volume.load(Ordering::Relaxed));
                                    }
                                }
                            }
                            cursor += step;
                        }
                    }
                    let mut sounds = for_sounds.lock().unwrap();
                    for voice in sounds.iter_mut() {
                        let frames = voice.samples.len() / 2;
                        for frame in data.chunks_mut(channels) {
                            if voice.looped && frames > 0 {
                                voice.cursor %= frames as f64;
                            }
                            let index = voice.cursor as usize;
                            for (channel, out) in frame.iter_mut().take(2).enumerate() {
                                *out += voice.samples.get(index * 2 + channel).copied().unwrap_or(0.0) * voice.volume;
                            }
                            voice.cursor += voice.step;
                        }
                    }
                    sounds.retain(|voice| voice.looped || (voice.cursor as usize) < voice.samples.len() / 2);
                },
                |error| eprintln!("Sound output: {error}"),
                None,
            ).map_err(|e| e.to_string())?;
            stream.play().map_err(|e| e.to_string())?;
            Ok(Output { _stream: stream, shared, sounds, device_rate, volumes, rate: rate as f64, speed, length_ms: frames as f64 / rate as f64 * 1000.0 })
        }

        pub fn play(&self) {
            self.shared.playing.store(true, Ordering::Relaxed);
        }

        // shortcut: the file is read and decoded when the sound is asked for (a few ms for the short ones). Keep decoded sounds around if a hitch shows.
        pub fn play_sound(&self, path: &Path, volume: f32, looped: bool) {
            match decode(path) {
                Ok((samples, rate)) => self.sounds.lock().unwrap().push(Voice { samples: Arc::new(samples), step: rate as f64 / self.device_rate, cursor: 0.0, volume, looped }),
                Err(error) => eprintln!("Sound: {error}"),
            }
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
            Some(((frame / self.rate + since * self.speed) * 1000.0).min(self.length_ms))
        }
    }
}
