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

        ServiceNotFoundJSON(service_id: String) {
            description("Service not found in `os-config-api.json`")
            display("Service `{}` not found in `os-config-api.json`", service_id)
        }

        ConfigNotFoundJSON(service_id: String, name: String) {
            description("Config not found in `os-config-api.json`")
            display("Service `{}` config `{}` not found in `os-config-api.json`", service_id, name)
        }

        Systemctl(args: String) {
            description("`systemctl` failed")
            display("`systemctl {}` failed", args)
        }

        RestartService(name: String) {
            description("Restarting service failed")
            display("Restarting {} failed", name)
        }

        WriteFile(path: String) {
            description("Writing file failed")
            display("Writing `{}` failed", path)
        }

        ParsePermissionMode(mode: String) {
            description("Parsing permission mode failed")
            display("Parsing permission mode `{}` failed", mode)
        }
    }
}

pub fn exit_code(e: &Error) -> i32 {
    match *e.kind() {
        ErrorKind::ReadOSConfig => 3,
        ErrorKind::GetOSConfigApi => 4,
        ErrorKind::RestartService(_) => 5,
        ErrorKind::WriteFile(_) => 6,
        ErrorKind::ServiceNotFoundJSON(_) => 7,
        ErrorKind::ConfigNotFoundJSON(_, _) => 8,
        _ => 1,
    }
}
