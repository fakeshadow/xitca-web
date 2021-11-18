//! A Http server returns Hello World String as Response from multiple services.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{convert::Infallible, io};

use h3_quinn::quinn::{
    crypto::rustls::TlsSession, generic::ServerConfig, CertificateChain, PrivateKey, ServerConfigBuilder,
};
use openssl::ssl::{AlpnError, SslAcceptor, SslFiletype, SslMethod};
use xitca_http::{
    bytes::Bytes,
    h1, h2, h3,
    http::{header, Request, Response, Version},
    util::Logger,
    HttpServiceBuilder, ResponseBody,
};
use xitca_io::net::TcpStream;
use xitca_service::{fn_service, ServiceFactoryExt};

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("xitca=info,[xitca_http_logger]=trace")
        .init();

    // construct http2 openssl config.
    let acceptor = h2_config()?;

    // construct http3 quic server config
    let config = h3_config()?;

    let h1_factory = || HttpServiceBuilder::h1(fn_service(handler_h1)).with_logger();
    let h2_factory = move || HttpServiceBuilder::h2(fn_service(handler_h2)).openssl(acceptor.clone());
    let h3_factory = || HttpServiceBuilder::h3(fn_service(handler_h3)).transform(Logger::new());

    // construct server
    xitca_server::Builder::new()
        // bind to a http/1 service.
        .bind::<_, _, _, TcpStream>("http/1", "127.0.0.1:8080", h1_factory)?
        // bind to a http/2 service.
        // *. http/1 and http/2 both use tcp listener so it should be using a separate port.
        .bind::<_, _, _, TcpStream>("http/2", "127.0.0.1:8081", h2_factory)?
        // bind to a http/3 service.
        // *. note the service name must be unique.
        //
        // Bind to same service with different bind_xxx API is allowed for reusing one service
        // on multiple socket addresses and protocols.
        .bind_h3("http/3", "127.0.0.1:8080", config, h3_factory)?
        .build()
        .await
}

async fn handler_h1(_: Request<h1::RequestBody>) -> Result<Response<ResponseBody>, Infallible> {
    Ok(Response::builder()
        .header(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/plain; charset=utf-8"),
        )
        .body(Bytes::from_static(b"Hello World from Http/1!").into())
        .unwrap())
}

async fn handler_h2(_: Request<h2::RequestBody>) -> Result<Response<ResponseBody>, Box<dyn std::error::Error>> {
    let res = Response::builder()
        .status(200)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Bytes::from_static(b"Hello World from Http/2!").into())?;
    Ok(res)
}

async fn handler_h3(_: Request<h3::RequestBody>) -> Result<Response<ResponseBody>, Box<dyn std::error::Error>> {
    Response::builder()
        .status(200)
        .version(Version::HTTP_3)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Bytes::from_static(b"Hello World from Http/3!").into())
        .map_err(Into::into)
}

fn h2_config() -> io::Result<SslAcceptor> {
    // set up openssl and alpn protocol.
    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
    builder.set_private_key_file("./cert/key.pem", SslFiletype::PEM)?;
    builder.set_certificate_chain_file("./cert/cert.pem")?;

    builder.set_alpn_select_callback(|_, protocols| {
        const H2: &[u8] = b"\x02h2";
        const H11: &[u8] = b"\x08http/1.1";

        if protocols.windows(3).any(|window| window == H2) {
            Ok(b"h2")
        } else if protocols.windows(9).any(|window| window == H11) {
            Ok(b"http/1.1")
        } else {
            Err(AlpnError::NOACK)
        }
    });

    builder.set_alpn_protos(b"\x08http/1.1\x02h2")?;

    Ok(builder.build())
}

fn h3_config() -> io::Result<ServerConfig<TlsSession>> {
    let mut config = ServerConfigBuilder::default();
    config.protocols(&[b"h3-29", b"h3-28", b"h3-27"]);

    let key = std::fs::read("./cert/key.pem")?;
    let cert = std::fs::read("./cert/cert.pem")?;

    let key = PrivateKey::from_pem(&key).unwrap();
    let cert = CertificateChain::from_pem(&cert).unwrap();

    config.certificate(cert, key).unwrap();

    Ok(config.build())
}
