//! Backend-neutral interactive REPL loop (`BufRead` + `Write` + interrupt flag).

pub mod engine;
#[cfg(feature = "vm")]
pub mod vm_engine;

use std::io::{self, BufRead, Write};
use std::mem;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use engine::{EngineResponse, ReplEngine};

/// Shared cancellation flag toggled by `SIGINT` or programmatic test harnesses.
pub type InterruptHandle = Arc<AtomicBool>;

/// Runs the REPL read-eval-print loop over `reader`, writing output to `stdout`
/// and diagnostics/status to `stderr`.
pub fn run_loop<E, R, WOut, WErr>(
    engine: &mut E,
    mut reader: R,
    stdout: &mut WOut,
    stderr: &mut WErr,
    interactive: bool,
    interrupt: &InterruptHandle,
) -> io::Result<()>
where
    E: ReplEngine,
    R: BufRead,
    WOut: Write,
    WErr: Write,
{
    let mut buffer = String::new();
    let mut consecutive_ctrl_c: u8 = 0;
    let mut pending_line: Option<String> = None;

    loop {
        let raw_line = if let Some(line) = pending_line.take() {
            line
        } else {
            if interactive {
                let prompt = if buffer.is_empty() { "flick> " } else { ".. " };
                write!(stdout, "{prompt}")?;
                stdout.flush()?;
            }
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    if !buffer.trim().is_empty() {
                        let phrase = mem::take(&mut buffer);
                        if !dispatch_input(engine, &phrase, stdout, stderr, interrupt)? {
                            break;
                        }
                    }
                    break;
                }
                Ok(_) => line,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                    interrupt.store(false, Ordering::Release);
                    if !buffer.is_empty() {
                        buffer.clear();
                        consecutive_ctrl_c = 0;
                        writeln!(stderr, "^C")?;
                        continue;
                    }
                    consecutive_ctrl_c = consecutive_ctrl_c.saturating_add(1);
                    if consecutive_ctrl_c >= 2 {
                        break;
                    }
                    writeln!(stderr, "press Ctrl-C again or `:quit` to exit")?;
                    continue;
                }
                Err(err) => return Err(err),
            }
        };

        if interrupt.swap(false, Ordering::AcqRel)
            && buffer.is_empty()
            && raw_line.trim().is_empty()
        {
            consecutive_ctrl_c = consecutive_ctrl_c.saturating_add(1);
            if consecutive_ctrl_c >= 2 {
                break;
            }
            writeln!(stderr, "press Ctrl-C again or `:quit` to exit")?;
            continue;
        }

        if !process_line(
            engine,
            raw_line,
            &mut buffer,
            &mut pending_line,
            &mut consecutive_ctrl_c,
            stdout,
            stderr,
            interrupt,
        )? {
            break;
        }
    }

    if let Err(err) = engine.shutdown() {
        writeln!(stderr, "error: shutdown failed: {err}")?;
    }
    Ok(())
}

#[expect(clippy::too_many_arguments, reason = "REPL loop line handler state")]
fn process_line<E, WOut, WErr>(
    engine: &mut E,
    raw_line: String,
    buffer: &mut String,
    pending_line: &mut Option<String>,
    consecutive_ctrl_c: &mut u8,
    stdout: &mut WOut,
    stderr: &mut WErr,
    interrupt: &InterruptHandle,
) -> io::Result<bool>
where
    E: ReplEngine,
    WOut: Write,
    WErr: Write,
{
    let trimmed = raw_line.trim();
    if buffer.is_empty() {
        if trimmed.is_empty() {
            return Ok(true);
        }
        *consecutive_ctrl_c = 0;
        if trimmed.starts_with(':') && !needs_more_lines(&raw_line) {
            return dispatch_command(engine, trimmed, stdout, stderr, interrupt);
        }
        buffer.push_str(&raw_line);
        if !needs_more_lines(buffer) {
            let phrase = mem::take(buffer);
            return dispatch_input(engine, &phrase, stdout, stderr, interrupt);
        }
        return Ok(true);
    }

    let waiting_indent = delimiters_balanced(buffer) && ends_with_indent_opener(buffer);
    let is_continuation =
        raw_line.starts_with(' ') || raw_line.starts_with('\t') || trimmed.starts_with('|');
    if waiting_indent && !trimmed.is_empty() && !is_continuation {
        let phrase = mem::take(buffer);
        if !dispatch_input(engine, &phrase, stdout, stderr, interrupt)? {
            return Ok(false);
        }
        *pending_line = Some(raw_line);
        return Ok(true);
    }
    if trimmed.is_empty() && delimiters_balanced(buffer) {
        let phrase = mem::take(buffer);
        return dispatch_input(engine, &phrase, stdout, stderr, interrupt);
    }
    buffer.push_str(&raw_line);
    if !needs_more_lines(buffer) {
        let phrase = mem::take(buffer);
        return dispatch_input(engine, &phrase, stdout, stderr, interrupt);
    }
    Ok(true)
}

fn dispatch_input<E, WOut, WErr>(
    engine: &mut E,
    phrase: &str,
    stdout: &mut WOut,
    stderr: &mut WErr,
    interrupt: &InterruptHandle,
) -> io::Result<bool>
where
    E: ReplEngine,
    WOut: Write,
    WErr: Write,
{
    let trimmed = phrase.trim();
    if trimmed.is_empty() {
        return Ok(true);
    }
    if trimmed.starts_with(':') {
        return dispatch_command(engine, trimmed, stdout, stderr, interrupt);
    }
    interrupt.store(false, Ordering::Release);
    let response = engine.eval(trimmed, Arc::clone(interrupt));
    interrupt.store(false, Ordering::Release);
    write_response(response, stdout, stderr)?;
    Ok(true)
}

