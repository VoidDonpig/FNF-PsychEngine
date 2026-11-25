//! A file chosen through the system's own dialog (the Haxe editor's FileDialogHandler), by the tools the system has:
//! zenity, else kdialog, on Linux; osascript on macOS; PowerShell's OpenFileDialog, SaveFileDialog and
//! FolderBrowserDialog on Windows.
//! The tool runs beside the game, which keeps drawing: the editor asks `Pending::answer` each frame. What is asked
//! (the title, the folder, the file's name) is handed to the tool as arguments of its own or in its environment, never
//! written into a script, so a name with quotes or `$()` in it is only a name.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// How a dialog ended.
#[derive(Debug, PartialEq)]
pub enum Answer {
    Chosen(PathBuf),
    Cancelled,
    /// The tool started but did not work (no display, an error): the editor asks its own way instead.
    Failed,
}

/// A dialog that is up.
pub struct Pending(Receiver<Answer>);

impl Pending {
    /// How it ended, once it has.
    pub fn answer(&self) -> Option<Answer> {
        match self.0.try_recv() {
            Ok(answer) => Some(answer),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Answer::Failed),
        }
    }
}

/// A file to open, of the `endings` given ("json"), from `folder`; None when there is no dialog here.
pub fn open(title: &str, endings: &[&str], folder: &Path) -> Option<Pending> {
    start(&Ask { title, folder, save: None, endings, directory: false })
}

/// A folder (to save files in), from `folder`; None when there is no dialog here.
pub fn folder(title: &str, folder: &Path) -> Option<Pending> {
    start(&Ask { title, folder, save: None, endings: &[], directory: true })
}

/// Where to save a file, `name` offered, in `folder`; None when there is no dialog here.
pub fn save(title: &str, name: &str, folder: &Path) -> Option<Pending> {
    start(&Ask { title, folder, save: Some(name), endings: &[], directory: false })
}

/// What is asked: a file to open (`save` None), where to save the one named, or a folder (`directory`).
struct Ask<'a> {
    title: &'a str,
    folder: &'a Path,
    save: Option<&'a str>,
    endings: &'a [&'a str],
    directory: bool,
}

/// A tool to run: its arguments and what it is given in its environment.
#[derive(Debug)]
struct Run {
    program: &'static str,
    args: Vec<OsString>,
    env: Vec<(&'static str, OsString)>,
}

/// An argument of `--option=value` shape (one argument: the value is not read as options).
fn joined(option: &str, value: impl AsRef<std::ffi::OsStr>) -> OsString {
    let mut arg = OsString::from(option);
    arg.push(value);
    arg
}

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn zenity(ask: &Ask) -> Run {
    let mut args = vec![OsString::from("--file-selection"), joined("--title=", ask.title)];
    if ask.directory {
        args.extend([OsString::from("--directory"), joined("--filename=", ask.folder.join(""))]);
        return Run { program: "zenity", args, env: Vec::new() };
    }
    match ask.save {
        Some(name) => args.extend([OsString::from("--save"), joined("--filename=", ask.folder.join(name))]),
        None => {
            // (a folder's path ending in / starts there)
            args.push(joined("--filename=", ask.folder.join("")));
            if !ask.endings.is_empty() {
                let patterns: Vec<String> = ask.endings.iter().map(|ending| format!("*.{ending}")).collect();
                args.push(joined("--file-filter=", format!("{} | {}", patterns.join(" "), patterns.join(" "))));
            }
            args.push(OsString::from("--file-filter=All files | *"));
        }
    }
    Run { program: "zenity", args, env: Vec::new() }
}

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn kdialog(ask: &Ask) -> Run {
    let mut args = vec![OsString::from("--title"), OsString::from(ask.title)];
    if ask.directory {
        args.extend([OsString::from("--getexistingdirectory"), ask.folder.as_os_str().to_owned()]);
        return Run { program: "kdialog", args, env: Vec::new() };
    }
    match ask.save {
        Some(name) => args.extend([OsString::from("--getsavefilename"), ask.folder.join(name).into_os_string()]),
        None => {
            args.extend([OsString::from("--getopenfilename"), ask.folder.as_os_str().to_owned()]);
            let patterns: Vec<String> = ask.endings.iter().map(|ending| format!("*.{ending}")).collect();
            args.push(OsString::from(if patterns.is_empty() { "*".to_owned() } else { patterns.join(" ") }));
        }
    }
    Run { program: "kdialog", args, env: Vec::new() }
}

/// The script reads what is asked from its arguments (`argv`): the title, the folder, the name offered, the endings.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn osascript(ask: &Ask) -> Run {
    let choose = match ask.save {
        None if ask.directory => "set chosen to choose folder with prompt (item 1 of argv) default location ((POSIX file (item 2 of argv)) as alias)",
        Some(_) => "set chosen to choose file name with prompt (item 1 of argv) default name (item 3 of argv) default location ((POSIX file (item 2 of argv)) as alias)",
        // (the endings come as one argument, split at spaces; with none, every file is offered)
        None if ask.endings.is_empty() => "set chosen to choose file with prompt (item 1 of argv) default location ((POSIX file (item 2 of argv)) as alias)",
        None => "set chosen to choose file with prompt (item 1 of argv) default location ((POSIX file (item 2 of argv)) as alias) of type (words of (item 4 of argv))",
    };
    let mut args = Vec::new();
    for line in ["on run argv", choose, "return POSIX path of chosen", "end run"] {
        args.extend([OsString::from("-e"), OsString::from(line)]);
    }
    args.extend([OsString::from(ask.title), ask.folder.as_os_str().to_owned(), OsString::from(ask.save.unwrap_or("")), OsString::from(ask.endings.join(" "))]);
    Run { program: "osascript", args, env: Vec::new() }
}

