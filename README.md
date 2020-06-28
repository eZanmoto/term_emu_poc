Terminal Emulator Proof-of-Concept
==================================

About
-----

This is a proof-of-concept terminal emulator. It is used to illustrate how other
"proxy" terminal emulators (i.e. those which run further terminal emulator
programs, such as `tmux` and `ssh`) operate.

This project simply renders the "sub"-terminal with a single space of padding.

Usage
-----

This project can be built with Rust using `cargo build`, or it can be built with
Docker using `bash build.sh`. Both will build the binary to `target/debug/tep`,
which can be run directly.

Operation
---------

This program is essentially the internal components of
[Alacritty](https://github.com/alacritty/alacritty) (primarily, its [terminal
parser
implementation](https://github.com/alacritty/alacritty/blob/7433f45ff9c6efeb48e223e90dd4aa9ee135b5e8/src/term/mod.rs)
and its [PTY
spawner](https://github.com/alacritty/alacritty/blob/7433f45ff9c6efeb48e223e90dd4aa9ee135b5e8/src/tty.rs))
re-targeted to run on the command-line instead of in a GUI. It uses
[pancurses](https://github.com/ihalila/pancurses) for rendering to the
command-line.
