use std::collections::HashMap;

#[derive(Debug)]
enum State {
    Normal,
    Escaped,
    Format,
    Hex,
    Oct,
}

pub fn unescape_mapped(
    instr: String,
    lut: HashMap<char, impl AsRef<str>>,
) -> Result<String, String> {
    let mut wrkstr: Vec<char> = instr.clone().chars().collect();
    let mut outbuf: String = Default::default();
    let mut buf: String = Default::default();
    let mut state: State = State::Normal;

    while wrkstr.len() > 0 {
        let mut tmp = [0u8; 4];
        let chr = wrkstr.remove(0);

        match state {
            State::Escaped => {
                match chr {
                    'x' => state = State::Hex,
                    'e' => buf += "\033",
                    'a' => buf += "\x07",
                    'b' => buf += "\x7f",
                    'f' => buf += "\x0c",
                    'n' => buf += "\n",
                    'r' => buf += "\r",
                    't' => buf += "\t",
                    'v' => buf += "\x0b",
                    '\\' => buf += "\\",
                    '0'..='9' => {
                        buf += &chr.to_string();
                        state = State::Oct;
                    }
                    '[' => {
                        outbuf += "\x01";
                        state = State::Normal;
                    }
                    ']' => {
                        outbuf += "\x02";
                        state = State::Normal;
                    }
                    _ => {
                        outbuf += chr.encode_utf8(&mut tmp);
                        state = State::Normal;
                    }
                };
            }
            State::Format => {
                if !lut.contains_key(&chr) {
                    return Err(format!("invalid formatting string: %{}", chr));
                }

                outbuf += lut[&chr].as_ref();

                state = State::Normal;
            }
            State::Normal => {
                match chr {
                    '%' => state = State::Format,
                    '\\' => state = State::Escaped,
                    _ => outbuf += chr.encode_utf8(&mut tmp),
                };
            }
            State::Hex => match chr {
                '0'..='9' | 'A'..='F' | 'a'..='f' => buf += &chr.to_string(),
                _ => {
                    match u32::from_str_radix(&buf, 16) {
                        Ok(i) => match char::from_u32(i) {
                            Some(c) => {
                                outbuf += &c.to_string();
                                buf = Default::default();
                                state = State::Normal;
                                wrkstr.insert(0, chr);
                            }
                            None => return Err(format!("invalid unicode codepoint: \\x{}", buf)),
                        },
                        Err(_) => return Err(format!("invalid 32-bit hex code: \\x{}", buf)),
                    };
                }
            },
            State::Oct => match chr {
                '0'..='7' if buf.len() < 3 => {
                    buf += &chr.to_string();
                }
                _ => {
                    match u32::from_str_radix(&buf, 8) {
                        Ok(i) => match char::from_u32(i) {
                            Some(c) => {
                                outbuf += &c.to_string();
                                buf = Default::default();
                                state = State::Normal;
                                wrkstr.insert(0, chr);
                            }
                            None => return Err(format!("invalid unicode codepoint: \\0{}", buf)),
                        },
                        Err(_) => return Err(format!("invalid 32-bit octal code: \\0{}", buf)),
                    };
                }
            },
        }
    }

    Ok(outbuf)
}

pub fn unescape(instr: String) -> Result<String, String> {
    unescape_mapped(instr, HashMap::<char, &str>::new())
}
