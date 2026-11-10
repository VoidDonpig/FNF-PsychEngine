package backend;

/**
 * Shows each frame through a DXGI flip-model swap chain (native/psych_input/src/present.rs).
 * Windows can put such a frame on screen without the desktop compositor
 * ("Hardware: Independent Flip"), which OpenGL's own presentation never gets in a window.
 *
 * Only on Windows builds with the Rust library (see RustBuild). Everywhere else, and when the
 * graphics driver cannot do it, present() answers false.
 */
#if RUST_INPUT
@:cppFileCode('extern "C" { int psych_present(int exclusive); int psych_present_error(); int psych_present_hresult(); }')
#end
class FlipPresent
{
	/**
	 * Presents the frame OpenGL just rendered. False when the caller has to swap the OpenGL buffers itself.
	 * `exclusive`: take the display for the game alone (exclusive fullscreen) while its window is in front.
	 **/
	public static function present(exclusive:Bool):Bool
	{
		#if RUST_INPUT
		return untyped __cpp__('psych_present({0}) != 0', exclusive ? 1 : 0);
		#else
		return false;
		#end
	}

	/** Why present() answered false: the step that failed (listed in present.rs) and its Windows error code. **/
	public static function error():String
	{
		#if RUST_INPUT
		var step:Int = untyped __cpp__('psych_present_error()');
		var hresult:Int = untyped __cpp__('psych_present_hresult()');
		return 'step $step, error 0x' + StringTools.hex(hresult, 8);
		#else
		return 'not in this build';
		#end
	}
}
