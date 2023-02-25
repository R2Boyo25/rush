use bitflags::bitflags;
use common_macros::hash_map;
use lazy_static::lazy_static;
use std::collections::HashMap;

bitflags! {
    pub struct Flags: u16 {
        const PEXEC      = 0b1;  // Print exec - print every command
        const LPEXEC     = 0b10; // Limited print exec - don't print conditionals
        const EXITONFAIL = 0b100;
        const ERRUNSET   = 0b1000;
    }
}

lazy_static! {
    pub static ref FLUT: HashMap<char, Flags> = hash_map! {
        'x' => Flags::PEXEC,
        'X' => Flags::LPEXEC,
        'e' => Flags::EXITONFAIL,
        'u' => Flags::ERRUNSET
    };
}
