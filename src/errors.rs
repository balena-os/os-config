use std::ffi::OsString;
use std::path::PathBuf;

error_chain! {
    foreign_links {
        Io(::std::io::Error);
        Reqwest(::reqwest::Error);
        SerdeJSON(::serde_json::Error);
        DBus(::dbus::Error);
        DBusTypeMismatch(::dbus::arg::TypeMismatchError);
        OpenSSL(::openssl::error::ErrorStack);
    }

    errors {
        MergeConfigJSON(path: PathBuf) {
            description("Merging `config.json` failed")
            display("Merging {:?} failed", path)
        }

        ReadConfigJSON(path: PathBuf) {
            description("Reading `config.json` failed")
            display("Reading {:?} failed", path)
        }

        WriteConfigJSON(path: PathBuf) {
            description("Writing `config.json` failed")
            display("Writing {:?} failed", path)
        }

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

        NotAnObjectJSON {
            description("Expected JSON object")
        }

        ApiEndpointNotStringJSON {
            description("`apiEndpoint` should be a string")
        }

        ApiEndpointNotFoundJSON {
            description("`apiEndpoint` not found")
        }

        MasterKeyNotStringJSON {
            description("`deviceMasterKey` should be a string")
        }

        MasterKeyNotFoundJSON {
            description("`deviceMasterKey` not found")
        }

        StartService(name: String) {
            description("Starting service failed")
            display("Starting {} failed", name)
        }

        StopService(name: String) {
            description("Stopping service failed")
            display("Stopping {} failed", name)
        }

        ReloadRestartService(name: String) {
            description("Reloading or restarting service failed")
            display("Reloading or restarting {} failed", name)
        }

        AwaitServiceState(name: String, state: String) {
            description("Awaiting service to enter state failed")
            display("Awaiting {} to enter {} state failed", name, state)
        }

        AwaitServiceStateTimeout {
            description("Timed out awaiting service state")
        }

        WriteFile(path: PathBuf) {
            description("Writing file failed")
            display("Writing {:?} failed", path)
        }

        RemoveFile(path: PathBuf) {
            description("Removing file failed")
            display("Removing {:?} failed", path)
        }

        NotAFile(path: PathBuf) {
            description("Expected file")
            display("Expected file: {:?}", path)
        }

        NotAUnicodeFileName(file_name: OsString) {
            description("Expected Unicode file name")
            display("Expected Unicode file name: {:?}", file_name)
        }

        ParsePermissionMode(mode: String) {
            description("Parsing permission mode failed")
            display("Parsing permission mode `{}` failed", mode)
        }

        GenerateDeviceApiKey {
            description("Generating `deviceApiKey` failed")
        }
    }
}

pub fn exit_code(e: &Error) -> i32 {
    match *e.kind() {
        ErrorKind::ReadOSConfig => 3,
        ErrorKind::GetOSConfigApi => 4,
        ErrorKind::StartService(_) => 5,
        ErrorKind::StopService(_) => 6,
        ErrorKind::ReloadRestartService(_) => 7,
        ErrorKind::WriteFile(_) => 8,
        ErrorKind::ServiceNotFoundJSON(_) => 9,
        ErrorKind::ConfigNotFoundJSON(_, _) => 10,
        ErrorKind::MergeConfigJSON(_) => 11,
        _ => 1,
    }
}
