// Copyright 2020 Sean Kelleher. All rights reserved.
// Use of this source code is governed by a MIT
// licence that can be found in the LICENCE file.

use std::convert::TryInto;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;

extern crate alacritty;
extern crate pancurses;

use alacritty::ansi::{Color, NamedColor, Processor};
use alacritty::cli::Options;
use alacritty::config::Config;
use alacritty::index::{Point, Line, Column};
use alacritty::Term;
use alacritty::term::SizeInfo;
use alacritty::tty;

use pancurses::colorpair::ColorPair;
use pancurses::Input;
use pancurses::ToChtype;
use pancurses::Window;

const OS_IO_ERROR: i32 = 5;

fn main() {
    let win = pancurses::initscr();

    // Characters are not rendered when they're typed, instead they're sent to
    // the underlying terminal, which decides whether to echo them or not (by
    // writing new characters from `ptyf`, below). An example scenario of when
    // this comes in handy is in the case of typing backspace. Using `noecho`
    // prevents `^?` being briefly echoed before the cursor in between the time
    // that the backspace key was pressed and the time when the new rendering of
    // the terminal state is received and output.
    pancurses::noecho();

    pancurses::start_color();

    for i in 0..COLOUR_INDEXES.len()-1 {
        pancurses::init_pair(i as i16, COLOUR_INDEXES[i], pancurses::COLOR_BLACK);
    }

    // We put the window input into non-blocking mode so that `win.getch()`
    // returns `None` immediately if there is no input. This allows us to read
    // from the PTY and the the window in the same thread. Note that this
    // results in a busy loop, which should ideally be replaced by blocking
    // reads on separate threads for efficiency.
    win.nodelay(true);

    let (y, x) = win.get_max_yx();
    let size = new_size_info(x - 2, y - 2);

    let conf = Config::default();

    // `pty` provides methods for manipulating the PTY.
    let pty = tty::new(&conf, &Options::default(), &&size, None);

    // `ptyf` is a `File` interface to the server end of the PTY client/server
    // pair.
    let mut ptyf = pty.reader();

    // `parser` reads and parses the data read from `pty`, and updates the state
    // of the terminal "display" that is maintained in `term`.
    let mut parser = Processor::new();
    let mut term = Term::new(&conf, size);

    let border_chars = ['*', '+', '-'];
    let mut cur_border_char = 0;

    let mut exit_reason: Option<String> = None;
    let mut buf = [0u8; 0x1000];
    // We would ideally avoid using labels for loop termination but we use one
    // here for simplicity.
    'evt_loop: loop {
        match ptyf.read(&mut buf[..]) {
            Ok(0) => {
                // End-of-file.
                break 'evt_loop;
            },
            Ok(n) => {
                for byte in &buf[..n] {
                    parser.advance(&mut term, *byte, &mut ptyf);
                }
                let result = render_term_to_win(&term, &win, border_chars[cur_border_char]);
                if let Err(err) = result {
                    let colour_type =
                        match err {
                            RenderError::ColourSpecFound => "specification",
                            RenderError::ColourIndexFound => "index",
                        };
                    exit_reason = Some(format!(
                        "encountered a colour {}, which isn't currently supported",
                        colour_type,
                    ));
                    break 'evt_loop;
                }
            },
            Err(e) => {
                let k = e.kind();
                if k == ErrorKind::Other && e.raw_os_error() == Some(OS_IO_ERROR) {
                    // We interpret an `OS_IO_ERROR` as the PTY process having
                    // terminated, as it corresponds with this during
                    // experimentation.
                    break 'evt_loop;
                }

                if k != ErrorKind::Interrupted && k != ErrorKind::WouldBlock {
                    exit_reason = Some(format!(
                        "couldn't read from PTY (error kind: {:?}, os error: {:?}): {}",
                        e.kind(),
                        e.raw_os_error(),
                        e,
                    ));
                    break 'evt_loop;
                };
            },
        }

        if let Some(input) = win.getch() {
            match input {
                Input::Character(c) => {
                    let utf8_len = c.len_utf8();
                    let mut bytes = Vec::with_capacity(utf8_len);
                    unsafe {
                        bytes.set_len(utf8_len);
                        c.encode_utf8(&mut bytes[..]);
                    }

                    if utf8_len == 1 && bytes[0] == 4 {
                        // We use `^D` as a trigger to change the border style.
                        cur_border_char = (cur_border_char + 1) % border_chars.len();
                        let result = render_term_to_win(&term, &win, border_chars[cur_border_char]);
                        if let Err(err) = result {
                            let colour_type =
                                match err {
                                    RenderError::ColourSpecFound => "specification",
                                    RenderError::ColourIndexFound => "index",
                                };
                            exit_reason = Some(format!(
                                "encountered a colour {}, which isn't currently supported",
                                colour_type,
                            ));
                            break 'evt_loop;
                        }
                    } else {
                        let mut i = 0;
                        while i < utf8_len {
                            match ptyf.write(&bytes[..]) {
                                Ok(0) => {
                                    exit_reason = Some(format!("PTY is unable to accept bytes"));
                                    break 'evt_loop;
                                },
                                Ok(n) => {
                                    i += n;
                                },
                                Err(e) => {
                                    let k = e.kind();
                                    if k != ErrorKind::Interrupted && k != ErrorKind::WouldBlock {
                                        exit_reason = Some(format!(
                                            "couldn't read from PTY (error kind: {:?}, os error: {:?}): {}",
                                            e.kind(),
                                            e.raw_os_error(),
                                            e,
                                        ));
                                        break 'evt_loop;
                                    };
                                },
                            }
                        }
                    }
                },
                Input::KeyResize => {
                    let (y, x) = win.get_max_yx();
                    let size = new_size_info(x - 2, y - 2);
                    term.resize(&size);
                    pty.resize(&&size);
                },
                _ => {
                    exit_reason = Some(format!("unhandled input: {:?}", input));
                    break 'evt_loop;
                },
            }
        }
    }

    pancurses::endwin();

    if let Some(s) = exit_reason {
        println!("process exited: {}", s);
    }
}

