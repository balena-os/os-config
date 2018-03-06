error_chain! {
    foreign_links {
        Io(::std::io::Error);
        Reqwest(::reqwest::Error);
        SerdeJSON(::serde_json::Error);
    }

    errors {
        ReadOSConfig {
            description("Reading `os-config.json` failed")
        }

        GetOSConfigApi {
            description("Getting `os-config-api.json` failed")
        }

        MissingSchemaVersionJSON {
            description("Missing `schema_version`")
        }

        SchemaVersionNotStringJSON {
            description("`schema_version` should be a string")
        }

        UnexpectedShemaVersionJSON(expected: &'static str, got: String) {
            description("Expected schema version")
            display("Expected schema version {}, got {}", expected, got)
        }
    }
}

pub fn exit_code(e: &Error) -> i32 {
    match *e.kind() {
        ErrorKind::ReadOSConfig => 3,
        ErrorKind::GetOSConfigApi => 4,
        _ => 1,
    }
}
