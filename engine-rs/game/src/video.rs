//! Videos, as the Haxe engine plays them in cutscenes (objects/VideoSprite.hx): mp4 files with H.264 pictures and AAC
//! sound, as wide as the screen and in its middle, over black unless they play during the song. Pictures are decoded on
//! a thread of their own a few frames ahead; the sound is decoded first, and the picture follows the time since it began.
//! Holding Accept for a second skips one that may be skipped, while a ring fills at the bottom right.
// shortcut: one picture every frame time of the file (constant frame rate, as the game's videos are); add the samples'
// own times if a mod's video has a varying rate.
// shortcut: a video over the song goes on under the pause menu; pause its sound and clock if a mod's mid-song video needs it.
use crate::audio::Song;
use crate::gfx::*;
use std::io::BufReader;
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::Instant;

/// A picture: its width, height and RGBA pixels.
type Frame = (usize, usize, Vec<u8>);

/// How long Accept is held to skip, in seconds (VideoSprite's _timeToSkip).
const TO_SKIP: f32 = 1.0;

pub struct Video {
    frames: mpsc::Receiver<Frame>,
    /// Pixel buffers given back to the decoder once shown, so it does not make a new one each frame.
    spare: mpsc::Sender<Vec<u8>>,
    sound: mpsc::Receiver<(Arc<Vec<i16>>, u32)>,
    /// The sound once it plays, to stop it with the video.
    playing: Option<Arc<Vec<i16>>>,
    frame_seconds: f64,
    picture: Option<Texture2D>,
    shown: u64,
    /// When the sound began (and so the pictures); None while it is decoded.
    started: Option<Instant>,
    held: f32,
    over: bool,
    looped: bool,
    pub skippable: bool,
    /// Over black, the game waiting (a cutscene); else drawn over the song, which goes on.
    pub cover: bool,
    /// Started when the song was over: the song ends with it.
    pub ending: bool,
}

