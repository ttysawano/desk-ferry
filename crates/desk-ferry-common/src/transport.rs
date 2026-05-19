use std::{
    fs::File,
    io::{BufReader, Read, Write},
    net::TcpStream,
    path::Path,
    sync::Arc,
};

use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
    ClientConfig, ClientConnection, DigitallySignedStruct, Error as RustlsError, RootCertStore,
    ServerConfig, ServerConnection, SignatureScheme, StreamOwned,
};

use crate::{
    error::{DeskFerryError, Result},
    protocol::{decode_json_line, encode_json_line, ProtocolMessage},
    security::{certificate_fingerprint_sha256, normalize_fingerprint},
};

pub type TlsServerStream = StreamOwned<ServerConnection, TcpStream>;
pub type TlsClientStream = StreamOwned<ClientConnection, TcpStream>;

#[derive(Debug)]
struct FingerprintVerifier {
    expected: String,
}

impl FingerprintVerifier {
    fn new(expected: &str) -> Result<Self> {
        let expected = normalize_fingerprint(expected);
        if expected.is_empty() {
            return Err(DeskFerryError::SecurityPolicy(
                "server certificate fingerprint is required".to_string(),
            ));
        }
        Ok(Self { expected })
    }
}

impl ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, RustlsError> {
        let actual = certificate_fingerprint_sha256(end_entity.as_ref());
        if actual == self.expected {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(RustlsError::General(
                "server certificate fingerprint mismatch".to_string(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

pub fn load_certs(path: impl AsRef<Path>) -> Result<Vec<CertificateDer<'static>>> {
    let mut reader = BufReader::new(File::open(path)?);
    rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| DeskFerryError::Tls(error.to_string()))
}

pub fn load_private_key(path: impl AsRef<Path>) -> Result<PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(File::open(path)?);
    rustls_pemfile::private_key(&mut reader)
        .map_err(|error| DeskFerryError::Tls(error.to_string()))?
        .ok_or_else(|| DeskFerryError::Tls("no private key found".to_string()))
}

pub fn server_config_from_pem(
    cert_path: impl AsRef<Path>,
    key_path: impl AsRef<Path>,
) -> Result<Arc<ServerConfig>> {
    let certs = load_certs(cert_path)?;
    let key = load_private_key(key_path)?;
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| DeskFerryError::Tls(error.to_string()))?;
    Ok(Arc::new(config))
}

pub fn client_config_with_fingerprint(expected_fingerprint: &str) -> Result<Arc<ClientConfig>> {
    let verifier = FingerprintVerifier::new(expected_fingerprint)?;
    let mut config = ClientConfig::builder()
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();
    config
        .dangerous()
        .set_certificate_verifier(Arc::new(verifier));
    Ok(Arc::new(config))
}

pub fn accept_tls(stream: TcpStream, config: Arc<ServerConfig>) -> Result<TlsServerStream> {
    let connection =
        ServerConnection::new(config).map_err(|error| DeskFerryError::Tls(error.to_string()))?;
    Ok(StreamOwned::new(connection, stream))
}

pub fn connect_tls(
    stream: TcpStream,
    server_name: &str,
    expected_fingerprint: &str,
) -> Result<TlsClientStream> {
    let config = client_config_with_fingerprint(expected_fingerprint)?;
    let server_name = ServerName::try_from(server_name.to_string())
        .map_err(|_| DeskFerryError::Tls("invalid TLS server name".to_string()))?;
    let connection = ClientConnection::new(config, server_name)
        .map_err(|error| DeskFerryError::Tls(error.to_string()))?;
    Ok(StreamOwned::new(connection, stream))
}

pub fn send_message(stream: &mut impl Write, message: &ProtocolMessage) -> Result<()> {
    let line = encode_json_line(message)?;
    stream.write_all(line.as_bytes())?;
    stream.flush()?;
    Ok(())
}

pub fn read_message(stream: &mut impl Read) -> Result<ProtocolMessage> {
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        let read = stream.read(&mut byte)?;
        if read == 0 {
            return Err(DeskFerryError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed",
            )));
        }
        bytes.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    let line = String::from_utf8(bytes).map_err(|_| DeskFerryError::InvalidJsonLine)?;
    decode_json_line(&line)
}
