//! Build phases on stderr, with animation only on an interactive terminal.

use std::cell::RefCell;
use std::env;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};
use std::iter::repeat_n;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anstyle::{AnsiColor, Color, Style};
#[cfg(unix)]
use rustix::termios::tcgetwinsize;

use crate::args::{DiagnosticsFormat, GlobalArguments};
use crate::{report, timings};

const BRAND: Style = Style::new()
    .bold()
    .fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const DIM: Style = Style::new().dimmed();
const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const INTERVAL: Duration = Duration::from_millis(80);

#[derive(Clone, Copy)]
pub enum Mode {
    Hidden,
    Plain { color: bool },
    Live { color: bool },
}

impl Mode {
    pub fn for_cli(global: &GlobalArguments) -> Self {
        if global.quiet || matches!(global.diagnostics, DiagnosticsFormat::Json) {
            return Self::Hidden;
        }
        let color = report::color(global);
        if io::stderr().is_terminal()
            && env::var_os("TERM").as_deref() != Some(OsStr::new("dumb"))
            && env::var_os("CI").is_none()
        {
            Self::Live { color }
        } else {
            Self::Plain { color }
        }
    }
}

pub struct Progress {
    output: Output,
}

enum Output {
    Hidden,
    Plain {
        started: Instant,
        renderer: RefCell<Renderer>,
    },
    Live {
        stages: Sender<Message>,
        worker: Option<JoinHandle<Renderer>>,
    },
}

enum Message {
    Stage(&'static str),
    Stop,
}

impl Progress {
    pub fn new(mode: Mode) -> Self {
        let color = match mode {
            Mode::Hidden => {
                return Self {
                    output: Output::Hidden,
                };
            }
            Mode::Plain { color } | Mode::Live { color } => color,
        };
        let started = Instant::now();
        if matches!(mode, Mode::Live { .. }) {
            let (stages, receiver) = mpsc::channel();
            if let Ok(worker) =
                thread::Builder::new().spawn(move || animate(&receiver, started, color))
            {
                return Self {
                    output: Output::Live {
                        stages,
                        worker: Some(worker),
                    },
                };
            }
        }
        Self {
            output: Output::Plain {
                started,
                renderer: RefCell::new(Renderer::new(color)),
            },
        }
    }