impl Video {
    pub fn open(path: &Path, skippable: bool, cover: bool, looped: bool, ending: bool) -> Result<Video, String> {
        let failed = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
        let file = std::fs::File::open(path).map_err(|e| failed(&e))?;
        let size = file.metadata().map_err(|e| failed(&e))?.len();
        let reader = mp4::Mp4Reader::read_header(BufReader::new(file), size).map_err(|e| failed(&e))?;
        let (track, count, frame_seconds) = reader.tracks().iter().find(|(_, track)| track.media_type().ok() == Some(mp4::MediaType::H264))
            .map(|(id, track)| (*id, track.sample_count(), track.duration().as_secs_f64() / track.sample_count().max(1) as f64)).ok_or_else(|| failed(&"no H.264 pictures"))?;
        let (send_frames, frames) = mpsc::sync_channel(2);
        let (spare, spares) = mpsc::channel();
        let (send_sound, sound) = mpsc::channel();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            // no sound (or sound that cannot be read): the pictures play on the clock alone
            let _ = send_sound.send(decode_sound(&path).map_err(|error| eprintln!("{}: {error}", path.display())).unwrap_or((Arc::default(), 48000)));
            if let Err(error) = decode_pictures(reader, track, count, looped, &send_frames, &spares) {
                eprintln!("{}: {error}", path.display());
            }
        });
        Ok(Video { frames, spare, sound, frame_seconds, playing: None, picture: None, shown: 0, started: None, held: 0.0, over: false, looped, skippable, cover, ending })
    }

    /// Goes on; `accept` is whether Accept is down. True once it is over or skipped (its sound stopped).
    pub fn update(&mut self, song: &Song, dt: f32, accept: bool) -> bool {
        let over = self.advance(song, dt, accept);
        if over {
            if let Some(samples) = &self.playing {
                song.stop_samples(samples);
            }
        }
        over
    }

    fn advance(&mut self, song: &Song, dt: f32, accept: bool) -> bool {
        if self.started.is_none() {
            let Ok((samples, rate)) = self.sound.try_recv() else { return false };
            song.play_samples(samples.clone(), rate, self.looped);
            self.playing = Some(samples);
            self.started = Some(Instant::now());
        }
        if self.skippable {
            // the ring empties slowly when let go of, as VideoSprite's
            self.held = if accept { (self.held + dt).min(TO_SKIP) } else { self.held + (-0.1 - self.held) * (dt * 3.0).clamp(0.0, 1.0) }.max(0.0);
            if self.held >= TO_SKIP {
                return true;
            }
        }
        let due = self.started.map_or(0, |started| (started.elapsed().as_secs_f64() / self.frame_seconds) as u64 + 1);
        let mut newest = None;
        while self.shown < due {
            match self.frames.try_recv() {
                Ok(frame) => {
                    self.shown += 1;
                    if let Some((_, _, older)) = newest.replace(frame) {
                        let _ = self.spare.send(older);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.over = true;
                    break;
                }
            }
        }
        if let Some((width, height, pixels)) = newest {
            match &self.picture {
                Some(picture) if picture.size() == vec2(width as f32, height as f32) => {
                    picture.update(&pixels);
                    let _ = self.spare.send(pixels);
                }
                _ => self.picture = Some(Texture2D::from_image(&Image { width: width as u16, height: height as u16, bytes: pixels })),
            }
        }
        self.over
    }

    /// The video over the whole screen (the camera being the screen's 1280 by 720), and the skip ring.
    pub fn draw(&self) {
        if self.cover {
            draw_rectangle(-50.0, -50.0, 1380.0, 820.0, BLACK);
        }
        if let Some(picture) = &self.picture {
            // as wide as the screen, as tall as that makes it, in the middle
            let height = 1280.0 * picture.height() / picture.width();
            draw_texture_ex(picture, 0.0, (720.0 - height) / 2.0, WHITE, DrawTextureParams { dest_size: Some(vec2(1280.0, height)), ..Default::default() });
        }
        // FlxPieDial(radius 40, 40 pieces, clockwise from the top, a hole of 24) at the bottom right; it shows as it fills
        let filled = (self.held / TO_SKIP * 1.025).min(1.0);
        let alpha = ((filled - 0.025) / 0.975).clamp(0.0, 1.0);
        if alpha > 0.0 {
            let middle = vec2(1280.0 - 160.0 + 40.0, 720.0 - 152.0 + 40.0);
            for piece in 0..(filled * 40.0).round() as usize {
                let angle = (piece as f32 + 0.5) / 40.0 * std::f32::consts::TAU;
                let at = middle + vec2(angle.sin(), -angle.cos()) * 32.0;
                draw_rectangle_turned(at.x - 3.5, at.y - 8.0, 7.0, 16.0, angle, Color { a: alpha, ..WHITE });
            }
        }
    }
}

/// The file's sound, as stereo 16 bit samples, and their rate.
fn decode_sound(path: &Path) -> Result<(Arc<Vec<i16>>, u32), String> {
    use symphonia::core::{audio::SampleBuffer, codecs, formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint};
    let stream = MediaSourceStream::new(Box::new(std::fs::File::open(path).map_err(|e| e.to_string())?), Default::default());
    let mut format = symphonia::default::get_probe().format(Hint::new().with_extension("mp4"), stream, &FormatOptions::default(), &MetadataOptions::default()).map_err(|e| e.to_string())?.format;
    let track = format.tracks().iter().find(|track| track.codec_params.codec != codecs::CODEC_TYPE_NULL).ok_or("no sound")?;
    let (id, mut rate) = (track.id, track.codec_params.sample_rate.unwrap_or(48000));
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &codecs::DecoderOptions::default()).map_err(|e| e.to_string())?;
    let (mut samples, mut buffer) = (Vec::new(), None::<SampleBuffer<i16>>);
    while let Ok(packet) = format.next_packet() {
        // (a damaged packet is left out; the rest plays)
        let Some(decoded) = (packet.track_id() == id).then(|| decoder.decode(&packet).ok()).flatten() else { continue };
        let (spec, frames) = (*decoded.spec(), decoded.capacity() as u64);
        rate = spec.rate;
        let buffer = buffer.get_or_insert_with(|| SampleBuffer::new(frames, spec));
        buffer.copy_interleaved_ref(decoded);
        let channels = spec.channels.count().max(1);
        for frame in buffer.samples().chunks(channels) {
            samples.extend([frame[0], frame[(channels > 1) as usize]]);
        }
    }
    Ok((Arc::new(samples), rate))
}