/// The script reads what is asked from its environment; it writes the file chosen, nothing when cancelled.
#[cfg_attr(not(windows), allow(dead_code))]
fn powershell(ask: &Ask) -> Run {
    let kind = if ask.save.is_some() { "SaveFileDialog" } else { "OpenFileDialog" };
    let script = if ask.directory {
        // (the folder chosen is its SelectedPath)
        "trap { exit 2 }; Add-Type -AssemblyName System.Windows.Forms; [Console]::OutputEncoding = [Text.Encoding]::UTF8; \
         $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = $env:FUNKIN_DIALOG_TITLE; $d.SelectedPath = $env:FUNKIN_DIALOG_FOLDER; \
         if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::Out.Write($d.SelectedPath) }".to_owned()
    } else { format!(
        "trap {{ exit 2 }}; Add-Type -AssemblyName System.Windows.Forms; [Console]::OutputEncoding = [Text.Encoding]::UTF8; \
         $d = New-Object System.Windows.Forms.{kind}; $d.Title = $env:FUNKIN_DIALOG_TITLE; $d.InitialDirectory = $env:FUNKIN_DIALOG_FOLDER; \
         $d.FileName = $env:FUNKIN_DIALOG_NAME; $d.Filter = $env:FUNKIN_DIALOG_FILTER; \
         if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ [Console]::Out.Write($d.FileName) }}"
    ) };
    let patterns: Vec<String> = ask.endings.iter().map(|ending| format!("*.{ending}")).collect();
    let mut filter = if patterns.is_empty() { String::new() } else { format!("{0}|{0}|", patterns.join(";")) };
    filter.push_str("All files (*.*)|*.*");
    let env = vec![
        ("FUNKIN_DIALOG_TITLE", OsString::from(ask.title)),
        ("FUNKIN_DIALOG_FOLDER", ask.folder.as_os_str().to_owned()),
        ("FUNKIN_DIALOG_NAME", OsString::from(ask.save.unwrap_or(""))),
        ("FUNKIN_DIALOG_FILTER", OsString::from(filter)),
    ];
    let args = ["-NoProfile", "-NonInteractive", "-STA", "-Command", &script].map(OsString::from).to_vec();
    Run { program: "powershell", args, env }
}

/// The tools to try, in turn, on this system.
fn runs(ask: &Ask) -> Vec<Run> {
    if cfg!(windows) {
        vec![powershell(ask)]
    } else if cfg!(target_os = "macos") {
        vec![osascript(ask)]
    } else if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        Vec::new()
    } else {
        vec![zenity(ask), kdialog(ask)]
    }
}

