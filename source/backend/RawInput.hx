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
		return songTime + (haxe.Timer.stamp() * 1000 - age - clockTime) * playbackRate;
		#else
		return songTime;
		#end
	}
}
