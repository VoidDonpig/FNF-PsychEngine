//! Plays a song and tells where in it the listener is. The song position drives everything else,
//! so it comes from the sound card's own sample count, not from the frame timer.
use std::path::Path;
use std::time::Instant;

/// How loud seven bands of a stretch of sound are, low to high, each from 0 (silent) to 1.
/// `window` is one channel's samples, `rate` how many of them make a second.
pub fn bands(window: &[f32], rate: f32) -> [f32; 7] {
    [60.0, 135.0, 300.0, 680.0, 1500.0, 3400.0, 7500.0].map(|pitch: f32| {
        let (mut along, mut across) = (0.0, 0.0);
        for (index, sample) in window.iter().enumerate() {
            // faded in and out, so that the cut ends of the stretch do not ring
            let fade = 0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / window.len() as f32).cos();
            let turn = std::f32::consts::TAU * pitch * index as f32 / rate;
            along += sample * fade * turn.cos();
            across += sample * fade * turn.sin();
        }
        let loudness = (along * along + across * across).sqrt() / (window.len().max(1) as f32 / 4.0);
        // from -50 dB up to -10 dB
        ((20.0 * (loudness + 1e-9).log10() + 50.0) / 40.0).clamp(0.0, 1.0)
    })
}

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

    /// Jumps to `ms` into the song, playing or paused.
    pub fn seek(&mut self, ms: f64) {
        self.played = ms;
        self.started = self.started.map(|_| Instant::now());
        #[cfg(feature = "audio")]
        if let Some(output) = &self.output {
            output.seek(ms);
        }
    }

    /// Plays `speed` times as fast from here on (the pitch follows).
    pub fn set_speed(&mut self, speed: f64) {
        // what was played so far was played at the old speed
        self.played = self.position_ms();
        self.started = self.started.map(|_| Instant::now());
        self.speed = speed;
        #[cfg(feature = "audio")]
        if let Some(output) = &self.output {
            output.shared.speed.store(speed.to_bits(), std::sync::atomic::Ordering::Relaxed);
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

    /// How loud seven bands of the instrumental are right now, low to high (0 to 1). All silent without sound.
    pub fn levels(&self) -> [f32; 7] {
        #[cfg(feature = "audio")]
        if let Some(output) = &self.output {
            return output.levels();
        }
        [0.0; 7]
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
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    pub struct Shared {
        playing: AtomicBool,
        /// How fast the song plays, an f64 as bits.
        pub speed: AtomicU64,
        /// A song frame to jump to when the next buffer is filled. Locked only while `anchor` is.
        seek: Mutex<Option<f64>>,
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
        pub shared: Arc<Shared>,
        pub sounds: Arc<Mutex<Vec<Voice>>>,
        device_rate: f64,
        /// One per track, an f32 as bits.
        pub volumes: Arc<Vec<AtomicU32>>,
        rate: f64,
        /// The instrumental's samples, for showing how it sounds.
        first: Arc<Vec<f32>>,
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

    /// A decoded file: its samples, its rate, and whether it is a track of a song.
    type Decoded = (std::path::PathBuf, Arc<Vec<f32>>, u32, bool);

    /// The files decoded so far: sounds for good (they are short, or come back all the time like the menu music), the tracks of a
    /// song until another song is opened. Playing or trying a song again does not decode it again.
    static DECODED: Mutex<Vec<Decoded>> = Mutex::new(Vec::new());

    pub(super) fn decoded(path: &Path, track: bool) -> Result<(Arc<Vec<f32>>, u32), String> {
        if let Some((_, samples, rate, _)) = DECODED.lock().unwrap().iter().find(|kept| kept.0 == path) {
            return Ok((samples.clone(), *rate));
        }
        let (samples, rate) = decode(path)?;
        let samples = Arc::new(samples);
        DECODED.lock().unwrap().push((path.to_owned(), samples.clone(), rate, track));
        Ok((samples, rate))
    }

    /// The tracks of a song, decoded side by side. Those of the song before are let go of.
    pub(super) fn tracks_of(tracks: &[&Path]) -> Vec<Result<(Arc<Vec<f32>>, u32), String>> {
        if !tracks.is_empty() {
            DECODED.lock().unwrap().retain(|kept| !kept.3 || tracks.contains(&kept.0.as_path()));
        }
        std::thread::scope(|scope| {
            let decoding: Vec<_> = tracks.iter().map(|track| scope.spawn(move || decoded(track, true))).collect();
            decoding.into_iter().map(|track| track.join().unwrap_or_else(|_| Err("decoding failed".to_owned()))).collect()
        })
    }

    impl Output {
        pub fn open(tracks: &[&Path], speed: f64) -> Result<Output, String> {
            let mut decoded = tracks_of(tracks).into_iter();
            // no tracks: an output that only plays sounds, as the menus use it
            let (first, rate) = decoded.next().unwrap_or_else(|| Ok((Arc::default(), 44100)))?;
            let mut loaded = vec![first.clone()];
            for (track, other) in tracks.iter().skip(1).zip(decoded) {
                let (samples, track_rate) = other?;
                if track_rate != rate {
                    // shortcut: tracks of one song share a sample rate in every song seen so far; resample here if one does not.
                    eprintln!("{} has another sample rate than the instrumental, left silent", track.display());
                    loaded.push(Arc::default());
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
            let shared = Arc::new(Shared { playing: AtomicBool::new(false), speed: AtomicU64::new(speed.to_bits()), seek: Mutex::new(None), anchor: Mutex::new(None), frozen: Mutex::new(None) });
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
                            if let Some(frame) = for_callback.seek.lock().unwrap().take() {
                                cursor = frame;
                            }
                            if for_callback.playing.load(Ordering::Relaxed) {
                                *anchor = Some((cursor, Instant::now() + until_heard));
                            }
                        }
                        let step = step * f64::from_bits(for_callback.speed.load(Ordering::Relaxed));
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
            Ok(Output { _stream: stream, shared, sounds, device_rate, volumes, rate: rate as f64, first, length_ms: frames as f64 / rate as f64 * 1000.0 })
        }

        pub fn play(&self) {
            self.shared.playing.store(true, Ordering::Relaxed);
        }

        pub fn play_sound(&self, path: &Path, volume: f32, looped: bool) {
            match decoded(path, false) {
                Ok((samples, rate)) => self.sounds.lock().unwrap().push(Voice { samples, step: rate as f64 / self.device_rate, cursor: 0.0, volume, looped }),
                Err(error) => eprintln!("Sound: {error}"),
            }
        }

        /// Stops the sound; `heard_ms` is what the position stays at until it plays again.
        pub fn pause(&self, heard_ms: f64) {
            self.shared.playing.store(false, Ordering::Relaxed);
            *self.shared.frozen.lock().unwrap() = Some(heard_ms);
            *self.shared.anchor.lock().unwrap() = None;
        }

        /// Jumps to `ms` into the song: that is the position until the next buffer plays from there.
        pub fn seek(&self, ms: f64) {
            let mut anchor = self.shared.anchor.lock().unwrap();
            *self.shared.seek.lock().unwrap() = Some(ms / 1000.0 * self.rate);
            *anchor = None;
            *self.shared.frozen.lock().unwrap() = Some(ms);
        }

        /// The bands of the instrumental around what is heard now.
        pub fn levels(&self) -> [f32; 7] {
            let Some(heard) = self.heard_ms() else { return [0.0; 7] };
            let from = (heard / 1000.0 * self.rate) as usize;
            let window: Vec<f32> = self.first.chunks(2).skip(from).take(1024).map(|frame| frame.iter().sum::<f32>() / 2.0).collect();
            if window.len() < 64 { [0.0; 7] } else { super::bands(&window, self.rate as f32) }
        }

        /// None until the first buffer of the song is on its way out.
        pub fn heard_ms(&self) -> Option<f64> {
            // an output without tracks (the menus', or a song started part way in for a picture) has no position of its own
            if self.length_ms == 0.0 {
                return None;
            }
            let Some((frame, at)) = *self.shared.anchor.lock().unwrap() else { return *self.shared.frozen.lock().unwrap() };
            let now = Instant::now();
            let since = if now >= at { (now - at).as_secs_f64() } else { -(at - now).as_secs_f64() };
            Some(((frame / self.rate + since * f64::from_bits(self.shared.speed.load(Ordering::Relaxed))) * 1000.0).min(self.length_ms))
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "audio")]
    #[test]
    fn sounds_are_decoded_once_and_a_songs_tracks_until_another_song() {
        use super::output::{decoded, tracks_of};
        use std::sync::Arc;
        let sounds = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/shared/sounds");
        let [scroll, confirm, cancel] = ["scrollMenu.ogg", "confirmMenu.ogg", "cancelMenu.ogg"].map(|name| sounds.join(name));
        let track = |path: &std::path::Path| tracks_of(&[path]).remove(0).unwrap().0;
        let sound = decoded(&scroll, false).unwrap().0;
        let song = track(&confirm);
        assert!(!song.is_empty() && Arc::ptr_eq(&song, &track(&confirm)), "tried again: not decoded again");
        // the menus open the output without tracks: the song's stay
        assert!(tracks_of(&[]).is_empty() && Arc::ptr_eq(&song, &track(&confirm)));
        // another song, and back: the first one's tracks were let go of, sounds never are
        track(&cancel);
        assert!(!Arc::ptr_eq(&song, &track(&confirm)));
        assert!(Arc::ptr_eq(&sound, &decoded(&scroll, false).unwrap().0));
        assert!(decoded(&sounds.join("no such sound.ogg"), false).is_err());
    }

    #[test]
    fn a_paused_song_stays_where_it_is_wound_to() {
        let mut song = super::Song::load(&[]);
        song.seek(5000.0);
        song.set_speed(2.0);
        assert_eq!(song.position_ms(), 5000.0);
        song.start();
        std::thread::sleep(std::time::Duration::from_millis(20));
        // twice as fast: 20 ms are 40 ms of song at least
        assert!(song.position_ms() >= 5040.0);
        song.pause();
        song.seek(100.0);
        assert_eq!(song.position_ms(), 100.0);
    }

    #[test]
    fn a_tone_shows_in_its_own_band() {
        let rate = 44100.0;
        let tone: Vec<f32> = (0..1024).map(|index| 0.5 * (std::f32::consts::TAU * 300.0 * index as f32 / rate).sin()).collect();
        let levels = super::bands(&tone, rate);
        assert!(levels[2] > 0.9, "{levels:?}");
        assert!(levels.iter().enumerate().all(|(band, level)| band == 2 || *level < levels[2] - 0.3), "{levels:?}");
        assert_eq!(super::bands(&[0.0; 1024], rate), [0.0; 7]);
    }
}
