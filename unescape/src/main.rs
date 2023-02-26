use common_macros::hash_map;
use unescape::unescape_mapped;

fn main() {
    println!(
        "{}",
        unescape_mapped("a\\t\\\x70t%aa".to_string(), hash_map!('a' => "Hewwo"))
            .expect("Unable to unescape")
    );
}