    pub fn stage(&self, label: &'static str) {
        timings::stage(label);
        match &self.output {
            Output::Hidden => {}
            Output::Plain { started, renderer } => {
                let mut renderer = renderer.borrow_mut();
                if !renderer.failed {
                    renderer.render(label, None, started.elapsed(), None);
                    renderer.write();
                }
            }
            Output::Live { stages, .. } => {
                let _ = stages.send(Message::Stage(label));
            }
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        if let Output::Live { stages, worker } = &mut self.output {
            let _ = stages.send(Message::Stop);
            if let Some(worker) = worker.take()
                && let Ok(mut renderer) = worker.join()
            {
                renderer.clear();
            }
        }
    }
}

fn animate(receiver: &Receiver<Message>, started: Instant, color: bool) -> Renderer {
    let mut renderer = Renderer::new(color);
    let mut label = None;
    let mut frame = 0;
    loop {
        match receiver.recv_timeout(INTERVAL) {
            Ok(Message::Stage(next)) => label = Some(next),
            Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        let Some(label) = label else { continue };
        renderer.render(
            label,
            Some(FRAMES[frame]),
            started.elapsed(),
            Some(columns()),
        );
        if !renderer.write() {
            break;
        }
        frame = (frame + 1) % FRAMES.len();
    }
    renderer
}

struct Renderer {
    color: bool,
    text: String,
    output: String,
    previous: usize,
    failed: bool,
}

impl Renderer {
    fn new(color: bool) -> Self {
        Self {
            color,
            text: String::with_capacity(128),
            output: String::with_capacity(128),
            previous: 0,
            failed: false,
        }
    }

    fn render(
        &mut self,
        label: &str,
        spinner: Option<char>,
        elapsed: Duration,
        columns: Option<usize>,
    ) {
        self.text.clear();
        self.text.push_str("  ");
        if let Some(spinner) = spinner {
            self.text.push(spinner);
            self.text.push(' ');
        }
        self.text.push_str("flick");
        let brand_end = self.text.len();
        let _ = write!(self.text, "  {label}  ");
        let time_start = self.text.len();
        let _ = write!(self.text, "{:.1}s", elapsed.as_secs_f64());

        let limit = columns.map_or(usize::MAX, |width| width.saturating_sub(1));
        let mut end = 0;
        let mut visible = 0;
        for (cell, (byte, ch)) in self.text.char_indices().take(limit).enumerate() {
            end = byte + ch.len_utf8();
            visible = cell + 1;
        }
        self.text.truncate(end);
        let brand_start = 2.min(end);
        let brand_end = brand_end.min(end);
        let time_start = time_start.min(end);

        self.output.clear();
        if spinner.is_some() {
            self.output.push('\r');
        }
        self.output.push_str(&self.text[..brand_start]);
        styled(
            &mut self.output,
            BRAND,
            &self.text[brand_start..brand_end],
            self.color,
        );
        self.output.push_str(&self.text[brand_end..time_start]);
        styled(&mut self.output, DIM, &self.text[time_start..], self.color);
        if spinner.is_some() {
            let padding = self.previous.min(limit).saturating_sub(visible);
            self.output.extend(repeat_n(' ', padding));
            self.previous = visible;
        } else {
            self.output.push('\n');
        }
    }

    fn write(&mut self) -> bool {
        let mut stderr = io::stderr().lock();
        self.failed = stderr
            .write_all(self.output.as_bytes())
            .and_then(|()| stderr.flush())
            .is_err();
        !self.failed
    }

    fn clear(&mut self) {
        if self.failed || self.previous == 0 {
            return;
        }
        self.output.clear();
        self.output.push('\r');
        let visible = self.previous.min(columns().saturating_sub(1));
        self.output.extend(repeat_n(' ', visible));
        self.output.push('\r');
        self.write();
    }
}

fn styled(output: &mut String, style: Style, text: &str, color: bool) {
    if color && !text.is_empty() {
        let _ = write!(output, "{style}{text}{style:#}");
    } else {
        output.push_str(text);
    }
}

fn columns() -> usize {
    #[cfg(unix)]
    if let Ok(size) = tcgetwinsize(io::stderr())
        && size.ws_col > 0
    {
        return usize::from(size.ws_col);
    }
    // shortcut: width uses COLUMNS/80; use a native query when adding dynamic resize support on non-Unix.
    env::var("COLUMNS")
        .ok()
        .and_then(|width| width.parse().ok())
        .filter(|width| *width > 0)
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use super::{FRAMES, Renderer};
    use crate::report;
    use std::time::Duration;

    #[test]
    fn frames_fit_and_erase_previous_text() {
        for width in [1_usize, 20, 40, 80] {
            for color in [false, true] {
                let mut renderer = Renderer::new(color);
                for label in ["Compiling Flick modules with a longer label", "A"] {
                    let previous = renderer.previous;
                    renderer.render(
                        label,
                        Some(FRAMES[0]),
                        Duration::from_millis(1200),
                        Some(width),
                    );
                    let frame = report::strip_ansi(&renderer.output);
                    let text = frame.strip_prefix('\r').expect("redraw starts with CR");
                    assert!(!text.contains('\n'));
                    assert!(!text.contains('\r'));
                    assert!(text.chars().count() < width);
                    assert_eq!(
                        text.chars().count(),
                        previous.max(renderer.previous).min(width - 1)
                    );
                    assert!(text.starts_with(&renderer.text));
                    assert!(text[renderer.text.len()..].chars().all(|ch| ch == ' '));
                    if !color {
                        assert!(!renderer.output.contains('\x1b'));
                    }
                    if label == "A" && width > 1 {
                        assert!(
                            previous > renderer.previous,
                            "shorter phase erases its tail"
                        );
                    }
                }
            }
        }
    }
}
