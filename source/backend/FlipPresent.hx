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
@:cppFileCode('extern "C" int psych_present();')
#end
class FlipPresent
{
	/** Presents the frame OpenGL just rendered. False when the caller has to swap the OpenGL buffers itself. **/
	public static function present():Bool
	{
		#if RUST_INPUT
		return untyped __cpp__('psych_present() != 0');
		#else
		return false;
		#end
	}
}
