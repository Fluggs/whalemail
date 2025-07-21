pub fn string_as_bytes(s: &String) -> String {
    s.as_bytes()
        .iter()
        .map(|x| format!("{:02x?}", x))
        .collect::<Vec<_>>()
        .join(" ")
}