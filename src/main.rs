use common_macros::hash_map;
use gethostname::gethostname;
use nix::unistd::ttyname;
use std::collections::HashMap;
use std::env;

use rustyline::config::{BellStyle, Builder};
use rustyline::error::ReadlineError;
use rustyline::history::History;
use rustyline::DefaultEditor;
use shlex;

use chrono::Local;
use time::macros::format_description;
use time::OffsetDateTime;

use users::{get_current_uid, get_user_by_uid};

use unescape::unescape_mapped;

mod flags;
use crate::flags::{Flags, FLUT};

#[derive(Debug)]
enum State {
    Exec(String), // Command currently running
    MainPrompt,   // PS1
    BefrPrompt,   // PS0
    ContPrompt,   // PS2
}

fn strftime_replace(s: String) -> Result<String, String> {
    let mut outbuf: String = Default::default();
    let mut tmpbuf: String = Default::default();
    let mut inesc: bool = false;
    let mut insub: bool = false;
    let mut infmt: bool = false;

    for chr in s.chars() {
        match chr {
            '%' if !infmt => inesc = true,
            'D' if inesc => {
                inesc = false;
                insub = true;
            }
            _ if inesc => {
                outbuf += "%";
                outbuf += &chr.to_string();
                inesc = false;
            }
            '{' if insub => {
                infmt = true;
                insub = false;
            }
            _ if insub => insub = false,
            '}' if infmt => {
                outbuf += &Local::now().format(&tmpbuf).to_string();
                infmt = false;
                tmpbuf = Default::default();
            }
            _ if infmt => tmpbuf += &chr.to_string(),
            _ => outbuf += &chr.to_string(),
        }
    }

    if insub {
        return Err("Unopened date expression; use like %D{format}".to_string());
    }

    if infmt {
        return Err("Unclosed date expression".to_string());
    }

    Ok(outbuf)
}

fn get_prompt<'a>(
    varname: &'a str,
    default: &'a str,
    mapping: HashMap<char, impl AsRef<str>>,
) -> String {
    match strftime_replace(env::var(varname).unwrap_or(default.to_string())) {
        Ok(prompt) => match unescape_mapped(prompt, mapping) {
            Ok(prompt) => prompt,
            Err(err) => {
                println!("rush: ${}: {}", varname, err);
                default.to_string()
            }
        },
        Err(err) => {
            println!("rush: ${}: %D: {}", varname, err);
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
        'h' => gethostname().to_string_lossy().to_string().split(".").last().unwrap().to_string(),
        'H' => gethostname().to_string_lossy().to_string(),
        'l' => match ttyname(0) {
            Ok(name) => match name.file_name() {
                Some(fname) => fname.to_str().unwrap().to_string(),
                None => "err".to_string()
            }
            Err(_) => "err".to_string()
        },
        's' => env::args().last().unwrap().split("/").last().unwrap().to_string(),
        'w' => env::current_dir().expect("No idea where we are. (env::current_dir())").to_string_lossy().to_string().replace(env::var("HOME").unwrap_or("/home/".to_string() + &get_user_by_uid(get_current_uid()).unwrap().name().to_string_lossy()).as_str(), "~/").replace("//", "/"),
        'W' => env::current_dir().expect("No idea where we are. (env::current_dir())").to_string_lossy().to_string().replace(env::var("HOME").unwrap_or("/home/".to_string() + &get_user_by_uid(get_current_uid()).unwrap().name().to_string_lossy()).as_str(), "~/").replace("//", "/").split("/").last().unwrap().to_string(),
        'd' => match OffsetDateTime::now_local() {
            Ok(curtime) => match curtime.format(format_description!("[weekday], [month repr:short] [day]")) {
                Ok(formatted_date) => formatted_date,
                Err(_) => "???, ??? ??".to_string()
            }
            Err(_) => "???, ??? ??".to_string()
        },
        't' => match OffsetDateTime::now_local() {
            Ok(curtime) => match curtime.format(format_description!("[hour]:[minute]:[second]")) {
                Ok(formatted_time) => formatted_time,
                Err(_) => "??:??:??".to_string()
            }
            Err(_) => "??:??:??".to_string()
        },
        'T' => match OffsetDateTime::now_local() {
            Ok(curtime) => match curtime.format(format_description!("[hour repr:12]:[minute]:[second] [period]")) {
                Ok(formatted_time) => formatted_time,
                Err(_) => "??:??:??".to_string()
            }
            Err(_) => "??:??:??".to_string()
        },
        '@' => match OffsetDateTime::now_local() {
            Ok(curtime) => match curtime.format(format_description!("[hour repr:12]:[minute] [period]")) {
                Ok(formatted_time) => formatted_time,
                Err(_) => "??:?? ?M".to_string()
            }
            Err(_) => "??:?? ?M".to_string()
        },
        'A' => match OffsetDateTime::now_local() {
            Ok(curtime) => match curtime.format(format_description!("[hour]:[minute]")) {
                Ok(formatted_time) => formatted_time,
                Err(_) => "??:??".to_string()
            }
            Err(_) => "??:??".to_string()
        },
        // %e - execution time
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

    let mut state: State = State::MainPrompt;
    let mut flags: Flags = Flags::from_bits(0).unwrap();
    let mut command_count: u64 = 0;
    let mut status: i32 = 0;

    loop {
        let history_count: u64 = rl.history().len().try_into().unwrap();
        match state {
            State::MainPrompt => {
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
                match shlex::split(command) {
                    Some(argv) => {
                        if argv.len() < 1 {
                            state = State::MainPrompt;
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
                                    match strftime_replace(argv[1..].join(" ")) {
                                        Ok(prompt) => match unescape_mapped(
                                            prompt,
                                            prompt_map(command_count, history_count)
                                        ) {
                                            Ok(prompt) => {
                                                prompt
                                            }
                                            Err(error) => {
                                                status = 1;
                                                format!("rush: format: {}", error)
                                            }
                                        },
                                        Err(error) => {
                                            status = 1;
                                            format!("rush: format: %D: {}", error)
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

                state = State::MainPrompt;
            }
            unknown => todo!("Unimplemented state: {:?}", unknown),
        };
    }
}
