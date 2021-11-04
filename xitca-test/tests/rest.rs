use xitca_http::{
    body::ResponseBody,
    h1,
    http::{header, Method, Request, Response},
};
use xitca_io::bytes::Bytes;
use xitca_service::fn_service;
use xitca_test::{test_h1_server, Error};

#[tokio::test]
async fn h1_get() -> Result<(), Error> {
    let mut handle = test_h1_server(|| fn_service(handle))?;

    let server_url = format!("http://{}/", handle.ip_port_string());

    let c = xitca_client::Client::new();

    let res = c.get(&server_url)?.send().await?;
    assert_eq!(res.status().as_u16(), 200);
    let body = res.string().await?;
    assert_eq!("GET Response", body);

    for _ in 0..3 {
        let mut req = c.post(&server_url)?.body(Bytes::from("Hello,World!"));
        req.headers_mut()
            .insert(header::CONTENT_TYPE, header::HeaderValue::from_static("text/plain"));
        let res = req.send().await?;
        assert_eq!(res.status().as_u16(), 200);
        let body = res.string().await?;
        assert_eq!("Hello,World!", body);
    }

    handle.try_handle()?.stop(false);

    handle.await?;

    Ok(())
}

async fn handle(req: Request<h1::RequestBody>) -> Result<Response<ResponseBody>, Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::new(Bytes::from("GET Response").into())),
        (&Method::POST, "/") => {
            let length = req.headers().get(header::CONTENT_LENGTH).unwrap().clone();
            let ty = req.headers().get(header::CONTENT_TYPE).unwrap().clone();
            let body = req.into_body();
            let mut res = Response::new(ResponseBody::stream(Box::pin(body) as _));

            res.headers_mut().insert(header::CONTENT_LENGTH, length);
            res.headers_mut().insert(header::CONTENT_TYPE, ty);

            Ok(res)
        }
        _ => todo!(),
    }
}