/// Decodes the pictures in the order they are shown, until the video is let go of (or, not looped, to its end).
fn decode_pictures(mut reader: mp4::Mp4Reader<BufReader<std::fs::File>>, track: u32, count: u32, looped: bool, frames: &mpsc::SyncSender<Frame>, spares: &mpsc::Receiver<Vec<u8>>) -> Result<(), String> {
    use openh264::formats::YUVSource;
    let avc = reader.tracks()[&track].trak.mdia.minf.stbl.stsd.avc1.as_ref().ok_or("no H.264 settings")?.avcc.clone();
    // the mp4's NAL units go to OpenH264 as a byte stream: each behind a start code instead of its length
    let mut stream = Vec::new();
    let nal = |stream: &mut Vec<u8>, unit: &[u8]| stream.extend([0, 0, 0, 1].iter().chain(unit));
    let send = |picture: &openh264::decoder::DecodedYUV| {
        let (width, height) = picture.dimensions();
        let mut pixels = spares.try_recv().unwrap_or_default();
        pixels.resize(width * height * 4, 0);
        picture.write_rgba8(&mut pixels);
        frames.send((width, height, pixels)).is_ok()
    };
    loop {
        // flushing after each picture would lose the pictures B-frames need: they come out once the decoder has them
        let config = openh264::decoder::DecoderConfig::new().flush_after_decode(openh264::decoder::Flush::NoFlush);
        let mut decoder = openh264::decoder::Decoder::with_api_config(openh264::OpenH264API::from_source(), config).map_err(|e| e.to_string())?;
        stream.clear();
        for set in avc.sequence_parameter_sets.iter().chain(&avc.picture_parameter_sets) {
            nal(&mut stream, &set.bytes);
        }
        decoder.decode(&stream).map_err(|e| e.to_string())?;
        for index in 1..=count {
            let Some(sample) = reader.read_sample(track, index).map_err(|e| e.to_string())? else { continue };
            stream.clear();
            let mut rest = &sample.bytes[..];
            while let Some((length, after)) = rest.split_first_chunk::<4>() {
                let length = (u32::from_be_bytes(*length) as usize).min(after.len());
                nal(&mut stream, &after[..length]);
                rest = &after[length..];
            }
            // a picture that cannot be decoded is left out; the next ones may still be
            match decoder.decode(&stream) {
                Ok(Some(picture)) if !send(&picture) => return Ok(()),
                Err(error) => eprintln!("Video picture {index}: {error}"),
                _ => {}
            }
        }
        for picture in decoder.flush_remaining().map_err(|e| e.to_string())? {
            if !send(&picture) {
                return Ok(());
            }
        }
        if !looped {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_picture_and_the_sound_come_out() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/base_game/videos/2hotCutscene.mp4");
        let video = super::Video::open(&path, true, true, false, false).unwrap();
        let (samples, rate) = video.sound.recv().unwrap();
        assert_eq!(rate, 48000);
        assert!(samples.len() / 2 > 48000 * 28, "28 seconds of stereo sound");
        assert!((video.frame_seconds - 1.0 / 24.0).abs() < 1e-3);
        let mut count = 0;
        for (width, height, pixels) in video.frames.iter() {
            assert_eq!((width, height, pixels.len()), (1280, 720, 1280 * 720 * 4));
            count += 1;
        }
        // (B-frames: lost if the decoder were flushed after each one)
        assert_eq!(count as f64, (28.4166 / video.frame_seconds).round());
    }
}
