use core::future::Future;

use crate::service::pipeline::PipelineService;

use super::{
    pipeline::{marker, PipelineServiceFactory},
    ServiceFactory,
};

impl<SF, Req, Arg, SF1, Res> ServiceFactory<Req, Arg> for PipelineServiceFactory<SF, SF1, marker::Map>
where
    SF: ServiceFactory<Req, Arg>,
    SF1: Fn(Result<SF::Response, SF::Error>) -> Result<Res, SF::Error> + Clone,
{
    type Response = Res;
    type Error = SF::Error;
    type Service = PipelineService<SF::Service, SF1, marker::Map>;
    type Future = impl Future<Output = Result<Self::Service, Self::Error>>;

    fn new_service(&self, arg: Arg) -> Self::Future {
        let service = self.factory.new_service(arg);
        let mapper = self.factory2.clone();

        async move {
            let service = service.await?;
            Ok(PipelineService::new(service, mapper))
        }
    }
}
