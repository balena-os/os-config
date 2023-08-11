use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::thread;
use std::time::Duration;

use actix_web::dev::ServerHandle;
use actix_web::rt::System;
use actix_web::web::{resource, Data};
use actix_web::{App, HttpResponse, HttpServer};

use openssl::pkey::PKey;
use openssl::ssl::{SslAcceptor, SslMethod};
use openssl::x509::X509;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/*******************************************************************************
*  Mock JSON HTTP server
*/

const CONFIG_ROUTE: &str = "/os/v1/config";

pub fn serve_config(config: String, with_ssl: bool, port: u16) -> Serve {
    let mut server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(config.clone()))
            .wrap(actix_web::middleware::Logger::default())
            .service(resource(CONFIG_ROUTE).to(|c: Data<String>| async move {
                HttpResponse::Ok()
                    .content_type("application/json")
                    .message_body((**c).clone())
            }))
    });

    server = if with_ssl {
        let pkey = PKey::private_key_from_pem(RSA_PRIVATE_KEY.as_bytes()).unwrap();
        let x509 = X509::from_pem(CERTIFICATE.as_bytes()).unwrap();

        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_verify(openssl::ssl::SslVerifyMode::NONE);
        acceptor.set_private_key(&pkey).unwrap();
        acceptor.set_certificate(&x509).unwrap();
        acceptor.check_private_key().unwrap();

        server.bind_openssl(server_address(port), acceptor)
    } else {
        server.bind(server_address(port))
    }
    .unwrap();

    let server = server.workers(1).system_exit().shutdown_timeout(3).run();

    let server_handle = server.handle();

    let fut = async move { server.await.unwrap() };

    let thread_handle = thread::spawn(move || {
        System::new().block_on(fut);
    });

    loop {
        if TcpStream::connect(server_address(port)).is_ok() {
            break;
        } else {
            thread::sleep(Duration::from_millis(100));
        }
    }

    Serve::new(server_handle, thread_handle)
}

pub struct Serve {
    server_handle: ServerHandle,
    thread_handle: Option<thread::JoinHandle<()>>,
    stopped: bool,
}

impl Serve {
    fn new(server_handle: ServerHandle, thread_handle: thread::JoinHandle<()>) -> Self {
        let stopped = false;
        let thread_handle = Some(thread_handle);
        Serve {
            server_handle,
            thread_handle,
            stopped,
        }
    }

    pub fn stop(&mut self) {
        System::new().block_on(async { self.server_handle.stop(false).await });
        self.thread_handle.take().unwrap().join().unwrap();
        self.stopped = true;
    }
}

impl Drop for Serve {
    fn drop(&mut self) {
        if !self.stopped {
            self.stop();
        }
    }
}

pub fn server_address(port: u16) -> String {
    format!("localhost:{port}")
}

/*******************************************************************************
*  File handling
*/

pub fn create_tmp_file(
    tmp_dir: &tempfile::TempDir,
    name: &str,
    contents: &str,
    mode: Option<u32>,
) -> String {
    let path = tmp_dir.path().join(name).to_str().unwrap().to_string();
    create_file(&path, contents, mode);
    path
}

fn create_file(path: &str, contents: &str, mode: Option<u32>) {
    let mut open_options = OpenOptions::new();

    open_options.create(true).write(true);

    if let Some(mode) = mode {
        open_options.mode(mode);
    }

    let mut file = open_options.open(path).unwrap();

    let unindented = unindent::unindent(contents);
    file.write_all(unindented.as_bytes()).unwrap();
    file.sync_all().unwrap();
}

pub fn validate_file(path: &str, expected: &str, mode: Option<u32>) {
    let mut file = File::open(path).unwrap();

    if let Some(mode) = mode {
        let metadata = file.metadata().unwrap();
        let read_mode = metadata.permissions().mode();
        assert_eq!(mode & read_mode, mode);
    }

    let mut read_contents = String::new();
    file.read_to_string(&mut read_contents).unwrap();

    assert_eq!(&read_contents, expected);
}

