use std::task::{Context, Poll};

use futures::{StreamExt, future::BoxFuture, stream::FuturesUnordered};
use libp2p_core::upgrade::{DeniedUpgrade, ReadyUpgrade};
use libp2p_swarm::{
    ConnectionHandler, ConnectionHandlerEvent, StreamProtocol, SubstreamProtocol,
    handler::{
        ConnectionEvent, DialUpgradeError, FullyNegotiatedInbound, FullyNegotiatedOutbound,
        ListenUpgradeError,
    },
};
use void::Void;

#[derive(Debug)]
pub struct Event {}

pub struct Handler {
    inbound: FuturesUnordered<BoxFuture<'static, Result<(), std::io::Error>>>,
}

impl Handler {
    pub fn new() -> Self {
        Self {
            inbound: FuturesUnordered::new(),
        }
    }
}

impl Default for Handler {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionHandler for Handler {
    type FromBehaviour = Void;
    type ToBehaviour = Event;
    type InboundProtocol = ReadyUpgrade<StreamProtocol>;
    type OutboundProtocol = DeniedUpgrade;
    type OutboundOpenInfo = Void;
    type InboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(
            ReadyUpgrade::new(StreamProtocol::new(crate::perf::PROTOCOL_NAME)),
            (),
        )
    }

    fn on_behaviour_event(&mut self, v: Self::FromBehaviour) {
        void::unreachable(v)
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol,
                info: _,
            }) => {
                let future = Box::pin(async move { crate::perf::server_mode(protocol).await });
                self.inbound.push(future);
            }
            // TODO: remove when Rust 1.82 is MSRV
            #[allow(unreachable_patterns)]
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound { info, .. }) => {
                void::unreachable(info)
            }

            // TODO: remove when Rust 1.82 is MSRV
            #[allow(unreachable_patterns)]
            ConnectionEvent::DialUpgradeError(DialUpgradeError { info, .. }) => {
                void::unreachable(info)
            }
            ConnectionEvent::AddressChange(_)
            | ConnectionEvent::LocalProtocolsChange(_)
            | ConnectionEvent::RemoteProtocolsChange(_) => {}
            // TODO: remove when Rust 1.82 is MSRV
            #[allow(unreachable_patterns)]
            ConnectionEvent::ListenUpgradeError(ListenUpgradeError { info: (), error }) => {
                panic!(
                    "ListenUpgradeError should not occur in this context: {:?}",
                    error
                );
            }
            _ => {}
        }
    }

    #[tracing::instrument(level = "trace", name = "ConnectionHandler::poll", skip(self, cx))]
    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        if let Poll::Ready(Some(_res)) = self.inbound.poll_next_unpin(cx) {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Event {}));
        }

        Poll::Pending
    }
}
