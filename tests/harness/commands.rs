//! Children for portable process fixtures. These return the first two Rune
//! arguments to a host process call; the caller still chooses its deadline,
//! input, and assertions. Unix keeps the original utilities. Windows byte
//! fixtures use raw .NET streams, never PowerShell's text pipeline or encoding.
//!
//! Shared by more than one test binary, and each of them uses part of it, so
//! what is unused in one is live in another: the dead-code warning would be
//! about the file's readers rather than about the file.
#![allow(dead_code)]

fn arguments(program: &str, args: &[&str]) -> String {
	format!(
		"{}, {}",
		serde_json::to_string(program).unwrap(),
		serde_json::to_string(args).unwrap()
	)
}

/// Expand named fixture children in Rune templates without treating Rune's
/// braces as Rust format strings. Only used by the three process test files;
/// these names describe a child, not a shell language to translate.
pub fn expand(source: &str) -> String {
	type Fixture = (&'static str, fn() -> String);
	let fixtures: &[Fixture] = &[
		("@CAT_DATA@", || cat("data")),
		("@DATA_ON_STDERR@", || streams(None, "data", false)),
		("@BIG_AND_PARTIAL@", || {
			streams(Some("big"), "partial", false)
		}),
		("@PARTIAL_AND_BIG@", || {
			streams(Some("partial"), "big", false)
		}),
		("@BIG_AND_FINE@", || streams(Some("big"), "fine", true)),
		("@EXIT_7@", || exit(7)),
		("@FAIL@", || exit(1)),
		("@SUCCEED@", || exit(0)),
		("@SLEEP_5@", || sleep(5)),
		("@PRESSURE@", pressure),
		("@FIRST_LINE@", first_line),
		("@COPY_STDIN@", copy_stdin),
		("@THREE_MILLION_ZEROS@", || zeros(3_000_000)),
		("@ECHO_TEXT@", || echo("text")),
		("@ECHO_BYTES@", || echo("bytes")),
	];
	let mut expanded = source.to_owned();
	for (name, child) in fixtures {
		if expanded.contains(name) {
			expanded = expanded.replace(name, &child());
		}
	}
	expanded
}

#[cfg(windows)]
fn powershell(script: &str) -> String {
	let executable = std::path::PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot"))
		.join("System32/WindowsPowerShell/v1.0/powershell.exe");
	arguments(
		executable.to_str().expect("PowerShell path must be UTF-8"),
		&[
			"-NoProfile",
			"-NonInteractive",
			"-Command",
			&format!("$ErrorActionPreference = 'Stop'; {script}"),
		],
	)
}

#[cfg(windows)]
fn quoted(text: &str) -> String {
	format!("'{}'", text.replace('\'', "''"))
}

/// A normal exit controlled from outside, without ever reading standard
/// input. Timing out the handshake is a failing child, never exit 0.
#[cfg(windows)]
pub fn exit_when_released(path: &std::path::Path) -> String {
	powershell(&format!(
		"$until = [DateTime]::UtcNow.AddSeconds(20); while (![IO.File]::Exists({})) {{ if ([DateTime]::UtcNow -ge $until) {{ throw 'the exit child was never released' }}; [Threading.Thread]::Sleep(5) }}; exit 0",
		quoted(path.to_str().expect("fixture path must be UTF-8"))
	))
}

pub fn exit(code: u8) -> String {
	#[cfg(unix)]
	return match code {
		0 => arguments("true", &[]),
		1 => arguments("false", &[]),
		_ => arguments("sh", &["-c", &format!("exit {code}")]),
	};
	#[cfg(windows)]
	return arguments("cmd.exe", &["/D", "/C", &format!("exit {code}")]);
}

pub fn sleep(seconds: u32) -> String {
	#[cfg(unix)]
	return arguments("sleep", &[&seconds.to_string()]);
	#[cfg(windows)]
	return arguments("ping", &["-n", &(seconds + 1).to_string(), "127.0.0.1"]);
}

pub fn cat(path: &str) -> String {
	#[cfg(unix)]
	return arguments("cat", &[path]);
	#[cfg(windows)]
	return powershell(&format!(
		"$file = [IO.File]::OpenRead({}); try {{ $file.CopyTo([Console]::OpenStandardOutput()) }} finally {{ $file.Dispose() }}",
		quoted(path)
	));
}

/// An optional file on stdout, then either a file or a literal line on stderr.
pub fn streams(out: Option<&str>, err: &str, literal_error: bool) -> String {
	#[cfg(unix)]
	{
		// The callers supply fixed fixture filenames and a fixed literal word.
		let out = out.map(|p| format!("cat {p}; ")).unwrap_or_default();
		let command = if literal_error { "echo" } else { "cat" };
		arguments("sh", &["-c", &format!("{out}{command} {err} >&2")])
	}
	#[cfg(windows)]
	{
		let out = out.map(|p| format!(
			"$file = [IO.File]::OpenRead({}); try {{ $file.CopyTo([Console]::OpenStandardOutput()) }} finally {{ $file.Dispose() }}; ", quoted(p)
		)).unwrap_or_default();
		let err = if literal_error {
			format!(
				"$bytes = [Text.Encoding]::UTF8.GetBytes({} + [char]10); [Console]::OpenStandardError().Write($bytes, 0, $bytes.Length)",
				quoted(err)
			)
		} else {
			format!(
				"$file = [IO.File]::OpenRead({}); try {{ $file.CopyTo([Console]::OpenStandardError()) }} finally {{ $file.Dispose() }}",
				quoted(err)
			)
		};
		powershell(&format!("{out}{err}"))
	}
}

pub fn echo(text: &str) -> String {
	#[cfg(unix)]
	return arguments("echo", &[text]);
	#[cfg(windows)]
	return powershell(&format!(
		"$bytes = [Text.Encoding]::UTF8.GetBytes({} + [char]10); [Console]::OpenStandardOutput().Write($bytes, 0, $bytes.Length)",
		quoted(text)
	));
}

pub fn copy_stdin() -> String {
	#[cfg(unix)]
	return arguments("cat", &[]);
	#[cfg(windows)]
	return powershell("[Console]::OpenStandardInput().CopyTo([Console]::OpenStandardOutput())");
}

pub fn first_line() -> String {
	#[cfg(unix)]
	return arguments("head", &["-1"]);
	#[cfg(windows)]
	return powershell(
		"$inputStream = [Console]::OpenStandardInput(); $outputStream = [Console]::OpenStandardOutput(); while (($nextByte = $inputStream.ReadByte()) -ne -1) { $outputStream.WriteByte([byte]$nextByte); if ($nextByte -eq 10) { break } }; $outputStream.Flush()",
	);
}

/// A child that needs a Unix shell, and says so.
///
/// The fixtures that use one build an escaped descendant out of `setsid` and
/// job control. Decision 5 rules that unreachable on Windows, so there is no
/// program to substitute: the call is left as it is, and it fails at the spawn
/// on Windows rather than pretending to ask the question.
pub fn shell(script: &str) -> String {
	arguments("sh", &["-c", script])
}

/// The same count of zero bytes on standard output and on standard error.
pub fn zeros_on_both(count: usize) -> String {
	#[cfg(unix)]
	return arguments(
		"sh",
		&[
			"-c",
			&format!("head -c {count} /dev/zero; head -c {count} /dev/zero >&2"),
		],
	);
	#[cfg(windows)]
	return powershell(&format!(
		"$bytes = New-Object byte[] {count}; [Console]::OpenStandardOutput().Write($bytes, 0, $bytes.Length); [Console]::OpenStandardError().Write($bytes, 0, $bytes.Length)"
	));
}

pub fn zeros(count: usize) -> String {
	#[cfg(unix)]
	return arguments("head", &["-c", &count.to_string(), "/dev/zero"]);
	#[cfg(windows)]
	return powershell(&format!(
		"$bytes = New-Object byte[] {count}; [Console]::OpenStandardOutput().Write($bytes, 0, $bytes.Length)"
	));
}

/// Fill both output pipes before draining stdin, then report every input byte.
pub fn pressure() -> String {
	#[cfg(unix)]
	return arguments(
		"sh",
		&[
			"-c",
			"head -c 200000 /dev/zero; head -c 200000 /dev/zero >&2; wc -c",
		],
	);
	#[cfg(windows)]
	return powershell(
		"$bytes = New-Object byte[] 200000; $outputStream = [Console]::OpenStandardOutput(); $outputStream.Write($bytes, 0, $bytes.Length); [Console]::OpenStandardError().Write($bytes, 0, $bytes.Length); $inputStream = [Console]::OpenStandardInput(); $total = 0; while (($count = $inputStream.Read($bytes, 0, $bytes.Length)) -ne 0) { $total += $count }; $tail = [Text.Encoding]::ASCII.GetBytes($total.ToString([Globalization.CultureInfo]::InvariantCulture) + [char]10); $outputStream.Write($tail, 0, $tail.Length)",
	);
}
