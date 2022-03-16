use core::future::Future;

use crate::{async_closure::AsyncClosure, factory::pipeline::marker};

use super::{pipeline::PipelineService, Service};

impl<S, Req, T, Fut, Res, Err> Service<Req> for PipelineService<S, T, marker::EnclosedFn>
where
    S: Service<Req> + Clone,
    T: Fn(S, Req) -> Fut + Clone,
    Fut: Future<Output = Result<Res, Err>>,
    Err: From<S::Error>,
{
    type Response = Res;
    type Error = Err;
    type Future<'f> = Fut where Self: 'f;

    #[inline]
    fn call(&self, req: Req) -> Self::Future<'_> {
        (self.service2)(self.service.clone(), req)
    }
}

impl<S, Req, T, Res, Err> Service<Req> for PipelineService<S, T, marker::EnclosedFn2>
where
    S: Service<Req>,
    T: for<'s> AsyncClosure<'s, S, Req, Output = Result<Res, Err>>,
    Err: From<S::Error>,
{
    type Response = Res;
    type Error = Err;
    type Future<'f> = impl Future<Output = Result<Res, Err>> where Self: 'f;

    #[inline]
    fn call(&self, req: Req) -> Self::Future<'_> {
        self.service2.call(&self.service, req)
    }
}