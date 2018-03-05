error_chain! {
    foreign_links {
        Io(::std::io::Error);
        Reqwest(::reqwest::Error);
        SerdeJSON(::serde_json::Error);
    }

    errors {
        ReadLink {
            description("Read link failed")
        }
    }
}

pub fn exit_code(e: &Error) -> i32 {
    match *e.kind() {
        ErrorKind::ReadLink => 3,
        _ => 1,
    }
}
