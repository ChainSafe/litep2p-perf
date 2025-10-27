use core::panic;
use std::{
    collections::VecDeque,
    task::{Context, Poll},
};

use futures::{StreamExt, future::BoxFuture, stream::FuturesUnordered};
use libp2p_core::upgrade::{DeniedUpgrade, ReadyUpgrade};
use libp2p_swarm::{
    ConnectionHandler, ConnectionHandlerEvent, StreamProtocol, SubstreamProtocol,
    handler::{
        ConnectionEvent, DialUpgradeError, FullyNegotiatedInbound, FullyNegotiatedOutbound,
        ListenUpgradeError,
    },
};

#[derive(Debug)]
pub struct Command {
    pub id: usize,
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

#[derive(Debug)]
pub struct Event {
    pub(crate) id: usize,
    pub(crate) result: Result<(), String>,
}

pub struct Handler {
    /// Queue of events to return when polled.
    queued_events: VecDeque<
        ConnectionHandlerEvent<
            <Self as ConnectionHandler>::OutboundProtocol,
            (),
            <Self as ConnectionHandler>::ToBehaviour,
        >,
    >,

    requested_streams: VecDeque<Command>,

    outbound: FuturesUnordered<BoxFuture<'static, (usize, Result<(), std::io::Error>)>>,
}

impl Handler {
    pub fn new() -> Self {
        Self {
            queued_events: Default::default(),
            requested_streams: Default::default(),
            outbound: FuturesUnordered::new(),
        }
    }
}

impl Default for Handler {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionHandler for Handler {
    type FromBehaviour = Command;
    type ToBehaviour = Event;
    type InboundProtocol = DeniedUpgrade;
    type OutboundProtocol = ReadyUpgrade<StreamProtocol>;
    type OutboundOpenInfo = ();
    type InboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(DeniedUpgrade, ())
    }

    fn on_behaviour_event(&mut self, command: Self::FromBehaviour) {
        self.requested_streams.push_back(command);

        self.queued_events
            .push_back(ConnectionHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(
                    ReadyUpgrade::new(StreamProtocol::new(crate::perf::PROTOCOL_NAME)),
                    (),
                ),
            })
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
            // TODO: remove when Rust 1.82 is MSRV
            #[allow(unreachable_patterns)]
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol, ..
            }) => panic!(
                "Unexpected FullyNegotiatedInbound event in server handler: {:?}",
                protocol
            ),
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol,
                info: (),
            }) => {
                let Command {
                    id,
                    upload_bytes,
                    download_bytes,
                } = self
                    .requested_streams
                    .pop_front()
                    .expect("opened a stream without a pending command");

                let future = Box::pin(async move {
                    let result =
                        crate::perf::client_mode(protocol, upload_bytes, download_bytes).await;
                    (id, result)
                });

                self.outbound.push(future);
            }

            ConnectionEvent::AddressChange(_)
            | ConnectionEvent::LocalProtocolsChange(_)
            | ConnectionEvent::RemoteProtocolsChange(_) => {}
            ConnectionEvent::DialUpgradeError(DialUpgradeError { info: (), error }) => {
                let Command { id, .. } = self
                    .requested_streams
                    .pop_front()
                    .expect("requested stream without pending command");
                self.queued_events
                    .push_back(ConnectionHandlerEvent::NotifyBehaviour(Event {
                        id,
                        result: Err(error.to_string()),
                    }));
            }
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
    ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, (), Self::ToBehaviour>> {
        if let Some(event) = self.queued_events.pop_front() {
            return Poll::Ready(event);
        }

        if let Poll::Ready(Some((id, result))) = self.outbound.poll_next_unpin(cx) {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Event {
                id,
                result: result.map_err(|err| err.to_string()),
            }));
        }

        Poll::Pending
    }
}
