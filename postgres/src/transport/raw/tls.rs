use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use std::io;

use rustls::{client::ClientConnection, StreamOwned};
use xitca_io::io::{AsyncIo, Interest, Ready};

use crate::{error::Error, transport::tls::dangerous_config};

pub(super) async fn connect<Io>(io: Io, host: &str) -> Result<TlsStream<Io>, Error>
where
    Io: AsyncIo,
{
    let name = host.try_into().map_err(|_| Error::ToDo)?;
    let config = dangerous_config(Vec::new());
    let session = ClientConnection::new(config, name).map_err(|_| Error::ToDo)?;
    Ok(TlsStream {
        io: StreamOwned::new(session, io),
    })
}

pub(super) struct TlsStream<Io>
where
    Io: AsyncIo,
{
    io: StreamOwned<ClientConnection, Io>,
}

impl<Io> AsyncIo for TlsStream<Io>
where
    Io: AsyncIo,
{
    type Future<'f> = impl Future<Output = io::Result<Ready>> + 'f where Self: 'f;

    #[inline]
    fn ready(&self, interest: Interest) -> Self::Future<'_> {
        self.io.get_ref().ready(interest)
    }

    #[inline]
    fn poll_ready(&self, interest: Interest, cx: &mut Context<'_>) -> Poll<io::Result<Ready>> {
        self.io.get_ref().poll_ready(interest, cx)
    }

    fn is_vectored_write(&self) -> bool {
        self.io.get_ref().is_vectored_write()
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        AsyncIo::poll_shutdown(Pin::new(self.get_mut().io.get_mut()), cx)
    }
}

impl<Io> io::Read for TlsStream<Io>
where
    Io: AsyncIo,
{
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut self.io, buf)
    }
}

impl<Io> io::Write for TlsStream<Io>
where
    Io: AsyncIo,
{
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut self.io, buf)
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        io::Write::write_vectored(&mut self.io, bufs)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut self.io)
    }
}