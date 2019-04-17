#![deny(missing_docs)]

/*!
This crate provides a terminal interface that runs alongside your app
*/

use std::{
    io::{stdout, Write},
    iter,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self, JoinHandle},
};

use clap::{App, ArgMatches, Result as ClapResult};
use crossterm::{InputEvent, KeyEvent, Terminal, TerminalInput};

pub use clap;

/// Behavior for processing commands
pub trait CommandProcessor: Sized {
    /// The type output after the command has been parsed
    type Parsed;
    /// Parse an input
    fn parse(&mut self, input: &str) -> Self::Parsed;
}

impl<F, R> CommandProcessor for F
where
    F: Fn(&str) -> R,
{
    type Parsed = R;
    fn parse(&mut self, input: &str) -> Self::Parsed {
        self(input)
    }
}

impl<'a, 'b> CommandProcessor for App<'a, 'b> {
    type Parsed = ClapResult<ArgMatches<'a>>;
    fn parse(&mut self, input: &str) -> Self::Parsed {
        self.get_matches_from_safe_borrow(
            iter::once(env!("CARGO_PKG_NAME")).chain(input.split_whitespace()),
        )
    }
}

/// A handle to a terminal interface that processes commands
pub struct Console<M> {
    recv: Receiver<M>,
    closed: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl<M> Console<M>
where
    M: Send + 'static,
{
    /// Create a new `Console` with the given state and processor builder
    pub fn new<B, F, P>(builder: B, process: F) -> Self
    where
        B: FnOnce() -> P + Send + 'static,
        P: CommandProcessor,
        F: Fn(P::Parsed) -> Option<M> + Send + 'static,
    {
        let (send, recv) = mpsc::channel();
        let closed = Arc::new(AtomicBool::from(false));
        let closed_clone = Arc::clone(&closed);
        let handle = thread::spawn(move || {
            let closed = closed_clone;
            let (width, _) = Terminal::new().terminal_size();
            let terminal = TerminalInput::new();
            let reader = terminal.read_sync();
            let mut processor = builder();
            let mut input = String::new();
            let mut history: Vec<String> = Vec::new();
            let mut curr = None;
            let mut cursor = 0;
            macro_rules! flush {
                () => {
                    let _ = stdout().flush();
                };
            }
            macro_rules! clear_line {
                () => {
                    print!(
                        "\r{}",
                        (0..(width as usize - input.len()))
                            .map(|_| ' ')
                            .collect::<String>()
                    );
                };
            }
            macro_rules! set_line {
                ($line:expr) => {
                    clear_line!();
                    input = $line;
                    print!("\r{}", input);
                    print!("\r{}", &input[..cursor.min(input.len())]);
                    flush!();
                };
            }
            for event in reader {
                if closed.load(Ordering::Relaxed) {
                    return;
                }
                if let InputEvent::Keyboard(key_event) = event {
                    match key_event {
                        KeyEvent::Backspace => {
                            let reprint = if cursor >= input.len() {
                                input.pop().is_some()
                            } else if !input.is_empty() && cursor > 0 {
                                input.remove(cursor - 1);
                                true
                            } else {
                                false
                            };
                            if reprint {
                                cursor -= 1;
                                set_line!(input);
                            }
                        }
                        KeyEvent::Delete => {
                            let reprint = if cursor < input.len() {
                                input.remove(cursor);
                                true
                            } else {
                                false
                            };
                            if reprint {
                                set_line!(input);
                            }
                        }
                        KeyEvent::Up => {
                            if curr.is_none() {
                                curr = Some(history.len());
                            }
                            if let Some(ref mut curr) = curr {
                                if *curr > 0 {
                                    *curr -= 1;
                                }
                                let new_input = history[*curr].clone();
                                cursor = new_input.len();
                                set_line!(new_input);
                            }
                        }
                        KeyEvent::Down => {
                            if let Some(c) = curr {
                                let new_input = if c < history.len() - 1 {
                                    curr = Some(c + 1);
                                    history[c + 1].clone()
                                } else {
                                    curr = None;
                                    String::new()
                                };
                                cursor = new_input.len();
                                set_line!(new_input);
                            }
                        }
                        KeyEvent::Left => {
                            if cursor > 0 {
                                cursor -= 1;
                                set_line!(input);
                            }
                        }
                        KeyEvent::Right => {
                            if cursor < input.len() {
                                cursor += 1;
                                set_line!(input);
                            }
                        }
                        KeyEvent::Char(c) => {
                            if c == '\n' {
                                println!();
                                // Submit
                                let parsed = processor.parse(input.trim());
                                history.push(input.trim().to_string());
                                input.clear();
                                cursor = 0;
                                curr = None;
                                if let Some(message) = process(parsed) {
                                    let _ = send.send(message);
                                } else {
                                    closed.store(true, Ordering::Relaxed);
                                    return;
                                }
                            } else {
                                // Add character
                                input.insert(cursor, c);
                                cursor += 1;
                                set_line!(input);
                            }
                        }
                        _ => {}
                    }
                }
            }
        });
        Console {
            recv,
            closed,
            handle: Some(handle),
        }
    }
    /// Get a message from the `Console`
    pub fn poll(&self) -> Option<M> {
        self.recv.try_recv().ok()
    }
    /// Check if the console is open
    pub fn is_open(&self) -> bool {
        !self.closed.load(Ordering::Relaxed)
    }
}

impl<M> Drop for Console<M> {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
        let _ = self.handle.take().unwrap().join();
    }
}