pub fn validate_json_file(path: &str, expected: &str, erase_api_key: bool) {
    let mut file = File::open(path).unwrap();
    let mut read_contents = String::new();
    file.read_to_string(&mut read_contents).unwrap();

    let mut read_json: serde_json::Value = serde_json::from_str(&read_contents).unwrap();

    if erase_api_key {
        let option = read_json.as_object_mut().unwrap().remove("deviceApiKey");
        if let Some(value) = option {
            assert_eq!(value.as_str().unwrap().len(), 32);
        }

        let api_endpoint = read_json
            .get("apiEndpoint")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let pos = api_endpoint.find("://").unwrap();
        let key = &api_endpoint[pos + 3..];

        let option = read_json
            .as_object_mut()
            .unwrap()
            .get_mut("deviceApiKeys")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        if let Some(value) = option {
            assert_eq!(value.as_str().unwrap().len(), 32);
        }
    }

    let expected_json: serde_json::Value = serde_json::from_str(expected).unwrap();

    assert_eq!(read_json, expected_json);
}

pub fn validate_does_not_exist(path: &str) {
    assert!(!Path::new(path).exists());
}

/*******************************************************************************
*  Certificates
*/

use std::process::Command;

/**
 * Generate a private key & certificate pair using command line openssl
 */
pub fn generate_self_signed_cert() -> (String, String) {
    let output = Command::new("openssl")
        .args([
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-x509",
            "-subj",
            "/CN=localhost",
            "-keyout",
            "/dev/stdout",
            "-out",
            "/dev/stdout",
        ])
        .output()
        .expect("Failed to generate certificate");

    let output_str = String::from_utf8_lossy(&output.stdout).to_string();

    let private_key = extract_substring(
        &output_str,
        "-----BEGIN PRIVATE KEY-----",
        "-----END PRIVATE KEY-----",
    )
    .unwrap_or_else(|| panic!("Failed to extract private key"));

    let certificate = extract_substring(
        &output_str,
        "-----BEGIN CERTIFICATE-----",
        "-----END CERTIFICATE-----",
    )
    .unwrap_or_else(|| panic!("Failed to extract certificate"));

    (private_key, certificate)
}

/**
 * Extract a substring from a string, returning the substring with start & end included
 */
fn extract_substring(input: &str, start: &str, end: &str) -> Option<String> {
    input.find(start).map(|start_idx| {
        let end_idx = input.find(end).unwrap_or_else(|| {
            panic!("Failed to get end index for substring");
        }) + end.len()
            - 1;
        input[start_idx..=end_idx].to_owned()
    })
}

pub const CERTIFICATE: &str = "-----BEGIN CERTIFICATE-----
MIIC+zCCAeOgAwIBAgIJAJ7uOrUr2fm1MA0GCSqGSIb3DQEBBQUAMBQxEjAQBgNV
BAMMCWxvY2FsaG9zdDAeFw0xODExMTkxOTUzMTBaFw0yODExMTYxOTUzMTBaMBQx
EjAQBgNVBAMMCWxvY2FsaG9zdDCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoC
ggEBALwFQy0+iRhdyTx+XsGfW+h1TrMYFZKhl8vLAcYS9HZrRcgt3ySdtNroad3g
bVHApErI6glgbcJALhtjuRU5nc4otEqm/b/WxKlBjegxbVhUVJ/eQaQzvdJ0rpkZ
lMRnc7Nr/4YrOnZZv6/ziFTKbT9dCZwElPWGXUKIYY3Fjr4+wK7vEIv7QWnLVQNz
Uwnf6OILMToqM8RhZ/7AG22EezorpMwiCkmSygdzkGqvSxJgiwdhha+dmLKASkTD
+ZAoBeierNEKLAb+LURXOcB0Arjodq6BMsIK0QZvqQRMBopjueNYwUGIvGmLsaaM
JJjl53BUYiee/O80NRa9GOufNnkCAwEAAaNQME4wHQYDVR0OBBYEFB+jF/2dZmYL
xCuKeEt1+SflDgxaMB8GA1UdIwQYMBaAFB+jF/2dZmYLxCuKeEt1+SflDgxaMAwG
A1UdEwQFMAMBAf8wDQYJKoZIhvcNAQEFBQADggEBAISEthY0fjAZCfvkvBBceA3p
+7EZx5bk2BbiReZvIeyhlMXwFvI02mgvRpuqghgzwxNsTalO/dY2SsQDiZWBKMwn
oNN1lMUwcZMZpF7YjqJIegh105oyT40+BK4TyaCMh8t1ibLBIQcfaE17zFFqCPb6
dU5hfMAEU8xhg2lCGexrdm1xspFppEfeQjNWRSUZ8/l8qSrI4lT5FIKCrGO0t4do
GCiaNJgTtjOliVzkC8CfuP7fCYhImSy5Yq2nYscE51zdYN4wzWxek1g5o6cso9DB
FCirmU50c+3sgivNBlkSCQYdJ8pkrsTLOr6jmKiF67GWomNZ2TgAOCuByvAw1e0=
-----END CERTIFICATE-----";