/// The first tool that starts, waited for on a thread of its own.
fn start(ask: &Ask) -> Option<Pending> {
    for run in runs(ask) {
        let mut command = Command::new(run.program);
        command.args(&run.args).envs(run.env.iter().map(|(name, value)| (name, value))).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            // (no console window comes up with it)
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let Ok(child) = command.spawn() else { continue };
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(answer_of(child.wait_with_output()));
        });
        return Some(Pending(receive));
    }
    None
}

/// Whether what a tool said on its error output is about not reaching a display or desktop (zenity: "cannot open display",
/// kdialog: "cannot connect to X server"), which is not a cancel though the tool ends with 1 then as it does for one.
fn no_display(stderr: &[u8]) -> bool {
    let said = String::from_utf8_lossy(stderr).to_lowercase();
    ["cannot open display", "cannot connect", "could not connect", "unable to init", "failed to connect", "no display", "no protocol specified"].iter().any(|sign| said.contains(sign))
}

/// The path the tool wrote (its last line ending taken off); cancelled when it wrote none or ended with 1, as they
/// all do when cancelled, unless its error output says it could not reach a display.
fn answer_of(output: std::io::Result<Output>) -> Answer {
    let Ok(output) = output else { return Answer::Failed };
    let mut bytes = output.stdout;
    while bytes.last().is_some_and(|byte| *byte == b'\n' || *byte == b'\r') {
        bytes.pop();
    }
    match output.status.code() {
        Some(0) if !bytes.is_empty() => Answer::Chosen(path_of(bytes)),
        Some(1) if no_display(&output.stderr) => Answer::Failed,
        Some(0 | 1) => Answer::Cancelled,
        _ => Answer::Failed,
    }
}

#[cfg(unix)]
fn path_of(bytes: Vec<u8>) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(OsString::from_vec(bytes))
}