const COLOUR_INDEXES: [i16; 8] = [
    pancurses::COLOR_WHITE,
    pancurses::COLOR_RED,
    pancurses::COLOR_GREEN,
    pancurses::COLOR_BLUE,
    pancurses::COLOR_CYAN,
    pancurses::COLOR_MAGENTA,
    pancurses::COLOR_YELLOW,
    pancurses::COLOR_BLACK,
];

fn get_colour_index(c: i16) -> usize {
    for i in 1..COLOUR_INDEXES.len()-1 {
        if c == COLOUR_INDEXES[i] {
            return i
        }
    }
    0
}

fn new_size_info(w: i32, h: i32) -> SizeInfo {
    SizeInfo {
        width: w as f32,
        height: h as f32,
        cell_width: 1.0,
        cell_height: 1.0,
        padding_x: 0.0,
        padding_y: 0.0,
    }
}

fn render_term_to_win(term: &Term, win: &Window, border_char: char) -> RenderResult {
    win.clear();

    let (y, x) = win.get_max_yx();
    for i in 0..y {
        win.mvaddch(i, 0, border_char);
        win.mvaddch(i, x-1, border_char);
    }
    for i in 0..x {
        win.mvaddch(0, i, border_char);
        win.mvaddch(y-1, i, border_char);
    }

    let grid = term.grid();
    let mut line = Line(0);
    while line < grid.num_lines() {
        let mut col = Column(0);
        while col < grid.num_cols() {
            let cell = grid[line][col];
            match cell.fg {
                Color::Named(name) => {
                    let c = match name {
                        NamedColor::Background => pancurses::COLOR_BLACK,
                        NamedColor::Black => pancurses::COLOR_BLACK,
                        NamedColor::Blue => pancurses::COLOR_BLUE,
                        NamedColor::BrightBlack => pancurses::COLOR_BLACK,
                        NamedColor::BrightBlue => pancurses::COLOR_BLUE,
                        NamedColor::BrightCyan => pancurses::COLOR_CYAN,
                        NamedColor::BrightGreen => pancurses::COLOR_GREEN,
                        NamedColor::BrightMagenta => pancurses::COLOR_MAGENTA,
                        NamedColor::BrightRed => pancurses::COLOR_RED,
                        NamedColor::BrightWhite => pancurses::COLOR_WHITE,
                        NamedColor::BrightYellow => pancurses::COLOR_YELLOW,
                        NamedColor::Cursor => pancurses::COLOR_BLACK,
                        NamedColor::CursorText => pancurses::COLOR_WHITE,
                        NamedColor::Cyan => pancurses::COLOR_CYAN,
                        NamedColor::DimBlack => pancurses::COLOR_BLACK,
                        NamedColor::DimBlue => pancurses::COLOR_BLUE,
                        NamedColor::DimCyan => pancurses::COLOR_CYAN,
                        NamedColor::DimGreen => pancurses::COLOR_GREEN,
                        NamedColor::DimMagenta => pancurses::COLOR_MAGENTA,
                        NamedColor::DimRed => pancurses::COLOR_RED,
                        NamedColor::DimWhite => pancurses::COLOR_WHITE,
                        NamedColor::DimYellow => pancurses::COLOR_YELLOW,
                        NamedColor::Foreground => pancurses::COLOR_WHITE,
                        NamedColor::Green => pancurses::COLOR_GREEN,
                        NamedColor::Magenta => pancurses::COLOR_MAGENTA,
                        NamedColor::Red => pancurses::COLOR_RED,
                        NamedColor::White => pancurses::COLOR_WHITE,
                        NamedColor::Yellow => pancurses::COLOR_YELLOW,
                    };
                    win.attrset(ColorPair(get_colour_index(c) as u8));
                    win.mvaddch(
                        (line.0 as i32) + 1,
                        (col.0 as i32) + 1,
                        cell.c.to_chtype(),
                    );
                },
                Color::Spec(_) => {
                    return Err(RenderError::ColourSpecFound);
                },
                Color::Indexed(_) => {
                    return Err(RenderError::ColourIndexFound);
                },
            };
            col += 1;
        }
        line += 1;
    }

    let Point{line: Line(row), col: Column(col)} = term.cursor().point;
    win.mv(
        ((row + 1) as usize).try_into().unwrap(),
        ((col + 1) as usize).try_into().unwrap(),
    );

    win.refresh();

    Ok(())
}

type RenderResult = Result<(), RenderError>;

enum RenderError {
    // These colour types aren't currently supported.
    ColourSpecFound,
    ColourIndexFound,
}
