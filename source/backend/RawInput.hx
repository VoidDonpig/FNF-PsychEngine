package backend;

/**
 * Keyboard events stamped with the time they really happened, read by the Rust raw input
 * thread (native/psych_input). OpenFL only hands over key events once per frame, so without
 * this a press is judged up to a whole frame away from when the player made it.
 *
 * Native only on Windows builds where the library was compiled (see RustBuild). Everywhere
 * else nothing is delivered and gameplay keeps using OpenFL key events.
 */
#if RUST_INPUT
@:cppFileCode('
extern "C" {
	int psych_rawinput_start();
	int psych_rawinput_next();
	double psych_rawinput_age_ms();
	int psych_rawinput_covers(int code);
}
')
#end
class RawInput
{
	// Song time (ms) paired with the clock time (ms) it was current at
	static var songTime:Float = 0;
	static var clockTime:Float = 0;
	static var frameClock:Float = 0;
	static var synced:Bool = false;
	static var started:Bool = false;

	/**
	 * How far behind the song the picture on screen is on average, in ms: the time from the start of
	 * a frame until it is shown, plus half the time it then stays up. Smoothed over the last frames.
	 * With the "Display Lag Correction" option, eventTime() judges keys against that picture.
	 **/
	public static var displayLag(default, null):Float = 0;
	static var lastPresent:Float = 0;
	static var frameOpen:Bool = false;

	/** Starts the capture thread (safe to call again) and forgets keys pressed before gameplay. **/
	public static function start():Void
	{
		#if RUST_INPUT
		untyped __cpp__('psych_rawinput_start()');
		while (next() >= 0) {}
		frameStart();

		if (started) return;
		started = true;
		// No frame runs while the window is in the background: song time stood still until now
		FlxG.signals.focusGained.add(function() clockTime = haxe.Timer.stamp() * 1000);
		#end
	}

	/** Whether events of this key code come from next(). When they do, ignore the OpenFL event of the same key. **/
	public static function covers(keyCode:Int):Bool
	{
		#if RUST_INPUT
		return untyped __cpp__('psych_rawinput_covers({0}) != 0', keyCode);
		#else
		return false;
		#end
	}

	/** Next key event as `keyCode | (pressed ? 0x10000 : 0)`, or -1 when there is none. **/
	public static function next():Int
	{
		#if RUST_INPUT
		return untyped __cpp__('psych_rawinput_next()');
		#else
		return -1;
		#end
	}

	/** Call once per frame, before anything updates, after reading the events with next(). **/
	public static function frameStart():Void
	{
		#if RUST_INPUT
		frameClock = haxe.Timer.stamp() * 1000;
		// Nobody called sync() last frame (paused): song time stood still, only the clock moved
		if (!synced) clockTime = frameClock;
		synced = false;
		frameOpen = true;
		#end
	}

	/** Call when a frame has just been put on screen. **/
	public static function framePresented():Void
	{
		#if RUST_INPUT
		var now:Float = haxe.Timer.stamp() * 1000;
		// Only frames that started with frameStart(): outside gameplay there is nothing to measure against
		if (frameOpen)
		{
			var lag:Float = (now - frameClock) + (now - lastPresent) * 0.5;
			if (lag < 500) displayLag += (lag - displayLag) * 0.2; // a longer one is a loading stall, not a frame
		}
		frameOpen = false;
		lastPresent = now;
		#end
	}

	/** Tells which song time (ms) this frame started at. Call every frame the song is not paused. **/
	public static function sync(songTime:Float):Void
	{
		#if RUST_INPUT
		RawInput.songTime = songTime;
		clockTime = frameClock;
		synced = true;
		#end
	}

	/** Song time (ms) at which the event next() just returned happened. **/
	public static function eventTime(playbackRate:Float):Float
	{
		#if RUST_INPUT
		var age:Float = untyped __cpp__('psych_rawinput_age_ms()');
		var time:Float = songTime + (haxe.Timer.stamp() * 1000 - age - clockTime) * playbackRate;
		// The player answers to the picture, and the picture is displayLag behind the song
		if (ClientPrefs.data.displayLagCorrection) time -= displayLag * playbackRate;
		return time;
		#else
		return songTime;
		#end
	}
}
