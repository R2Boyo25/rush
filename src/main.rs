use common_macros::hash_map;
use gethostname::gethostname;
use std::collections::HashMap;
use std::env;

use rustyline::config::{BellStyle, Builder};
use rustyline::error::ReadlineError;
use rustyline::history::History;
use rustyline::DefaultEditor;
use shlex;

use users::{get_current_uid, get_user_by_uid};

use unescape::unescape_mapped;

mod flags;
use crate::flags::{Flags, FLUT};

#[derive(Debug)]
enum State {
    Exec(String),    // Command currently running
    MainPrompt(i32), // PS1
    BefrPrompt,      // PS0
    ContPrompt,      // PS2
}

fn get_prompt<'a>(
    varname: &'a str,
    default: &'a str,
    mapping: HashMap<char, impl AsRef<str>>,
) -> String {
    match unescape_mapped(env::var(varname).unwrap_or(default.to_string()), mapping) {
        Ok(prompt) => prompt,
        Err(err) => {
            println!("rush: ${}: {}", varname, err);
            default.to_string()
        }
    }
}

fn prompt_map(command_count: u64, history_count: u64) -> HashMap<char, impl AsRef<str>> {
    hash_map!(
        '$' => match get_current_uid() {
            0 => "#",
            _ => "$"
        }.to_string(),
        'u' => match get_user_by_uid(get_current_uid()) {
            Some(username) => username.name().to_string_lossy().to_string(),
            None => get_current_uid().to_string()
        },
        '#' => command_count.to_string(),
        '!' => history_count.to_string(),
        'v' => format!("{}.{}", env!("CARGO_PKG_VERSION_MAJOR"), env!("CARGO_PKG_VERSION_MINOR")),
        'V' => env!("CARGO_PKG_VERSION").to_string(),
        'h' => gethostname().to_string_lossy().to_string()
    )
}

fn main() {
    let mut rl = DefaultEditor::with_config(
        Builder::new()
            .max_history_size(5000)
            .unwrap()
            .history_ignore_space(true)
            .completion_prompt_limit(75)
            .auto_add_history(true)
            .bell_style(BellStyle::Audible)
            .build(),
    )
    .unwrap();

    let mut state: State = State::MainPrompt(0);
    let mut flags: Flags = Flags::from_bits(0).unwrap();
    let mut command_count: u64 = 0;

    loop {
        let history_count: u64 = rl.history().len().try_into().unwrap();
        match state {
            State::MainPrompt(_status) => {
                let readline = rl.readline(&get_prompt(
                    "PS1",
                    "rush$ ",
                    prompt_map(command_count, history_count),
                ));
                match readline {
                    Ok(line) => {
                        command_count += 1;
                        state = State::Exec(line.as_str().to_string());
                    }
                    Err(ReadlineError::Interrupted) => {
                        println!("^C");
                    }
                    Err(ReadlineError::Eof) => {
                        println!("^D");
                        break;
                    }
                    Err(err) => {
                        println!("Rustyline error: {:?}", err);
                    }
                }
            }
            State::Exec(ref command) => {
                let mut status: i32 = 0;

                match shlex::split(command) {
                    Some(argv) => {
                        if argv.len() < 1 {
                            state = State::MainPrompt(status);
                            continue;
                        }

                        if flags.contains(Flags::PEXEC) {
                            println!(
                                "{}{}",
                                get_prompt("PS4", "+ ", prompt_map(command_count, history_count)),
                                shlex::join(argv.iter().map(|x| x.as_str()))
                            );
                        }

                        let com = &argv[0];

                        match com.as_str() {
                            "exit" => break,
                            "format" => {
                                println!(
                                    "{}",
                                    match unescape_mapped(
                                        argv[1..].join(" "),
                                        prompt_map(command_count, history_count)
                                    ) {
                                        Ok(prompt) => {
                                            prompt
                                        }
                                        Err(error) => {
                                            status = 1;
                                            println!("rush: prompt: {}", error);
                                            argv[1..].join(" ")
                                        }
                                    }
                                )
                            }
                            "set" => {
                                'args: for arg in &argv[1..] {
                                    let chars = &arg.chars().collect::<Vec<_>>()[1..];
                                    match arg.chars().nth(0).unwrap() {
                                        '-' => {
                                            for flag in chars {
                                                if !(FLUT.contains_key(flag)) {
                                                    println!("rush: set: invalid flag: {}", flag);
                                                    status = 1;
                                                    break 'args;
                                                }

                                                flags = flags | FLUT[flag];
                                            }
                                        }
                                        '+' => {
                                            for flag in chars {
                                                if !(FLUT.contains_key(flag)) {
                                                    println!("rush: set: invalid flag: {}", flag);
                                                    status = 1;
                                                    break 'args;
                                                }

                                                flags = flags - FLUT[flag];
                                            }
                                        }
                                        _ => {
                                            println!("rush: set: invalid argument; not a flag");
                                            status = 1;
                                            break 'args;
                                        }
                                    };
                                }
                            }
                            _ => status = 0,
                        };
                    }
                    None => {
                        if !(command.matches('"').count() % 2 == 0)
                            || !(command.matches('\'').count() % 2 == 0)
                        {
                            println!("rush: unclosed quote: {}", command);
                        } else {
                            println!("rush: invalid syntax; cannot split: {}", command);
                        }
                        status = 1;
                    }
                };

                if flags.contains(Flags::EXITONFAIL) && status != 0 {
                    break;
                }

                state = State::MainPrompt(status);
            }
            unknown => todo!("Unimplemented state: {:?}", unknown),
        };
    }
}