pub const RSA_PRIVATE_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAvAVDLT6JGF3JPH5ewZ9b6HVOsxgVkqGXy8sBxhL0dmtFyC3f
JJ202uhp3eBtUcCkSsjqCWBtwkAuG2O5FTmdzii0Sqb9v9bEqUGN6DFtWFRUn95B
pDO90nSumRmUxGdzs2v/his6dlm/r/OIVMptP10JnASU9YZdQohhjcWOvj7Aru8Q
i/tBactVA3NTCd/o4gsxOiozxGFn/sAbbYR7OiukzCIKSZLKB3OQaq9LEmCLB2GF
r52YsoBKRMP5kCgF6J6s0QosBv4tRFc5wHQCuOh2roEywgrRBm+pBEwGimO541jB
QYi8aYuxpowkmOXncFRiJ5787zQ1Fr0Y6582eQIDAQABAoIBAFlg3wg4/A7bNnhN
UloUmSot6ZV1U3v62SAFhvhTtmY8pFV+iN7tITYW2YyhzRXZz7/FNovyjPqUa9aV
VzxhwURpURtTurhhLeePxBemt2YP4JKGowmdlxTeZslcwb2DuBqIslVjY00zaM4J
pLs55ykB3zmNbAozL04batRsH2kLtmqQKslG5MFO6v5yrI+TtmWXOyq9PTlyLnzv
LHBsomb37SMVtbaqMyzY4sebOp/qAxvX/TUfG/bAuOGB5YoUhVotUX9Uk+GTWR/y
6/PYG3f4dlgh9aELJr+x1yECi5C2kgQUZGBU17XmyW57NhyNquo8VChWmHUCXbAJ
+5Qeq4kCgYEA7YLbgTKP4+zvA6zpgM0726NdKfSi+MXP23iXZOhd2VvOX9ThlTsa
TKY8XP2enS6p0t4kxfwVVOBQZQv13n4LY8GXKfpNSpks9FIGaMVJRVFsc+/IsJ1H
ai2y4Mp2VlRS7Pk+23Mwg5qkuby2+m4zp2lGa3uzvmHNkve4dZ4mXbsCgYEAyqgm
6KBBMav27DSWYqUHyvhkfoly2huDumIvzSdZuGdPagzW+WJJiaoFGF7ucb8PQo6c
1SfWJ80nCkvUcNvk3AM2jJyslYROVRrG6lVPQ0/RvzHCeDmxxO1XaE6o7re3V292
sxEboKsFisL1t3CHOp8Ua0kCju/JlGDsUWii31sCgYEA2T+63GCNcWSF9AyzwVb5
C5xQWVIlx/vYdt3FTU2mmmz5RnsIpGHdWoMr77sk3I2UVQdRB6/fKzXLE8Ju8UbF
0EeBp6oGDNgzYH+u0SK0NK2X0Cxim/ohGqQWXLuUpr6W45/QuRaSJ67KQgK2NDed
E+KdwS7zaI85ZNcmaJ9yZIUCgYATHpAlLFFaRVYTbNavUdCNZqfchE0wpJ3l7LOD
0G2Xhy+n2rRBbPNxKHg4l2Q5mQPwjJHhTlPXB3TidMsDJsvNsgPoejOSG5xkTRVt
MEU9HX+1YRVu0EqkQJwZfCpV80E534s8U6Xen6PzNneGKfioIDAF+yphn9/NvuMs
vwl2twKBgQDoK4MGuAAogSqRCorhb+WgwIPOkP0m8x5SIHNUTU05kBlKPU6tNPNC
nMUUIGvBB46Dm1drJabo4dA/qrk+NygHkjJslZV2qj/GAl5yiyU7ab5FMaXQI1Y9
rXj1PV+HFXivKmGYbTPXAcY4jtrEKN4n+d2k8R7vYC4PD5xFdlsRdA==
-----END RSA PRIVATE KEY-----";

pub fn cert_for_json(cert: &str) -> String {
    STANDARD.encode(cert)
}
