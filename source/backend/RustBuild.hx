package backend;

#if macro
import haxe.io.Path;
import haxe.macro.Compiler;
import haxe.macro.Context;
import sys.FileSystem;

/**
 * Compiles the Rust library in native/psych_input and links it into Windows builds.
 * Called from Project.xml. Without cargo, or when its build fails, the game still compiles:
 * RUST_INPUT stays undefined: gameplay reads the keyboard from OpenFL events and frames are
 * presented by OpenGL, as before.
 */
class RustBuild
{
	public static function run():Void
	{
		if (Context.defined('display') || !Context.defined('cpp') || !Context.defined('windows')
			|| Context.defined('mingw') || Sys.systemName() != 'Windows') return;

		var source:String = FileSystem.fullPath(Context.resolvePath('backend/RustBuild.hx'));
		var crate:String = Path.normalize(Path.join([Path.directory(source), '../../native/psych_input']));
		var target:String = (Context.defined('HXCPP_M32') || Context.defined('32bits')) ? 'i686-pc-windows-msvc' : 'x86_64-pc-windows-msvc';
		var lib:String = '$crate/target/$target/release/psych_input.lib';

		var built:Bool = false;
		try
		{
			built = Sys.command('cargo', ['build', '--release', '--target', target, '--manifest-path', '$crate/Cargo.toml']) == 0;
		}
		catch (e:Dynamic) {}

		if (!built || !FileSystem.exists(lib))
		{
			Sys.println('[RustBuild] Could not build native/psych_input for $target (is Rust installed? https://rustup.rs). Raw keyboard input is disabled in this build.');
			return;
		}

		Compiler.define('RUST_INPUT');
		Compiler.addGlobalMetadata('backend.RawInput', '@:buildXml("<target id=\'haxe\'><lib name=\'$lib\'/><lib name=\'user32.lib\'/><lib name=\'opengl32.lib\'/><lib name=\'d3d11.lib\'/></target>")');
	}
}
#end