fn dispatch_command<E, WOut, WErr>(
    engine: &mut E,
    line: &str,
    stdout: &mut WOut,
    stderr: &mut WErr,
    interrupt: &InterruptHandle,
) -> io::Result<bool>
where
    E: ReplEngine,
    WOut: Write,
    WErr: Write,
{
    let (cmd, rest) = match line.find(char::is_whitespace) {
        Some(idx) => (&line[..idx], line[idx..].trim()),
        None => (line, ""),
    };
    match cmd {
        ":quit" | ":q" => return Ok(false),
        ":help" | ":h" => {
            writeln!(
                stdout,
                "commands:\n  \
                 :handle <expr>   install a session-wide effect handler\n  \
                 :provide <expr>  install a session-wide service layer\n  \
                 :bg <expr>       run a task in the background\n  \
                 :jobs            list background jobs\n  \
                 :kill <n>        stop background job <n> with finalization\n  \
                 :quit / :q       exit the REPL"
            )?;
            stdout.flush()?;
        }
        ":handle" => {
            if rest.is_empty() {
                writeln!(stderr, "usage: :handle <handler expr>")?;
            } else {
                interrupt.store(false, Ordering::Release);
                let outcome = engine.handle(rest, Arc::clone(interrupt));
                interrupt.store(false, Ordering::Release);
                write_response(outcome, stdout, stderr)?;
            }
        }
        ":provide" => {
            if rest.is_empty() {
                writeln!(stderr, "usage: :provide <layer expr>")?;
            } else {
                interrupt.store(false, Ordering::Release);
                let outcome = engine.provide(rest, Arc::clone(interrupt));
                interrupt.store(false, Ordering::Release);
                write_response(outcome, stdout, stderr)?;
            }
        }
        ":bg" => {
            if rest.is_empty() {
                writeln!(stderr, "usage: :bg <task expr>")?;
            } else {
                let outcome = engine.spawn_bg(rest);
                write_response(outcome, stdout, stderr)?;
            }
        }
        ":jobs" => {
            let jobs = engine.list_jobs();
            if jobs.is_empty() {
                writeln!(stdout, "(no background jobs)")?;
            } else {
                for job in jobs {
                    writeln!(stdout, "{job}")?;
                }
            }
            stdout.flush()?;
        }
        ":kill" => match rest.parse::<u64>() {
            Ok(id) => {
                let outcome = engine.kill_job(id);
                write_response(outcome, stdout, stderr)?;
            }
            Err(_) => {
                writeln!(stderr, "usage: :kill <job-id>")?;
            }
        },
        other => {
            writeln!(stderr, "unknown command `{other}` (try `:help`)")?;
        }
    }
    Ok(true)
}

fn write_response<WOut: Write, WErr: Write>(
    response: EngineResponse,
    stdout: &mut WOut,
    stderr: &mut WErr,
) -> io::Result<()> {
    match response {
        EngineResponse::Output {
            stdout: captured,
            result,
        } => {
            write_captured(&captured, stdout)?;
            if let Some(line) = result {
                writeln!(stdout, "{line}")?;
            }
            stdout.flush()?;
        }
        EngineResponse::TypedError {
            stdout: captured,
            message,
        } => {
            write_captured(&captured, stdout)?;
            writeln!(stderr, "error: {message}")?;
            stderr.flush()?;
        }
        EngineResponse::UnhandledEffect {
            stdout: captured,
            hint,
            ..
        } => {
            write_captured(&captured, stdout)?;
            writeln!(stderr, "{hint}")?;
            stderr.flush()?;
        }
        EngineResponse::Interrupted { stdout: captured } => {
            write_captured(&captured, stdout)?;
            writeln!(stderr, "interrupted")?;
            stderr.flush()?;
        }
        EngineResponse::Error {
            stdout: captured,
            message,
        } => {
            write_captured(&captured, stdout)?;
            writeln!(stderr, "{message}")?;
            stderr.flush()?;
        }
    }
    Ok(())
}

fn write_captured<W: Write>(captured: &str, out: &mut W) -> io::Result<()> {
    if !captured.is_empty() {
        out.write_all(captured.as_bytes())?;
        if !captured.ends_with('\n') {
            out.write_all(b"\n")?;
        }
        out.flush()?;
    }
    Ok(())
}

fn needs_more_lines(text: &str) -> bool {
    !delimiters_balanced(text) || ends_with_indent_opener(text)
}

fn ends_with_indent_opener(text: &str) -> bool {
    let trimmed = text.trim_end();
    trimmed.ends_with('=') || trimmed.ends_with("with")
}

fn delimiters_balanced(text: &str) -> bool {
    let mut parens = 0i32;
    let mut brackets = 0i32;
    let mut braces = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' => parens += 1,
            ')' => parens -= 1,
            '[' => brackets += 1,
            ']' => brackets -= 1,
            '{' => braces += 1,
            '}' => braces -= 1,
            _ => {}
        }
    }
    !in_string && parens <= 0 && brackets <= 0 && braces <= 0
}
