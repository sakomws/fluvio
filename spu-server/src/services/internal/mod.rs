mod api;
mod fetch_stream;
mod service_impl;
mod fetch_stream_request;

use log::info;
use std::net::SocketAddr;

use kf_service::KfApiServer;
use service_impl::SpunternalService;

use crate::core::DefaultSharedGlobalContext;

pub use self::fetch_stream_request::FetchStreamRequest;
pub use self::fetch_stream_request::FetchStreamResponse;
pub use self::api::KfSPUPeerApiEnum;
pub use self::api::SpuPeerRequest;

pub(crate) type InternalApiServer = KfApiServer<
        SpuPeerRequest,
        KfSPUPeerApiEnum,
        DefaultSharedGlobalContext,
        SpunternalService>;

// start server
pub fn create_internal_server(addr: SocketAddr, ctx: DefaultSharedGlobalContext) -> InternalApiServer
 {
    info!("starting SPU: {} at internal service at: {}", ctx.local_spu_id(),addr);

    KfApiServer::new(addr, ctx, SpunternalService::new())
}
