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

#[derive(Debug, Clone)]
enum State {
    Exec(String),   // Command currently running
    MainPrompt,     // PS1
    AfterPrompt,    // PS0
    ContinuePrompt, // PS2
}

struct Context {
    state: State,
    rl: DefaultEditor,
    flags: Flags,
    status: i32,
    command_count: u64,
    shell_running: bool,
}

impl Context {
    fn strftime_replace(&self, s: String) -> Result<String, String> {
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
        &self,
        varname: &'a str,
        default: &'a str,
        mapping: HashMap<char, impl AsRef<str>>,
    ) -> String {
        match self.strftime_replace(env::var(varname).unwrap_or(default.to_string())) {
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

    fn prompt_map(&self) -> HashMap<char, impl AsRef<str>> {
        let history_count: u64 = self.rl.history().len().try_into().unwrap();

        hash_map!(
            '$' => match get_current_uid() {
                0 => "#",
                _ => "$"
            }.to_string(),
            'u' => match get_user_by_uid(get_current_uid()) {
                Some(username) => username.name().to_string_lossy().to_string(),
                None => get_current_uid().to_string()
            },
            '#' => self.command_count.to_string(),
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
                Ok(curtime) => curtime.format(format_description!("[weekday], [month repr:short] [day]")).unwrap_or("???, ??? ??".to_string()),
                Err(_) => "???, ??? ??".to_string()
            },
            't' => match OffsetDateTime::now_local() {
                Ok(curtime) => curtime.format(format_description!("[hour]:[minute]:[second]")).unwrap_or("??:??:??".to_string()),
                Err(_) => "??:??:??".to_string()
            },
            'T' => match OffsetDateTime::now_local() {
                Ok(curtime) => curtime.format(format_description!("[hour repr:12]:[minute]:[second] [period]")).unwrap_or("??:??:?? ?M".to_string()),
                Err(_) => "??:??:?? ?M".to_string()
            },
            '@' => match OffsetDateTime::now_local() {
                Ok(curtime) => curtime.format(format_description!("[hour repr:12]:[minute] [period]")).unwrap_or("??:?? ?M".to_string()),
                Err(_) => "??:?? ?M".to_string()
            },
            'A' => match OffsetDateTime::now_local() {
                Ok(curtime) => curtime.format(format_description!("[hour]:[minute]")).unwrap_or("??:??".to_string()),
                Err(_) => "??:??".to_string()
            },
            // %e - execution time
        )
    }

    fn handle_command(&mut self, command: &String) {
        match shlex::split(command) {
            Some(argv) => {
                if argv.len() < 1 {
                    self.state = State::MainPrompt;
                    return;
                }

                if self.flags.contains(Flags::PEXEC) {
                    println!(
                        "{}{}",
                        self.get_prompt("PS4", "+ ", self.prompt_map()),
                        shlex::join(argv.iter().map(|x| x.as_str()))
                    );
                }

                let com = &argv[0];

                match com.as_str() {
                    "exit" => self.shell_running = false,
                    "format" => {
                        println!(
                            "{}",
                            match self.strftime_replace(argv[1..].join(" ")) {
                                Ok(prompt) => match unescape_mapped(prompt, self.prompt_map()) {
                                    Ok(prompt) => {
                                        prompt
                                    }
                                    Err(error) => {
                                        self.status = 1;
                                        format!("rush: format: {}", error)
                                    }
                                },
                                Err(error) => {
                                    self.status = 1;
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
                                            self.status = 1;
                                            break 'args;
                                        }

                                        self.flags = self.flags | FLUT[flag];
                                    }
                                }
                                '+' => {
                                    for flag in chars {
                                        if !(FLUT.contains_key(flag)) {
                                            println!("rush: set: invalid flag: {}", flag);
                                            self.status = 1;
                                            break 'args;
                                        }

                                        self.flags = self.flags - FLUT[flag];
                                    }
                                }
                                _ => {
                                    println!("rush: set: invalid argument; not a flag");
                                    self.status = 1;
                                    break 'args;
                                }
                            };
                        }
                    }
                    _ => self.status = 0,
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
                self.status = 1;
            }
        };

        if self.flags.contains(Flags::EXITONFAIL) && self.status != 0 {
            self.shell_running = false;
        }

        self.state = State::MainPrompt;
    }

    fn handle_main_prompt(&mut self) {
        let readline = self
            .rl
            .readline(&self.get_prompt("PS1", "rush$ ", self.prompt_map()));
        match readline {
            Ok(line) => {
                self.command_count += 1;
                self.state = State::Exec(line.as_str().to_string());
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
            }
            Err(ReadlineError::Eof) => {
                println!("^D");
                self.shell_running = false;
            }
            Err(err) => {
                println!("Rustyline error: {:?}", err);
            }
        }
    }

    fn main(&mut self) {
        while self.shell_running {
            match self.state.clone() {
                State::MainPrompt => {
                    self.handle_main_prompt();
                }
                State::Exec(ref command) => {
                    self.handle_command(command);
                }
                unknown => todo!("Unimplemented state: {:?}", unknown),
            };
        }
    }

    fn new() -> Self {
        Self {
            rl: DefaultEditor::with_config(
                Builder::new()
                    .max_history_size(5000)
                    .unwrap()
                    .history_ignore_space(true)
                    .completion_prompt_limit(75)
                    .auto_add_history(true)
                    .bell_style(BellStyle::Audible)
                    .build(),
            )
            .unwrap(),
            state: State::MainPrompt,
            flags: Flags::from_bits(0).unwrap(),
            command_count: 0,
            status: 0,
            shell_running: true,
        }
    }
}

fn main() {
    Context::new().main()
}