#[cfg(not(unix))]
fn path_of(bytes: Vec<u8>) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTILE: &str = "a\" '$(touch x)`id`; rm -rf ~ & echo \"b.json";

    fn asks(folder: &Path) -> [Ask<'_>; 2] {
        [Ask { title: HOSTILE, folder, save: Some(HOSTILE), endings: &[], directory: false }, Ask { title: HOSTILE, folder, save: None, endings: &["json"], directory: false }]
    }

    fn count(run: &Run, value: &str) -> usize {
        run.args.iter().filter(|arg| arg.to_string_lossy().contains(value)).count()
    }

    #[test]
    fn zenity_and_kdialog_get_names_as_one_argument_each() {
        let folder = Path::new("/songs/it's $(here)");
        let [save, open] = asks(folder);
        let run = zenity(&save);
        assert!(run.args.contains(&joined("--title=", HOSTILE)));
        assert!(run.args.contains(&joined("--filename=", folder.join(HOSTILE))));
        assert!(run.args.contains(&OsString::from("--save")));
        let run = zenity(&open);
        assert!(run.args.contains(&joined("--filename=", folder.join(""))));
        assert!(run.args.contains(&OsString::from("--file-filter=*.json | *.json")));
        let run = kdialog(&save);
        assert_eq!(run.args, [OsString::from("--title"), HOSTILE.into(), "--getsavefilename".into(), folder.join(HOSTILE).into()]);
        let run = kdialog(&open);
        assert_eq!(run.args, ["--title", HOSTILE, "--getopenfilename", "/songs/it's $(here)", "*.json"].map(OsString::from));
    }

    #[test]
    fn scripts_read_names_from_outside_them() {
        let folder = Path::new("/songs/it's $(here)");
        let [save, open] = asks(folder);
        for ask in [&save, &open] {
            let run = osascript(ask);
            // the script lines hold nothing asked; the title, folder and name follow them, one argument each
            let tail = &run.args[run.args.len() - 4..];
            assert_eq!(tail[0], OsString::from(HOSTILE));
            assert_eq!(tail[1], folder.as_os_str());
            assert_eq!(tail[3], OsString::from(ask.endings.join(" ")));
            assert_eq!(count(&run, "here"), 1);
            assert_eq!(count(&run, "touch"), if ask.save.is_some() { 2 } else { 1 });
            let run = powershell(ask);
            assert_eq!(count(&run, "touch"), 0);
            assert_eq!(count(&run, "here"), 0);
            assert!(run.env.contains(&("FUNKIN_DIALOG_TITLE", OsString::from(HOSTILE))));
            assert!(run.env.contains(&("FUNKIN_DIALOG_FOLDER", folder.as_os_str().to_owned())));
        }
        assert!(powershell(&save).env.contains(&("FUNKIN_DIALOG_NAME", OsString::from(HOSTILE))));
        assert!(powershell(&save).args.iter().any(|arg| arg.to_string_lossy().contains("SaveFileDialog")));
        assert!(powershell(&open).env.contains(&("FUNKIN_DIALOG_FILTER", OsString::from("*.json|*.json|All files (*.*)|*.*"))));
    }

    #[test]
    fn open_dialogs_offer_only_the_endings_asked() {
        let ask = Ask { title: "t", folder: Path::new("/f"), save: None, endings: &["json", "png"], directory: false };
        assert!(osascript(&ask).args.iter().any(|arg| arg.to_string_lossy().ends_with("of type (words of (item 4 of argv))")));
        assert_eq!(osascript(&ask).args.last(), Some(&OsString::from("json png")));
        assert!(zenity(&ask).args.contains(&OsString::from("--file-filter=*.json *.png | *.json *.png")));
        assert_eq!(kdialog(&ask).args.last(), Some(&OsString::from("*.json *.png")));
        assert!(powershell(&ask).env.contains(&("FUNKIN_DIALOG_FILTER", OsString::from("*.json;*.png|*.json;*.png|All files (*.*)|*.*"))));
        let any = Ask { endings: &[], ..ask };
        assert!(!osascript(&any).args.iter().any(|arg| arg.to_string_lossy().contains("of type")));
    }

    #[test]
    fn folders_are_asked_for_with_the_same_care() {
        let folder = Path::new("/songs/it's $(here)");
        let ask = Ask { title: HOSTILE, folder, save: None, endings: &[], directory: true };
        assert_eq!(zenity(&ask).args, [OsString::from("--file-selection"), joined("--title=", HOSTILE), OsString::from("--directory"), joined("--filename=", folder.join(""))]);
        assert_eq!(kdialog(&ask).args, ["--title", HOSTILE, "--getexistingdirectory", "/songs/it's $(here)"].map(OsString::from));
        let run = osascript(&ask);
        assert!(count(&run, "choose folder") == 1 && count(&run, "touch") == 1 && count(&run, "here") == 1);
        let run = powershell(&ask);
        assert!(count(&run, "FolderBrowserDialog") == 1 && count(&run, "touch") == 0 && count(&run, "here") == 0);
        assert!(run.env.contains(&("FUNKIN_DIALOG_FOLDER", folder.as_os_str().to_owned())));
    }

    #[cfg(unix)]
    #[test]
    fn what_a_tool_wrote_is_the_answer() {
        use std::os::unix::process::ExitStatusExt;
        let output = |code: i32, out: &str| Ok(Output { status: std::process::ExitStatus::from_raw(code << 8), stdout: out.as_bytes().to_vec(), stderr: Vec::new() });
        assert_eq!(answer_of(output(0, "/a b/c.json\n")), Answer::Chosen(PathBuf::from("/a b/c.json")));
        assert_eq!(answer_of(output(0, "")), Answer::Cancelled);
        assert_eq!(answer_of(output(1, "")), Answer::Cancelled);
        assert_eq!(answer_of(output(2, "")), Answer::Failed);
        let said = |words: &str| Ok(Output { status: std::process::ExitStatus::from_raw(1 << 8), stdout: Vec::new(), stderr: words.as_bytes().to_vec() });
        assert_eq!(answer_of(said("Gtk-WARNING **: cannot open display: ")), Answer::Failed);
        assert_eq!(answer_of(said("kdialog: Cannot connect to X server :0")), Answer::Failed);
        assert_eq!(answer_of(said("")), Answer::Cancelled);
        assert_eq!(answer_of(said("execution error: User canceled. (-128)")), Answer::Cancelled);
    }
}
